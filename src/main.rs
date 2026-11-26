mod epub;
mod store;

use anyhow::Context as _;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{button::Button, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_webview::WebView;
use percent_encoding::percent_decode_str;
use rust_embed::RustEmbed;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use store::{HistoryEntry, Store};

actions!(orange_reader, [OpenFile, ShowHistory, CloseWindow, QuitApp]);

#[derive(RustEmbed)]
#[folder = "foliate-js/"]
struct FoliateAssets;

struct AppState {
    store: Rc<Store>,
    history_window: Option<(AnyWindowHandle, WeakEntity<HistoryView>)>,
    open_books: HashMap<i64, AnyWindowHandle>,
    last_error: Option<String>,
}

impl Global for AppState {}

fn is_epub_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
}

fn file_url_to_path(raw: &str) -> Option<PathBuf> {
    let path = if let Some(rest) = raw.strip_prefix("file://") {
        let without_host = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode_str(without_host).decode_utf8_lossy();
        PathBuf::from(decoded.into_owned())
    } else {
        PathBuf::from(raw)
    };
    is_epub_path(&path).then_some(path)
}

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}

struct ReaderServer {
    url: String,
    running: Arc<AtomicBool>,
}

impl Drop for ReaderServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct ReaderSettings {
    font_size: f32,
    line_height: f32,
    content_width: f32,
    letter_spacing: f32,
    paragraph_spacing: f32,
    font_family: String,
    dark: bool,
}

impl ReaderSettings {
    fn load(store: &Store) -> Self {
        let number = |key: &str, default: f32, min: f32, max: f32| {
            store
                .setting(key)
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
                .clamp(min, max)
        };
        Self {
            font_size: number("font_size", 18.0, 12.0, 32.0),
            line_height: number("line_height", 1.6, 1.0, 2.4),
            content_width: number("content_width", 720.0, 480.0, 960.0),
            letter_spacing: number("letter_spacing", 0.0, 0.0, 0.1),
            paragraph_spacing: number("paragraph_spacing", 1.0, 0.5, 2.5),
            font_family: store
                .setting("font_family")
                .filter(|family| matches!(family.as_str(), "original" | "serif" | "sans-serif"))
                .unwrap_or_else(|| "original".to_owned()),
            dark: store.setting("theme").as_deref() == Some("dark"),
        }
    }

    fn font_family_label(&self) -> &'static str {
        match self.font_family.as_str() {
            "serif" => "字体：衬线",
            "sans-serif" => "字体：无衬线",
            _ => "字体：原版",
        }
    }

    fn json(&self) -> Value {
        json!({
            "fontSize": self.font_size,
            "lineHeight": self.line_height,
            "contentWidth": self.content_width,
            "letterSpacing": self.letter_spacing,
            "paragraphSpacing": self.paragraph_spacing,
            "fontFamily": self.font_family,
            "dark": self.dark,
        })
    }
}

fn refresh_history(cx: &mut App) {
    let target = cx
        .global::<AppState>()
        .history_window
        .as_ref()
        .and_then(|(_, weak)| weak.upgrade());
    if let Some(view) = target {
        view.update(cx, |history, cx| history.reload(cx));
    }
}

fn open_or_focus_history_window(cx: &mut App) {
    if let Some((handle, weak)) = cx.global::<AppState>().history_window.clone() {
        let focused = handle.update(cx, |_, window, _| window.activate_window()).is_ok();
        if focused {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |history, cx| history.reload(cx));
            }
            return;
        }
        cx.global_mut::<AppState>().history_window = None;
    }

    let store = cx.global::<AppState>().store.clone();
    let error = cx.global_mut::<AppState>().last_error.take();
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Orange Reader · 最近阅读".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    match gpui_kit::open_window(options, cx, move |_, cx| {
        HistoryView::new(store, error, cx)
    }) {
        Ok((handle, entity)) => {
            cx.global_mut::<AppState>().history_window = Some((handle, entity.downgrade()));
        }
        Err(error) => eprintln!("无法打开最近阅读窗口：{error}"),
    }
}

