#[path = "../../../src-native/epub.rs"]
mod epub;
#[path = "../../../src-native/store.rs"]
mod store;

use anyhow::Context as _;
use gpui_kit::component::{button::Button, h_flex, v_flex};
use gpui_kit::*;
use gpui_webview::WebView;
use serde_json::{Value, json};
use std::{
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
use store::{Book, Store};

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
    fn load(store: Option<&Store>) -> Self {
        let number = |key: &str, default: f32, min: f32, max: f32| {
            store
                .and_then(|store| store.setting(key))
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(default)
                .clamp(min, max)
        };
        Self {
            font_size: number("font_size", 18.0, 12.0, 32.0),
            line_height: number("line_height", 1.6, 1.0, 2.4),
            content_width: number("content_width", 720.0, 480.0, 960.0),
            letter_spacing: number("web_letter_spacing", 0.0, 0.0, 0.1),
            paragraph_spacing: number("paragraph_spacing", 1.0, 0.5, 2.5),
            font_family: store
                .and_then(|store| store.setting("web_font_family"))
                .filter(|family| matches!(family.as_str(), "original" | "serif" | "sans-serif"))
                .unwrap_or_else(|| "original".to_owned()),
            dark: store.and_then(|store| store.setting("theme")).as_deref() == Some("dark"),
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

fn open_stored_book(store: Rc<Store>, book: Book, cx: &mut App) -> anyhow::Result<()> {
    let bytes = fs::read(&book.file_path).with_context(|| format!("无法读取《{}》", book.title))?;
    let progress = store.web_progress(book.id).context("无法读取阅读进度")?;
    let annotations = store.web_annotations(book.id).context("无法读取高亮")?;
    let annotations: Vec<Value> = annotations
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
    let location = progress.and_then(|(cfi, fraction)| {
        if cfi.starts_with("epubcfi(") {
            Some(json!(cfi))
        } else if fraction.is_finite() && (0.0..=1.0).contains(&fraction) {
            Some(json!({ "fraction": fraction }))
        } else {
            None
        }
    });
    let settings = ReaderSettings::load(Some(&store));
    let session = json!({ "location": location, "annotations": annotations,
        "settings": settings.json() })
    .to_string();
    let server = start_server(Some(bytes), Some(session));
    let persistence = Some((store, book.id));
    gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
        Spike::new(window, cx, server, persistence, settings)
    })?;
    Ok(())
}

struct LibraryView {
    store: Rc<Store>,
    books: Vec<Book>,
    error: Option<String>,
    pending_delete: Option<i64>,
}

impl LibraryView {
    fn new(store: Rc<Store>, cx: &mut App) -> Entity<Self> {
        let books = store.books().unwrap_or_default();
        cx.new(|_| Self {
            store,
            books,
            error: None,
            pending_delete: None,
        })
    }

    fn open_book(&mut self, book: Book, _: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = open_stored_book(self.store.clone(), book, cx) {
            self.error = Some(error.to_string());
            cx.notify();
        }
    }

    fn import_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut errors = Vec::new();
        for path in paths {
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
            {
                continue;
            }
            match epub::parse(&path).and_then(|epub| self.store.import(&path, &epub)) {
                Ok(()) => {}
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        self.books = self.store.books().unwrap_or_default();
        self.error = (!errors.is_empty()).then(|| errors.join("\n"));
        cx.notify();
    }

    fn choose_files(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let result = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("导入 EPUB".into()),
        });
        cx.spawn(async move |view, cx| {
            if let Ok(Ok(Some(paths))) = result.await {
                let _ = view.update(cx, |library, cx| library.import_paths(paths, cx));
            }
        })
        .detach();
    }

    fn delete_book(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.pending_delete != Some(id) {
            self.pending_delete = Some(id);
            cx.notify();
            return;
        }
        self.pending_delete = None;
        if let Err(error) = self.store.delete(id) {
            self.error = Some(format!("删除书籍失败：{error}"));
        }
        self.books = self.store.books().unwrap_or_default();
        cx.notify();
    }
}

