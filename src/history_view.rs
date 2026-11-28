use crate::{
    AppState, epub, is_epub_path, open_epub_paths, open_reader_for_entry, prompt_open_epubs,
    store::{HistoryEntry, Store},
};
use anyhow::Context as _;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::{fs, path::Path, rc::Rc};

fn display_path(path: &Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(relative) = path.strip_prefix(&home) {
            return format!("~/{}", relative.display());
        }
    }
    path.display().to_string()
}

fn short_time(ts: &str) -> String {
    if ts.len() >= 16 {
        ts[5..16].to_owned()
    } else {
        ts.to_owned()
    }
}

pub struct HistoryView {
    store: Rc<Store>,
    entries: Vec<HistoryEntry>,
    error: Option<String>,
}

impl HistoryView {
    pub fn new(store: Rc<Store>, initial_error: Option<String>, cx: &mut App) -> Entity<Self> {
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

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        match self.store.history() {
            Ok(entries) => {
                self.entries = entries;
            }
            Err(error) => self.error = Some(format!("读取历史记录失败：{error}")),
        }
        cx.notify();
    }

    pub fn set_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    fn open_entry(&mut self, entry: HistoryEntry, cx: &mut Context<Self>) {
        self.error = None;
        cx.notify();
        let path = entry.file_path;
        let app: &mut App = cx;
        app.defer(move |cx| {
            open_epub_paths(vec![path], false, cx);
        });
    }

    fn relocate_entry(&mut self, id: i64, cx: &mut Context<Self>) {
        let store = self.store.clone();
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
            let prepared = fs::read(&path)
                .with_context(|| format!("无法读取 {}", path.display()))
                .and_then(|bytes| {
                    let parsed = epub::parse(&path)
                        .with_context(|| format!("无法解析 {}", path.display()))?;
                    let entry = store.relocate_entry(id, &path, &bytes, &parsed)?;
                    Ok((entry, bytes))
                });
            let _ = cx.update(|cx| {
                let outcome = prepared.and_then(|(entry, bytes)| {
                    open_reader_for_entry(entry, bytes, false, cx)
                });
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                        this.error = outcome.err().map(|e| e.to_string());
                        this.reload(cx);
                    });
                }
            });
        })
        .detach();
    }

    fn remove_entry(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Err(error) = self.store.remove_history(id) {
            self.error = Some(format!("移出历史失败：{error}"));
        } else {
            self.error = None;
            cx.global_mut::<AppState>().open_books.remove(&id);
        }
        self.reload(cx);
    }
}