fn open_reader_for_entry(
    entry: HistoryEntry,
    bytes: Vec<u8>,
    self_test: bool,
    cx: &mut App,
) -> anyhow::Result<()> {
    if let Some(existing_handle) = cx.global::<AppState>().open_books.get(&entry.id).copied() {
        if existing_handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return Ok(());
        }
        cx.global_mut::<AppState>().open_books.remove(&entry.id);
    }

    let store = cx.global::<AppState>().store.clone();
    let annotations: Vec<Value> = store
        .annotations(entry.id)
        .context("无法读取高亮")?
        .into_iter()
        .map(|annotation| {
            json!({
                "id": annotation.id,
                "cfi": annotation.cfi,
                "color": annotation.color,
                "type": annotation.kind,
                "text": annotation.text,
            })
        })
        .collect();
    let location = match entry.cfi.as_deref() {
        Some(cfi) if cfi.starts_with("epubcfi(") => Some(json!(cfi)),
        _ if entry.fraction.is_finite() && entry.fraction > 0.0 && entry.fraction <= 1.0 => {
            Some(json!({ "fraction": entry.fraction }))
        }
        _ => None,
    };
    let settings = ReaderSettings::load(&store);
    let session = json!({
        "title": entry.title,
        "author": entry.author,
        "selfTest": self_test,
        "location": location,
        "annotations": annotations,
        "settings": settings.json(),
    })
    .to_string();
    let server = start_server(Some(bytes), Some(session));
    let title = entry.title.clone();
    let book_id = entry.id;
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let (handle, _) = gpui_kit::open_window(options, cx, move |window, cx| {
        ReaderWindow::new(window, cx, server, store, book_id, settings)
    })?;
    cx.global_mut::<AppState>().open_books.insert(book_id, handle);
    refresh_history(cx);
    Ok(())
}

fn open_epub_path(path: PathBuf, self_test: bool, cx: &mut App) -> anyhow::Result<()> {
    let bytes = fs::read(&path).with_context(|| format!("无法读取 {}", path.display()))?;
    let parsed = epub::parse(&path).with_context(|| format!("无法解析 {}", path.display()))?;
    let store = cx.global::<AppState>().store.clone();
    let entry = store.record_open(&path, &bytes, &parsed)?;
    open_reader_for_entry(entry, bytes, self_test, cx)
}

fn open_epub_paths(paths: Vec<PathBuf>, self_test: bool, cx: &mut App) {
    let mut errors = Vec::new();
    for path in paths {
        if !is_epub_path(&path) {
            continue;
        }
        if let Err(error) = open_epub_path(path, self_test, cx) {
            eprintln!("打开 EPUB 失败：{error}");
            errors.push(error.to_string());
        }
    }
    if !errors.is_empty() {
        let message = errors.join("\n");
        if let Some(view) = cx
            .global::<AppState>()
            .history_window
            .as_ref()
            .and_then(|(_, weak)| weak.upgrade())
        {
            view.update(cx, |history, cx| history.set_error(Some(message), cx));
        } else {
            cx.global_mut::<AppState>().last_error = Some(message);
            open_or_focus_history_window(cx);
        }
    }
}

fn prompt_open_epubs(cx: &mut App) {
    let result = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("打开 EPUB".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = result.await {
            let _ = cx.update(|cx| open_epub_paths(paths, false, cx));
        }
    })
    .detach();
}

struct HistoryView {
    store: Rc<Store>,
    entries: Vec<HistoryEntry>,
    error: Option<String>,
}

impl HistoryView {
    fn new(store: Rc<Store>, initial_error: Option<String>, cx: &mut App) -> Entity<Self> {
        let (entries, load_error) = match store.history() {
            Ok(entries) => (entries, None),
            Err(error) => (Vec::new(), Some(format!("读取历史记录失败：{error}"))),
        };
        cx.new(|_| Self {
            store,
            entries,
            error: initial_error.or(load_error),
        })
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        match self.store.history() {
            Ok(entries) => {
                self.entries = entries;
            }
            Err(error) => self.error = Some(format!("读取历史记录失败：{error}")),
        }
        cx.notify();
    }

    fn set_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    fn open_entry(&mut self, entry: HistoryEntry, cx: &mut Context<Self>) {
        self.error = None;
        if let Err(error) = open_epub_path(entry.file_path, false, cx) {
            self.error = Some(error.to_string());
        }
        self.reload(cx);
    }