impl Render for LibraryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.import_paths(paths.paths().to_vec(), cx);
            }))
            .child(
                h_flex()
                    .gap_3()
                    .child("Orange Reader · 书库")
                    .child(
                        Button::new("import")
                            .label("导入 EPUB")
                            .on_click(cx.listener(Self::choose_files)),
                    ),
            )
            .child(self.error.clone().unwrap_or_default())
            .child(if self.books.is_empty() {
                "书库为空。点击“导入 EPUB”添加书籍。".to_owned()
            } else {
                format!("{} 本书", self.books.len())
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .children(self.books.iter().cloned().map(|book| {
                        let open_book = book.clone();
                        let id = book.id;
                        div()
                            .id(("book", id as usize))
                            .w(px(190.))
                            .p_2()
                            .border_1()
                            .rounded_md()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(if let Some(path) = book.cover_path.clone() {
                                div()
                                    .h(px(240.))
                                    .overflow_hidden()
                                    .child(img(path).size_full())
                                    .into_any_element()
                            } else {
                                div()
                                    .h(px(240.))
                                    .bg(rgb(0xe9b48b))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(book.title.chars().next().unwrap_or(' ').to_string())
                                    .into_any_element()
                            })
                            .child(book.title)
                            .child(book.author)
                            .child(format!("已读 {}%", (book.fraction * 100.0).round() as u32))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new(("open", id as usize)).label("阅读").on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.open_book(open_book.clone(), window, cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        Button::new(("delete", id as usize))
                                            .label(if self.pending_delete == Some(id) {
                                                "确认删除"
                                            } else {
                                                "删除"
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.delete_book(id, cx)
                                            })),
                                    ),
                            )
                    })),
            )
    }
}

struct Spike {
    webview: Entity<WebView>,
    _server: ReaderServer,
    settings: ReaderSettings,
    settings_store: Option<Rc<Store>>,
}