impl Render for HistoryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.entries.len();
        v_flex()
            .size_full()
            .bg(rgb(0xf7f6f3))
            .text_color(rgb(0x1d1d1f))
            .on_drop(|paths: &ExternalPaths, _, cx| {
                let paths = paths.paths().to_vec();
                cx.defer(move |cx| open_epub_paths(paths, false, cx));
            })
            .when(!window.is_fullscreen(), |this| {
                this.child(
                    div()
                        .h(px(34.))
                        .w_full()
                        .flex_shrink_0()
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
            .child(
                v_flex()
                    .flex_1()
                    .w_full()
                    .max_w(px(740.))
                    .mx_auto()
                    .px_6()
                    .pb_5()
                    .gap_4()
                    .overflow_hidden()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .gap_3()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("最近阅读"),
                                    )
                                    .when(count > 0, |row| {
                                        row.child(
                                            div()
                                                .px_2()
                                                .py(px(2.))
                                                .rounded_full()
                                                .bg(rgb(0xeae7e1))
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(0x6e6e73))
                                                .child(count.to_string()),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .id("open-epub")
                                    .px_3()
                                    .py(px(6.))
                                    .rounded_lg()
                                    .bg(rgb(0x1d1d1f))
                                    .hover(|s| s.bg(rgb(0x3a3a3c)))
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xffffff))
                                    .cursor_pointer()
                                    .on_click(|_, _, cx| prompt_open_epubs(cx))
                                    .child("打开 EPUB"),
                            ),
                    )
                    .when_some(self.error.clone(), |this, error| {
                        this.child(
                            h_flex()
                                .px_3()
                                .py_2()
                                .gap_3()
                                .rounded_lg()
                                .bg(rgb(0xfef2f2))
                                .border_1()
                                .border_color(rgb(0xfecaca))
                                .text_xs()
                                .text_color(rgb(0x991b1b))
                                .justify_between()
                                .items_center()
                                .child(div().flex_1().min_w_0().child(error))
                                .child(
                                    div()
                                        .id("dismiss-error")
                                        .px_2()
                                        .py(px(2.))
                                        .rounded_md()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0xfee2e2)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.set_error(None, cx)
                                        }))
                                        .child("×"),
                                ),
                        )
                    })
                    .child(if self.entries.is_empty() {
                        v_flex()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .rounded_xl()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_dashed()
                            .border_color(rgb(0xd8d3c9))
                            .p_8()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(44.))
                                    .h(px(44.))
                                    .mb_1()
                                    .rounded_xl()
                                    .bg(rgb(0xfef3eb))
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xd96b27))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child("书"),
                            )
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("还没有阅读记录"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x86868b))
                                    .child("将 .epub 文件拖到这里，或点击右上角“打开 EPUB”（⌘O）"),
                            )
                            .into_any_element()
                    } else {
                        v_flex()
                            .flex_1()
                            .gap_2()
                            .overflow_y_scrollbar()
                            .children(self.entries.iter().cloned().map(|entry| {
                                let id = entry.id;
                                let exists = entry.file_path.exists();
                                let open_target = entry.clone();
                                let reveal_path = entry.file_path.clone();
                                let frac = entry.fraction.clamp(0.0, 1.0);
                                let percent = (frac * 100.0).round() as u32;
                                let fill_px = (frac * 56.0)
                                    .round()
                                    .max(if frac > 0.0 { 3.0 } else { 0.0 })
                                    as f32;
                                let chapter_label = entry
                                    .chapter_title
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_owned);
                                let location_text = display_path(&entry.file_path);
                                let time_text = short_time(&entry.last_opened_at);

                                h_flex()
                                    .id(("history", id as usize))
                                    .px_3()
                                    .py(px(10.))
                                    .gap_3()
                                    .rounded_xl()
                                    .bg(rgb(0xffffff))
                                    .border_1()
                                    .border_color(rgb(0xebe8e2))
                                    .hover(|s| {
                                        s.bg(rgb(0xfdfcfa)).border_color(rgb(0xd6d1c7))
                                    })
                                    .cursor_pointer()
                                    .items_center()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if exists {
                                            this.open_entry(open_target.clone(), cx);
                                        } else {
                                            this.relocate_entry(id, cx);
                                        }
                                    }))
                                    .child(
                                        if let Some(path) =
                                            entry.cover_path.clone().filter(|p| p.exists())
                                        {
                                            div()
                                                .w(px(46.))
                                                .h(px(64.))
                                                .flex_shrink_0()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(rgb(0xe8e4dc))
                                                .overflow_hidden()
                                                .child(
                                                    img(path)
                                                        .size_full()
                                                        .object_fit(ObjectFit::Cover),
                                                )
                                                .into_any_element()
                                        } else {
                                            div()
                                                .w(px(46.))
                                                .h(px(64.))
                                                .flex_shrink_0()
                                                .rounded_md()
                                                .bg(rgb(0xf2e6d8))
                                                .text_base()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(0x9a6b3f))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    entry
                                                        .title
                                                        .chars()
                                                        .next()
                                                        .unwrap_or('书')
                                                        .to_string(),
                                                )
                                                .into_any_element()
                                        },
                                    )
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap(px(4.))
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_baseline()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .truncate()
                                                            .text_sm()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(rgb(0x1d1d1f))
                                                            .child(entry.title.clone()),
                                                    )
                                                    .when(!entry.author.is_empty(), |row| {
                                                        row.child(
                                                            div()
                                                                .flex_shrink_0()
                                                                .max_w(px(140.))
                                                                .truncate()
                                                                .text_xs()
                                                                .text_color(rgb(0x86868b))
                                                                .child(entry.author.clone()),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .min_w_0()
                                                    .child(
                                                        div()
                                                            .w(px(56.))
                                                            .h(px(4.))
                                                            .flex_shrink_0()
                                                            .rounded_full()
                                                            .bg(rgb(0xebe8e2))
                                                            .overflow_hidden()
                                                            .child(
                                                                div()
                                                                    .h_full()
                                                                    .w(px(fill_px))
                                                                    .rounded_full()
                                                                    .bg(rgb(0xd96b27)),
                                                            ),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_shrink_0()
                                                            .text_xs()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(0x6e6e73))
                                                            .child(format!("{percent}%")),
                                                    )
                                                    .when_some(chapter_label, |row, chapter| {
                                                        row.child(
                                                            div()
                                                                .flex_1()
                                                                .min_w_0()
                                                                .truncate()
                                                                .text_xs()
                                                                .text_color(rgb(0x86868b))
                                                                .child(format!("· {chapter}")),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .min_w_0()
                                                    .child(if exists {
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .text_ellipsis_middle()
                                                            .text_xs()
                                                            .text_color(rgb(0x98989d))
                                                            .child(location_text)
                                                            .into_any_element()
                                                    } else {
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .truncate()
                                                            .text_xs()
                                                            .text_color(rgb(0xb45309))
                                                            .child(format!(
                                                                "文件已移动 · {location_text}"
                                                            ))
                                                            .into_any_element()
                                                    })
                                                    .child(
                                                        div()
                                                            .flex_shrink_0()
                                                            .text_xs()
                                                            .text_color(rgb(0xa1a1a6))
                                                            .child(time_text),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .flex_shrink_0()
                                            .items_center()
                                            .when(exists, |actions| {
                                                actions.child(
                                                    div()
                                                        .id(("reveal", id as usize))
                                                        .px_2()
                                                        .py(px(4.))
                                                        .rounded_md()
                                                        .text_xs()
                                                        .text_color(rgb(0x86868b))
                                                        .hover(|s| {
                                                            s.bg(rgb(0xf0eee9))
                                                                .text_color(rgb(0x1d1d1f))
                                                        })
                                                        .cursor_pointer()
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            |_, _, cx| cx.stop_propagation(),
                                                        )
                                                        .on_click(move |_, _, cx| {
                                                            cx.stop_propagation();
                                                            cx.reveal_path(&reveal_path);
                                                        })
                                                        .child("Finder"),
                                                )
                                            })
                                            .when(!exists, |actions| {
                                                actions.child(
                                                    div()
                                                        .id(("relocate", id as usize))
                                                        .px_2()
                                                        .py(px(4.))
                                                        .rounded_md()
                                                        .bg(rgb(0xfef3c7))
                                                        .hover(|s| s.bg(rgb(0xfde68a)))
                                                        .text_xs()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(0xb45309))
                                                        .cursor_pointer()
                                                        .on_mouse_down(
                                                            MouseButton::Left,
                                                            |_, _, cx| cx.stop_propagation(),
                                                        )
                                                        .on_click(cx.listener(
                                                            move |this, _, _, cx| {
                                                                cx.stop_propagation();
                                                                this.relocate_entry(id, cx);
                                                            },
                                                        ))
                                                        .child("重新定位"),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .id(("remove", id as usize))
                                                    .w(px(26.))
                                                    .h(px(26.))
                                                    .rounded_md()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .text_sm()
                                                    .text_color(rgb(0xa1a1a6))
                                                    .hover(|s| {
                                                        s.bg(rgb(0xfee2e2))
                                                            .text_color(rgb(0xdc2626))
                                                    })
                                                    .cursor_pointer()
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        |_, _, cx| cx.stop_propagation(),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            cx.stop_propagation();
                                                            this.remove_entry(id, cx);
                                                        },
                                                    ))
                                                    .child("×"),
                                            ),
                                    )
                            }))
                            .into_any_element()
                    })
                    .when(count > 0, |this| {
                        this.child(
                            div()
                                .w_full()
                                .text_center()
                                .text_xs()
                                .text_color(rgb(0xa1a1a6))
                                .child("点击书目继续阅读 · 拖入 .epub 文件或按 ⌘O 打开新书"),
                        )
                    }),
            )
    }
}