    fn relocate_entry(&mut self, id: i64, cx: &mut Context<Self>) {
        let result = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("重新定位 EPUB".into()),
        });
        cx.spawn(async move |view, cx| {
            let Ok(Ok(Some(paths))) = result.await else {
                return;
            };
            let Some(path) = paths.into_iter().find(|p| is_epub_path(p)) else {
                return;
            };
            let _ = view.update(cx, |this, cx| {
                let outcome = fs::read(&path)
                    .with_context(|| format!("无法读取 {}", path.display()))
                    .and_then(|bytes| {
                        let parsed = epub::parse(&path)
                            .with_context(|| format!("无法解析 {}", path.display()))?;
                        let entry = this.store.relocate_entry(id, &path, &bytes, &parsed)?;
                        open_reader_for_entry(entry, bytes, false, cx)
                    });
                match outcome {
                    Ok(()) => this.error = None,
                    Err(error) => this.error = Some(error.to_string()),
                }
                this.reload(cx);
            });
        })
        .detach();
    }

    fn remove_entry(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Err(error) = self.store.remove_history(id) {
            self.error = Some(format!("移出历史失败：{error}"));
        } else {
            self.error = None;
        }
        self.reload(cx);
    }
}

impl Render for HistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_5()
            .gap_4()
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                open_epub_paths(paths.paths().to_vec(), false, cx);
            }))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(div().text_lg().child("Orange Reader · 最近阅读"))
                            .child(div().text_sm().text_color(rgb(0x74685c)).child(
                                if self.entries.is_empty() {
                                    "直接打开磁盘上的 EPUB 阅读，不拷贝也不管理你的文件".to_owned()
                                } else {
                                    format!("共 {} 条阅读记录 · 支持拖入 EPUB 直接阅读", self.entries.len())
                                },
                            )),
                    )
                    .child(
                        Button::new("open-epub")
                            .label("打开 EPUB…")
                            .on_click(|_, _, cx| prompt_open_epubs(cx)),
                    ),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    h_flex()
                        .p_2()
                        .gap_2()
                        .rounded_md()
                        .bg(rgb(0xfee2e2))
                        .text_color(rgb(0x991b1b))
                        .justify_between()
                        .items_center()
                        .child(error)
                        .child(Button::new("dismiss-error").label("关闭").on_click(
                            cx.listener(|this, _, _, cx| this.set_error(None, cx)),
                        )),
                )
            })
            .child(if self.entries.is_empty() {
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(rgb(0x74685c))
                    .child("还没有阅读记录")
                    .child("点击右上角“打开 EPUB…”（⌘O），或直接将 .epub 文件拖入此窗口")
                    .into_any_element()
            } else {
                v_flex()
                    .flex_1()
                    .gap_3()
                    .overflow_y_scrollbar()
                    .children(self.entries.iter().cloned().map(|entry| {
                        let id = entry.id;
                        let exists = entry.file_path.exists();
                        let open_target = entry.clone();
                        let reveal_path = entry.file_path.clone();
                        let percent = (entry.fraction * 100.0).round() as u32;
                        let progress_text = match entry.chapter_title.as_deref() {
                            Some(chapter) if !chapter.is_empty() => {
                                format!("{chapter} · 已读 {percent}%")
                            }
                            _ if entry.cfi.is_some() || percent > 0 => {
                                format!("已读 {percent}%")
                            }
                            _ => "尚未开始阅读".to_owned(),
                        };
                        let location_text = if exists {
                            display_path(&entry.file_path)
                        } else {
                            format!("{}（文件已移动或删除）", display_path(&entry.file_path))
                        };

                        h_flex()
                            .id(("history", id as usize))
                            .p_3()
                            .gap_3()
                            .border_1()
                            .rounded_md()
                            .items_center()
                            .child(if let Some(path) = entry.cover_path.clone().filter(|p| p.exists()) {
                                div()
                                    .w(px(52.))
                                    .h(px(72.))
                                    .flex_shrink_0()
                                    .rounded_sm()
                                    .overflow_hidden()
                                    .child(img(path).size_full())
                                    .into_any_element()
                            } else {
                                div()
                                    .w(px(52.))
                                    .h(px(72.))
                                    .flex_shrink_0()
                                    .rounded_sm()
                                    .bg(rgb(0xe9b48b))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(entry.title.chars().next().unwrap_or('书').to_string())
                                    .into_any_element()
                            })
                            .child(
                                v_flex()
                                    .flex_1()
                                    .gap_1()
                                    .overflow_hidden()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(entry.title.clone())
                                            .when(!entry.author.is_empty(), |row| {
                                                row.child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0x74685c))
                                                        .child(entry.author.clone()),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(if exists {
                                                rgb(0x57534e)
                                            } else {
                                                rgb(0xb45309)
                                            })
                                            .child(location_text),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x78716c))
                                            .child(format!(
                                                "{progress_text} · 上次阅读 {}",
                                                entry.last_opened_at
                                            )),
                                    ),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_shrink_0()
                                    .when(exists, |actions| {
                                        actions
                                            .child(
                                                Button::new(("continue", id as usize))
                                                    .label("继续阅读")
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.open_entry(open_target.clone(), cx)
                                                    })),
                                            )
                                            .child(
                                                Button::new(("reveal", id as usize))
                                                    .label("显示位置")
                                                    .on_click(move |_, _, cx| {
                                                        cx.reveal_path(&reveal_path);
                                                    }),
                                            )
                                    })
                                    .when(!exists, |actions| {
                                        actions.child(
                                            Button::new(("relocate", id as usize))
                                                .label("重新定位")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.relocate_entry(id, cx)
                                                })),
                                        )
                                    })
                                    .child(
                                        Button::new(("remove", id as usize))
                                            .label("移出历史")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.remove_entry(id, cx)
                                            })),
                                    ),
                            )
                    }))
                    .into_any_element()
            })
    }
}

