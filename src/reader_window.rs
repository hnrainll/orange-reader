use crate::{
    NextPage, PrevPage, ToggleAppearance, ToggleFullscreen, ToggleNotes, ToggleToc,
    book::decode_base64,
    open_book_paths, open_or_focus_history_window, prompt_open_books, refresh_history,
    server::ReaderServer,
    settings::{ReaderSettings, is_valid_theme, theme_bg},
    store::Store,
};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use gpui_webview::WebView;
use serde_json::Value;
use std::rc::Rc;

fn is_valid_highlight_color(color: &str) -> bool {
    matches!(
        color,
        "#FFE066" | "#86EFAC" | "#93C5FD" | "#F9A8D4" | "#FCA5A5"
    )
}

pub struct ReaderWindow {
    webview: Entity<WebView>,
    focus_handle: FocusHandle,
    theme: String,
    _server: ReaderServer,
}

impl ReaderWindow {
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        server: ReaderServer,
        store: Rc<Store>,
        book_id: i64,
        initial_theme: String,
    ) -> Entity<Self> {
        let url = server.url.clone();
        let ipc_store = store.clone();
        let async_app = cx.to_async();
        let window_handle = window.window_handle();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        cx.new(|cx| {
            let weak_self: WeakEntity<Self> = cx.entity().downgrade();
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
                        if body.len() <= 2048 {
                            eprintln!("[foliate-js] {body}");
                        } else {
                            eprintln!("[foliate-js] ({} bytes)", body.len());
                        }
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
                            Some("open-file") => {
                                let app = async_app.clone();
                                app.spawn(async move |cx| {
                                    let _ = cx.update(prompt_open_books);
                                })
                                .detach();
                                return;
                            }
                            Some("show-history") => {
                                let app = async_app.clone();
                                app.spawn(async move |cx| {
                                    let _ = cx.update(open_or_focus_history_window);
                                })
                                .detach();
                                return;
                            }
                            Some("window-double-click") => {
                                let app = async_app.clone();
                                app.spawn(async move |cx| {
                                    let _ = cx.update(|cx| {
                                        let _ = window_handle.update(cx, |_, window, _| {
                                            window.titlebar_double_click()
                                        });
                                    });
                                })
                                .detach();
                                return;
                            }
                            Some("settings") => match message.get("settings") {
                                Some(settings) => {
                                    if let Some(next_theme) = settings
                                        .get("theme")
                                        .and_then(Value::as_str)
                                        .filter(|t| is_valid_theme(t))
                                        .map(str::to_owned)
                                    {
                                        let app = async_app.clone();
                                        let weak = weak_self.clone();
                                        app.spawn(async move |cx| {
                                            let _ = cx.update(|cx| {
                                                if let Some(entity) = weak.upgrade() {
                                                    entity.update(cx, |this, cx| {
                                                        if this.theme != next_theme {
                                                            this.theme = next_theme;
                                                            cx.notify();
                                                        }
                                                    });
                                                }
                                            });
                                        })
                                        .detach();
                                    }
                                    ReaderSettings::save_from_json(&ipc_store, settings)
                                }
                                None => return,
                            },
                            Some("relocate") => {
                                let fraction =
                                    message.get("fraction").and_then(Value::as_f64).filter(
                                        |fraction| {
                                            fraction.is_finite() && (0.0..=1.0).contains(fraction)
                                        },
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
                                let color = message
                                    .get("color")
                                    .and_then(Value::as_str)
                                    .filter(|color| is_valid_highlight_color(color));
                                match (cfi, text, color) {
                                    (Some(cfi), Some(text), Some(color)) => ipc_store
                                        .add_annotation(book_id, cfi, color, text)
                                        .map(|_| ()),
                                    _ => return,
                                }
                            }
                            Some("annotation-color") => {
                                let color = message
                                    .get("color")
                                    .and_then(Value::as_str)
                                    .filter(|color| is_valid_highlight_color(color));
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
                            Some("metadata") => {
                                let title = message
                                    .get("title")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|text| !text.is_empty() && text.len() < 512);
                                let author = message
                                    .get("author")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|text| !text.is_empty() && text.len() < 512);
                                let cover = message
                                    .get("coverBase64")
                                    .and_then(Value::as_str)
                                    .filter(|raw| !raw.is_empty() && raw.len() < 8 * 1024 * 1024)
                                    .and_then(decode_base64);
                                match ipc_store.update_metadata(
                                    book_id,
                                    title,
                                    author,
                                    cover.as_deref(),
                                ) {
                                    Ok(changed) => {
                                        should_refresh_history = changed;
                                        Ok(())
                                    }
                                    Err(error) => Err(error),
                                }
                            }
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
            Self {
                webview,
                focus_handle,
                theme: initial_theme,
                _server: server,
            }
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
}

impl Render for ReaderWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = rgb(theme_bg(&self.theme));
        v_flex()
            .size_full()
            .bg(bg)
            .key_context("ReaderWindow")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &PrevPage, _, cx| this.send("previous", cx)))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| this.send("next", cx)))
            .on_action(cx.listener(|this, _: &ToggleToc, _, cx| this.send("toc", cx)))
            .on_action(cx.listener(|this, _: &ToggleNotes, _, cx| this.send("notes", cx)))
            .on_action(cx.listener(|this, _: &ToggleAppearance, _, cx| this.send("appearance", cx)))
            .on_action(|_: &ToggleFullscreen, window, _| window.toggle_fullscreen())
            .on_drop(|paths: &ExternalPaths, _, cx| {
                let paths = paths.paths().to_vec();
                cx.defer(move |cx| open_book_paths(paths, false, cx));
            })
            .when(!window.is_fullscreen(), |this| {
                this.child(
                    div()
                        .h(px(30.))
                        .w_full()
                        .flex_shrink_0()
                        .bg(bg)
                        .window_control_area(WindowControlArea::Drag)
                        .on_mouse_down(MouseButton::Left, |event, window, _| {
                            if event.click_count == 2 {
                                window.titlebar_double_click();
                            } else {
                                window.start_window_move();
                            }
                        }),
                )
            })
            .child(div().flex_1().w_full().child(self.webview.clone()))
    }
}