impl Spike {
    fn new(
        window: &mut Window,
        cx: &mut App,
        server: ReaderServer,
        persistence: Option<(Rc<Store>, i64)>,
        settings: ReaderSettings,
    ) -> Entity<Self> {
        let url = server.url.clone();
        let settings_store = persistence.as_ref().map(|(store, _)| store.clone());
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
                    let Some((store, book_id)) = &persistence else {
                        return;
                    };
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
                    let result = match message.get("kind").and_then(Value::as_str) {
                        Some("relocate") => {
                            let fraction = message.get("fraction").and_then(Value::as_f64).filter(
                                |fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction),
                            );
                            match (cfi, fraction) {
                                (Some(cfi), Some(fraction)) => {
                                    store.save_web_progress(*book_id, cfi, fraction)
                                }
                                _ => return,
                            }
                        }
                        Some("highlight") => {
                            let text = message
                                .get("text")
                                .and_then(Value::as_str)
                                .filter(|text| !text.is_empty() && text.len() < 16384);
                            match (cfi, text) {
                                (Some(cfi), Some(text)) => store
                                    .add_web_annotation(*book_id, cfi, "#FFE066", text)
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
                                            "#FFE066" | "#FF9AA2" | "#A8E6CF" | "#A0C4FF"
                                        )
                                    });
                            match (cfi, color) {
                                (Some(cfi), Some(color)) => {
                                    store.recolor_web_annotation(*book_id, cfi, color)
                                }
                                _ => return,
                            }
                        }
                        Some("annotation-delete") => match cfi {
                            Some(cfi) => store.delete_web_annotation(*book_id, cfi),
                            None => return,
                        },
                        _ => return,
                    };
                    if let Err(error) = result {
                        eprintln!("保存阅读数据失败：{error}");
                    }
                });
            WebView::build(builder, window, cx).expect("无法创建 macOS WebView")
        });
        cx.new(|_| Self {
            webview,
            _server: server,
            settings,
            settings_store,
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
        if let Some(store) = &self.settings_store {
            let (name, value) = match key {
                "font-smaller" | "font-larger" => {
                    ("font_size", self.settings.font_size.to_string())
                }
                "line-smaller" | "line-larger" => {
                    ("line_height", self.settings.line_height.to_string())
                }
                "width-smaller" | "width-larger" => {
                    ("content_width", self.settings.content_width.to_string())
                }
                "letter-smaller" | "letter-larger" => (
                    "web_letter_spacing",
                    self.settings.letter_spacing.to_string(),
                ),
                "paragraph-smaller" | "paragraph-larger" => (
                    "paragraph_spacing",
                    self.settings.paragraph_spacing.to_string(),
                ),
                "font-family" => ("web_font_family", self.settings.font_family.clone()),
                "theme" => (
                    "theme",
                    if self.settings.dark { "dark" } else { "light" }.to_owned(),
                ),
                _ => unreachable!(),
            };
            if let Err(error) = store.save_setting(name, &value) {
                eprintln!("保存阅读设置失败：{error}");
            }
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

impl Render for Spike {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(
                h_flex()
                    .p_2()
                    .gap_2()
                    .child(
                        Button::new("previous")
                            .label("GPUI 上一页")
                            .on_click(cx.listener(|this, _, _, cx| this.send("previous", cx))),
                    )
                    .child(
                        Button::new("next")
                            .label("GPUI 下一页")
                            .on_click(cx.listener(|this, _, _, cx| this.send("next", cx))),
                    )
                    .child(
                        Button::new("highlight")
                            .label("GPUI 高亮选中")
                            .on_click(cx.listener(|this, _, _, cx| this.send("highlight", cx))),
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
                    .child(Button::new("font-smaller").label("A−").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("font-smaller", cx)),
                    ))
                    .child(format!("{}px", self.settings.font_size as u32))
                    .child(Button::new("font-larger").label("A+").on_click(
                        cx.listener(|this, _, _, cx| this.update_settings("font-larger", cx)),
                    ))
                    .child(
                        Button::new("theme")
                            .label(if self.settings.dark {
                                "浅色"
                            } else {
                                "深色"
                            })
                            .on_click(
                                cx.listener(|this, _, _, cx| this.update_settings("theme", cx)),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .p_2()
                    .gap_2()
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
                    )),
            )
            .child(
                h_flex()
                    .p_2()
                    .gap_2()
                    .child(
                        Button::new("font-family")
                            .label(format!("字体：{}", self.settings.font_family))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.update_settings("font-family", cx)
                            })),
                    )
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

fn serve(
    mut stream: TcpStream,
    foliate_dir: &Path,
    book: Option<&[u8]>,
    session: Option<&str>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
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
            include_bytes!("../reader.html").to_vec(),
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
            match fs::read(foliate_dir.join(&relative)) {
                Ok(bytes) => {
                    let mime = match relative.extension().and_then(|ext| ext.to_str()) {
                        Some("js") => "text/javascript; charset=utf-8",
                        Some("css") => "text/css; charset=utf-8",
                        Some("svg") => "image/svg+xml",
                        Some("html") => "text/html; charset=utf-8",
                        _ => "application/octet-stream",
                    };
                    ("200 OK", mime, bytes)
                }
                Err(_) => ("404 Not Found", "text/plain", Vec::new()),
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
    let foliate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../foliate-js");
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
                    if let Err(error) =
                        serve(stream, &foliate_dir, book.as_deref(), session.as_deref())
                    {
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

fn main() {
    let mut args = env::args_os().skip(1);
    let first_arg = args.next();
    let stored_id = if first_arg.as_deref() == Some(std::ffi::OsStr::new("--book-id")) {
        Some(
            args.next()
                .expect("--book-id 需要书籍 ID")
                .to_string_lossy()
                .parse::<i64>()
                .expect("书籍 ID 必须是整数"),
        )
    } else {
        None
    };
    let book_path = if stored_id.is_none() {
        first_arg.map(PathBuf::from)
    } else {
        None
    };
    let direct_server = book_path.map(|path| {
        let bytes =
            fs::read(&path).unwrap_or_else(|error| panic!("无法读取 {}: {error}", path.display()));
        start_server(Some(bytes), None)
    });
    let store = if direct_server.is_none() {
        Some(Rc::new(Store::open().expect("无法打开现有书库")))
    } else {
        None
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Some(server) = direct_server {
                gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
                    Spike::new(window, cx, server, None, ReaderSettings::load(None))
                })
                .expect("无法打开 GPUI 阅读窗口");
            } else if let (Some(store), Some(id)) = (&store, stored_id) {
                let book = store
                    .books()
                    .expect("无法读取书库")
                    .into_iter()
                    .find(|book| book.id == id)
                    .expect("书籍 ID 不存在");
                open_stored_book(store.clone(), book, cx).expect("无法打开书库中的书籍");
            } else if let Some(store) = &store {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    LibraryView::new(store.clone(), cx)
                })
                .expect("无法打开 GPUI 书库窗口");
            }
        });
}