struct ReaderWindow {
    webview: Entity<WebView>,
    _server: ReaderServer,
    settings: ReaderSettings,
    store: Rc<Store>,
}

impl ReaderWindow {
    fn new(
        window: &mut Window,
        cx: &mut App,
        server: ReaderServer,
        store: Rc<Store>,
        book_id: i64,
        settings: ReaderSettings,
    ) -> Entity<Self> {
        let url = server.url.clone();
        let ipc_store = store.clone();
        let async_app = cx.to_async();
        let webview = cx.new(|cx| {
            let allowed_url = url.clone();
            let allowed_blob_url = format!("blob:{allowed_url}");
            let builder = wry::WebViewBuilder::new()
                .with_url(&url)
                .with_navigation_handler(move |next| {
                    next.starts_with(&allowed_url)
                        || next.starts_with(&allowed_blob_url)
                        || next == "about:blank"
                })
                .with_ipc_handler(move |request| {
                    let body = request.body();
                    eprintln!("[foliate-js] {body}");
                    let Ok(message) = serde_json::from_str::<Value>(body) else {
                        return;
                    };
                    if message.get("managed").and_then(Value::as_bool) != Some(true) {
                        return;
                    }
                    let cfi = message
                        .get("cfi")
                        .and_then(Value::as_str)
                        .filter(|cfi| cfi.starts_with("epubcfi(") && cfi.len() < 4096);
                    let mut should_refresh_history = false;
                    let result = match message.get("kind").and_then(Value::as_str) {
                        Some("relocate") => {
                            let fraction = message.get("fraction").and_then(Value::as_f64).filter(
                                |fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction),
                            );
                            let chapter = message
                                .get("chapter")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|text| !text.is_empty() && text.len() < 512);
                            match (cfi, fraction) {
                                (Some(cfi), Some(fraction)) => {
                                    should_refresh_history = true;
                                    ipc_store.save_progress(book_id, cfi, fraction, chapter)
                                }
                                _ => return,
                            }
                        }
                        Some("highlight") => {
                            let text = message
                                .get("text")
                                .and_then(Value::as_str)
                                .filter(|text| !text.is_empty() && text.len() < 16384);
                            let color =
                                message
                                    .get("color")
                                    .and_then(Value::as_str)
                                    .filter(|color| {
                                        matches!(
                                            *color,
                                            "#FFE066"
                                                | "#86EFAC"
                                                | "#93C5FD"
                                                | "#F9A8D4"
                                                | "#FCA5A5"
                                        )
                                    });
                            match (cfi, text, color) {
                                (Some(cfi), Some(text), Some(color)) => ipc_store
                                    .add_annotation(book_id, cfi, color, text)
                                    .map(|_| ()),
                                _ => return,
                            }
                        }
                        Some("annotation-color") => {
                            let color =
                                message
                                    .get("color")
                                    .and_then(Value::as_str)
                                    .filter(|color| {
                                        matches!(
                                            *color,
                                            "#FFE066"
                                                | "#86EFAC"
                                                | "#93C5FD"
                                                | "#F9A8D4"
                                                | "#FCA5A5"
                                        )
                                    });
                            match (cfi, color) {
                                (Some(cfi), Some(color)) => {
                                    ipc_store.recolor_annotation(book_id, cfi, color)
                                }
                                _ => return,
                            }
                        }
                        Some("annotation-delete") => match cfi {
                            Some(cfi) => ipc_store.delete_annotation(book_id, cfi),
                            None => return,
                        },
                        _ => return,
                    };
                    if let Err(error) = result {
                        eprintln!("保存阅读数据失败：{error}");
                    } else if should_refresh_history {
                        let app = async_app.clone();
                        app.spawn(async move |cx| {
                            let _ = cx.update(refresh_history);
                        })
                        .detach();
                    }
                });
            WebView::build(builder, window, cx).expect("无法创建 macOS WebView")
        });
        cx.new(|_| Self {
            webview,
            _server: server,
            settings,
            store,
        })
    }

    fn send(&self, command: &str, cx: &mut Context<Self>) {
        let script = format!("window.readerFromRust?.('{command}')");
        self.webview.update(cx, |view, _| {
            if let Err(error) = view.raw().evaluate_script(&script) {
                eprintln!("发送命令失败：{error}");
            }
        });
    }

    fn update_settings(&mut self, key: &str, cx: &mut Context<Self>) {
        match key {
            "font-smaller" => self.settings.font_size = (self.settings.font_size - 2.0).max(12.0),
            "font-larger" => self.settings.font_size = (self.settings.font_size + 2.0).min(32.0),
            "line-smaller" => {
                self.settings.line_height = (self.settings.line_height - 0.2).max(1.0)
            }
            "line-larger" => self.settings.line_height = (self.settings.line_height + 0.2).min(2.4),
            "width-smaller" => {
                self.settings.content_width = (self.settings.content_width - 80.0).max(480.0)
            }
            "width-larger" => {
                self.settings.content_width = (self.settings.content_width + 80.0).min(960.0)
            }
            "letter-smaller" => {
                self.settings.letter_spacing = (self.settings.letter_spacing - 0.01).max(0.0)
            }
            "letter-larger" => {
                self.settings.letter_spacing = (self.settings.letter_spacing + 0.01).min(0.1)
            }
            "paragraph-smaller" => {
                self.settings.paragraph_spacing = (self.settings.paragraph_spacing - 0.1).max(0.5)
            }
            "paragraph-larger" => {
                self.settings.paragraph_spacing = (self.settings.paragraph_spacing + 0.1).min(2.5)
            }
            "font-family" => {
                self.settings.font_family = match self.settings.font_family.as_str() {
                    "original" => "serif",
                    "serif" => "sans-serif",
                    _ => "original",
                }
                .to_owned()
            }
            "theme" => self.settings.dark = !self.settings.dark,
            _ => return,
        }
        let (name, value) = match key {
            "font-smaller" | "font-larger" => ("font_size", self.settings.font_size.to_string()),
            "line-smaller" | "line-larger" => {
                ("line_height", self.settings.line_height.to_string())
            }
            "width-smaller" | "width-larger" => {
                ("content_width", self.settings.content_width.to_string())
            }
            "letter-smaller" | "letter-larger" => {
                ("letter_spacing", self.settings.letter_spacing.to_string())
            }
            "paragraph-smaller" | "paragraph-larger" => (
                "paragraph_spacing",
                self.settings.paragraph_spacing.to_string(),
            ),
            "font-family" => ("font_family", self.settings.font_family.clone()),
            "theme" => (
                "theme",
                if self.settings.dark { "dark" } else { "light" }.to_owned(),
            ),
            _ => unreachable!(),
        };
        if let Err(error) = self.store.save_setting(name, &value) {
            eprintln!("保存阅读设置失败：{error}");
        }
        let script = format!("window.applyReaderSettings?.({})", self.settings.json());
        self.webview.update(cx, |view, _| {
            if let Err(error) = view.raw().evaluate_script(&script) {
                eprintln!("应用阅读设置失败：{error}");
            }
        });
        cx.notify();
    }
}

impl Render for ReaderWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                open_epub_paths(paths.paths().to_vec(), false, cx);
            }))
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(
                        Button::new("open-file")
                            .label("打开…")
                            .on_click(|_, _, cx| prompt_open_epubs(cx)),
                    )
                    .child(
                        Button::new("history")
                            .label("最近阅读")
                            .on_click(|_, _, cx| open_or_focus_history_window(cx)),
                    )
                    .child(
                        Button::new("previous")
                            .label("上一页")
                            .on_click(cx.listener(|this, _, _, cx| this.send("previous", cx))),
                    )
                    .child(
                        Button::new("next")
                            .label("下一页")
                            .on_click(cx.listener(|this, _, _, cx| this.send("next", cx))),
                    )
                    .child(
                        Button::new("toc")
                            .label("目录")
                            .on_click(cx.listener(|this, _, _, cx| this.send("toc", cx))),
                    )
                    .child(
                        Button::new("notes")
                            .label("高亮")
                            .on_click(cx.listener(|this, _, _, cx| this.send("notes", cx))),
                    )
                    .child(
                        Button::new("highlight")
                            .label("高亮选中")
                            .on_click(cx.listener(|this, _, _, cx| this.send("highlight", cx))),
                    )
                    .child(Button::new("font-smaller").label("A−").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("font-smaller", cx)),
                    ))
                    .child(format!("{}px", self.settings.font_size as u32))
                    .child(Button::new("font-larger").label("A+").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("font-larger", cx)),
                    ))
                    .child(
                        Button::new("theme")
                            .label(if self.settings.dark { "浅色" } else { "深色" })
                            .on_click(
                                cx.listener(|this, _, _, cx| this.update_settings("theme", cx)),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .px_3()
                    .pb_2()
                    .gap_2()
                    .flex_wrap()
                    .items_center()
                    .child(
                        Button::new("font-family")
                            .label(self.settings.font_family_label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_settings("font-family", cx)
                            })),
                    )
                    .child(Button::new("line-smaller").label("行距−").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("line-smaller", cx)),
                    ))
                    .child(format!("{:.1}", self.settings.line_height))
                    .child(Button::new("line-larger").label("行距+").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("line-larger", cx)),
                    ))
                    .child(Button::new("width-smaller").label("宽度−").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("width-smaller", cx)),
                    ))
                    .child(format!("{}px", self.settings.content_width as u32))
                    .child(Button::new("width-larger").label("宽度+").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("width-larger", cx)),
                    ))
                    .child(Button::new("letter-smaller").label("字距−").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("letter-smaller", cx)),
                    ))
                    .child(format!("{:.2}em", self.settings.letter_spacing))
                    .child(Button::new("letter-larger").label("字距+").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("letter-larger", cx)),
                    ))
                    .child(Button::new("paragraph-smaller").label("段距−").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("paragraph-smaller", cx)),
                    ))
                    .child(format!("{:.1}em", self.settings.paragraph_spacing))
                    .child(Button::new("paragraph-larger").label("段距+").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("paragraph-larger", cx)),
                    )),
            )
            .child(div().flex_1().child(self.webview.clone()))
    }
}

fn serve(mut stream: TcpStream, book: Option<&[u8]>, session: Option<&str>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut request = [0u8; 8192];
    let length = stream.read(&mut request)?;
    let first_line = String::from_utf8_lossy(&request[..length]);
    let path = first_line
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/");

    let (status, mime, body) = if path == "/" {
        (
            "200 OK",
            "text/html; charset=utf-8",
            include_bytes!("reader.html").to_vec(),
        )
    } else if path == "/book.epub" {
        match book {
            Some(bytes) => ("200 OK", "application/epub+zip", bytes.to_vec()),
            None => ("404 Not Found", "text/plain", Vec::new()),
        }
    } else if path == "/session.json" {
        match session {
            Some(session) => ("200 OK", "application/json", session.as_bytes().to_vec()),
            None => ("404 Not Found", "text/plain", Vec::new()),
        }
    } else if let Some(relative) = path.strip_prefix("/foliate-js/") {
        let relative = PathBuf::from(relative);
        if relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            match relative.to_str().and_then(FoliateAssets::get) {
                Some(asset) => {
                    let mime = match relative.extension().and_then(|ext| ext.to_str()) {
                        Some("js") => "text/javascript; charset=utf-8",
                        Some("css") => "text/css; charset=utf-8",
                        Some("svg") => "image/svg+xml",
                        Some("html") => "text/html; charset=utf-8",
                        _ => "application/octet-stream",
                    };
                    ("200 OK", mime, asset.data.into_owned())
                }
                None => ("404 Not Found", "text/plain", Vec::new()),
            }
        } else {
            ("403 Forbidden", "text/plain", Vec::new())
        }
    } else {
        ("404 Not Found", "text/plain", Vec::new())
    };

    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)
}

fn start_server(book: Option<Vec<u8>>, session: Option<String>) -> ReaderServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("无法启动本地阅读页服务器");
    listener
        .set_nonblocking(true)
        .expect("无法配置本地阅读页服务器");
    let address = listener.local_addr().expect("无法获取本地端口");
    let book = Arc::new(book);
    let session = Arc::new(session);
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = running.clone();
    thread::spawn(move || {
        while thread_running.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if let Err(error) = stream.set_nonblocking(false) {
                        eprintln!("无法配置资源连接：{error}");
                        continue;
                    }
                    if let Err(error) = serve(stream, book.as_deref(), session.as_deref()) {
                        eprintln!("资源请求失败：{error}");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => eprintln!("本地连接失败：{error}"),
            }
        }
    });

    let url = format!("http://{address}/");
    eprintln!("阅读页：{url}");
    ReaderServer { url, running }
}

fn configure_menus(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-shift-o", ShowHistory, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", QuitApp, None),
    ]);
    cx.on_action(|_: &OpenFile, cx| prompt_open_epubs(cx));
    cx.on_action(|_: &ShowHistory, cx| open_or_focus_history_window(cx));
    cx.on_action(|_: &CloseWindow, cx| {
        if let Some(window) = cx.active_window() {
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
    });
    cx.on_action(|_: &QuitApp, cx| cx.quit());
    cx.set_menus(vec![
        Menu::new("Orange Reader").items([MenuItem::action("退出 Orange Reader", QuitApp)]),
        Menu::new("文件").items([
            MenuItem::action("打开 EPUB…", OpenFile),
            MenuItem::action("最近阅读", ShowHistory),
            MenuItem::separator(),
            MenuItem::action("关闭窗口", CloseWindow),
        ]),
    ]);
}

fn main() {
    let mut self_test = false;
    let mut initial_paths = Vec::new();
    for arg in env::args_os().skip(1) {
        if arg == std::ffi::OsStr::new("--self-test") {
            self_test = true;
        } else {
            initial_paths.push(PathBuf::from(arg));
        }
    }

    let store = Rc::new(Store::open().expect("无法打开阅读历史数据库"));
    let pending_urls: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let async_handle: Rc<RefCell<Option<AsyncApp>>> = Rc::new(RefCell::new(None));

    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);

    let url_queue = pending_urls.clone();
    let url_app = async_handle.clone();
    app.on_open_urls(move |urls| {
        let paths: Vec<PathBuf> = urls.iter().filter_map(|u| file_url_to_path(u)).collect();
        if paths.is_empty() {
            return;
        }
        if let Some(app) = url_app.borrow().clone() {
            app.spawn(async move |cx| {
                let _ = cx.update(|cx| open_epub_paths(paths, false, cx));
            })
            .detach();
        } else {
            url_queue.borrow_mut().extend(paths);
        }
    });

    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_or_focus_history_window(cx);
        }
    });

    app.run(move |cx| {
        gpui_kit::init(cx);
        cx.set_global(AppState {
            store,
            history_window: None,
            open_books: HashMap::new(),
            last_error: None,
        });
        configure_menus(cx);
        *async_handle.borrow_mut() = Some(cx.to_async());

        let mut startup_paths = initial_paths;
        startup_paths.extend(pending_urls.borrow_mut().drain(..));
        if startup_paths.is_empty() {
            open_or_focus_history_window(cx);
        } else {
            open_epub_paths(startup_paths, self_test, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{FoliateAssets, file_url_to_path};
    use std::path::PathBuf;

    #[test]
    fn embeds_epub_renderer() {
        assert!(FoliateAssets::get("view.js").is_some());
        assert!(FoliateAssets::get("epub.js").is_some());
        assert!(FoliateAssets::get("vendor/zip.js").is_some());
    }

    #[test]
    fn decodes_macos_file_urls_for_epubs() {
        assert_eq!(
            file_url_to_path("file:///Users/demo/Books/%E4%B8%89%E4%BD%93.epub"),
            Some(PathBuf::from("/Users/demo/Books/三体.epub"))
        );
        assert_eq!(
            file_url_to_path("file://localhost/tmp/Sample%20Book.EPUB"),
            Some(PathBuf::from("/tmp/Sample Book.EPUB"))
        );
        assert_eq!(file_url_to_path("file:///tmp/notes.txt"), None);
    }
}
