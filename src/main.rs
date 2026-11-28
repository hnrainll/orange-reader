mod book;
mod epub;
mod history_view;
mod reader_window;
mod server;
mod settings;
mod store;

use anyhow::Context as _;
use book::{BookFormat, is_supported_book_path, parse_book};
use gpui_kit::*;
use history_view::HistoryView;
use percent_encoding::percent_decode_str;
use reader_window::ReaderWindow;
use serde_json::{Value, json};
use server::start_server;
use settings::ReaderSettings;
use std::{
    cell::RefCell,
    collections::HashMap,
    env, fs,
    path::PathBuf,
    rc::Rc,
};
use store::{HistoryEntry, Store};

actions!(
    verso,
    [
        OpenFile,
        ShowHistory,
        CloseWindow,
        QuitApp,
        PrevPage,
        NextPage,
        ToggleToc,
        ToggleNotes,
        ToggleAppearance,
        ToggleFullscreen,
    ]
);

struct AppState {
    store: Rc<Store>,
    history_window: Option<(AnyWindowHandle, WeakEntity<HistoryView>)>,
    open_books: HashMap<i64, AnyWindowHandle>,
    last_error: Option<String>,
}

impl Global for AppState {}

fn file_url_to_path(raw: &str) -> Option<PathBuf> {
    let path = if let Some(rest) = raw.strip_prefix("file://") {
        let without_host = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode_str(without_host).decode_utf8_lossy();
        PathBuf::from(decoded.into_owned())
    } else {
        PathBuf::from(raw)
    };
    is_supported_book_path(&path).then_some(path)
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

fn schedule_refresh_history(cx: &mut App) {
    cx.defer(refresh_history);
}

fn open_or_focus_history_window(cx: &mut App) {
    if let Some((handle, weak)) = cx.global::<AppState>().history_window.clone() {
        let focused = cx.windows().contains(&handle)
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok();
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
        window_bounds: Some(WindowBounds::centered(size(px(760.), px(560.)), cx)),
        titlebar: Some(TitlebarOptions {
            title: Some("Verso".into()),
            appears_transparent: true,
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
        if cx.windows().contains(&existing_handle)
            && existing_handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            schedule_refresh_history(cx);
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
    let format = BookFormat::from_path(&entry.file_path).unwrap_or(BookFormat::Epub);
    let file_name = entry
        .file_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("book.epub");
    let has_cover = entry.cover_path.as_ref().is_some_and(|p| p.exists());
    let session = json!({
        "title": entry.title,
        "author": entry.author,
        "fileName": file_name,
        "mimeType": format.mime_type(),
        "hasCover": has_cover,
        "selfTest": self_test,
        "location": location,
        "annotations": annotations,
        "settings": settings.json(),
    })
    .to_string();
    let server = start_server(Some((bytes, format.mime_type())), Some(session));
    let title = entry.title.clone();
    let book_id = entry.id;
    let window_bounds = if self_test {
        WindowBounds::centered(size(px(1140.), px(780.)), cx)
    } else if let Some(display) = cx.primary_display() {
        WindowBounds::Windowed(display.visible_bounds())
    } else {
        WindowBounds::centered(size(px(1140.), px(780.)), cx)
    };
    let options = WindowOptions {
        window_bounds: Some(window_bounds),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let (handle, _) = gpui_kit::open_window(options, cx, move |window, cx| {
        ReaderWindow::new(window, cx, server, store, book_id, settings.theme)
    })?;
    cx.global_mut::<AppState>().open_books.insert(book_id, handle);
    schedule_refresh_history(cx);
    Ok(())
}

fn open_book_path(path: PathBuf, self_test: bool, cx: &mut App) -> anyhow::Result<()> {
    let bytes = fs::read(&path).with_context(|| format!("无法读取 {}", path.display()))?;
    let parsed = parse_book(&path, &bytes).with_context(|| format!("无法解析 {}", path.display()))?;
    let store = cx.global::<AppState>().store.clone();
    let entry = store.record_open(&path, &bytes, &parsed)?;
    open_reader_for_entry(entry, bytes, self_test, cx)
}

fn open_book_paths(paths: Vec<PathBuf>, self_test: bool, cx: &mut App) {
    let mut errors = Vec::new();
    for path in paths {
        if !is_supported_book_path(&path) {
            continue;
        }
        if let Err(error) = open_book_path(path, self_test, cx) {
            eprintln!("打开电子书失败：{error}");
            errors.push(error.to_string());
        }
    }
    if !errors.is_empty() {
        let message = errors.join("\n");
        cx.defer(move |cx| {
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
        });
    }
}

fn prompt_open_books(cx: &mut App) {
    let result = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("打开电子书".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = result.await {
            let _ = cx.update(|cx| open_book_paths(paths, false, cx));
        }
    })
    .detach();
}

fn configure_menus(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenFile, None),
        KeyBinding::new("cmd-shift-o", ShowHistory, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("left", PrevPage, Some("ReaderWindow")),
        KeyBinding::new("up", PrevPage, Some("ReaderWindow")),
        KeyBinding::new("pageup", PrevPage, Some("ReaderWindow")),
        KeyBinding::new("shift-space", PrevPage, Some("ReaderWindow")),
        KeyBinding::new("right", NextPage, Some("ReaderWindow")),
        KeyBinding::new("down", NextPage, Some("ReaderWindow")),
        KeyBinding::new("pagedown", NextPage, Some("ReaderWindow")),
        KeyBinding::new("space", NextPage, Some("ReaderWindow")),
        KeyBinding::new("cmd-shift-t", ToggleToc, Some("ReaderWindow")),
        KeyBinding::new("cmd-shift-n", ToggleNotes, Some("ReaderWindow")),
        KeyBinding::new("cmd-shift-a", ToggleAppearance, Some("ReaderWindow")),
        KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, Some("ReaderWindow")),
    ]);
    cx.on_action(|_: &OpenFile, cx| prompt_open_books(cx));
    cx.on_action(|_: &ShowHistory, cx| cx.defer(open_or_focus_history_window));
    cx.on_action(|_: &CloseWindow, cx| {
        cx.defer(|cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        });
    });
    cx.on_action(|_: &QuitApp, cx| cx.quit());
    cx.set_menus(vec![
        Menu::new("Verso").items([MenuItem::action("退出 Verso", QuitApp)]),
        Menu::new("文件").items([
            MenuItem::action("打开电子书…", OpenFile),
            MenuItem::action("最近阅读", ShowHistory),
            MenuItem::separator(),
            MenuItem::action("关闭窗口", CloseWindow),
        ]),
        Menu::new("阅读").items([
            MenuItem::action("上一页", PrevPage),
            MenuItem::action("下一页", NextPage),
            MenuItem::separator(),
            MenuItem::action("目录", ToggleToc),
            MenuItem::action("高亮列表", ToggleNotes),
            MenuItem::action("外观与排版", ToggleAppearance),
            MenuItem::separator(),
            MenuItem::action("切换全屏", ToggleFullscreen),
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
                let _ = cx.update(|cx| open_book_paths(paths, false, cx));
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
            open_book_paths(startup_paths, self_test, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::file_url_to_path;
    use std::path::PathBuf;

    #[test]
    fn decodes_macos_file_urls_for_supported_books() {
        assert_eq!(
            file_url_to_path("file:///Users/demo/Books/%E4%B8%89%E4%BD%93.epub"),
            Some(PathBuf::from("/Users/demo/Books/三体.epub"))
        );
        assert_eq!(
            file_url_to_path("file://localhost/tmp/Sample%20Book.AZW3"),
            Some(PathBuf::from("/tmp/Sample Book.AZW3"))
        );
        assert_eq!(
            file_url_to_path("file:///tmp/Kindle.mobi"),
            Some(PathBuf::from("/tmp/Kindle.mobi"))
        );
        assert_eq!(
            file_url_to_path("file:///tmp/Comic.cbz"),
            Some(PathBuf::from("/tmp/Comic.cbz"))
        );
        assert_eq!(
            file_url_to_path("file:///tmp/Novel.fb2.zip"),
            Some(PathBuf::from("/tmp/Novel.fb2.zip"))
        );
        assert_eq!(file_url_to_path("file:///tmp/notes.txt"), None);
        assert_eq!(file_url_to_path("file:///tmp/manual.pdf"), None);
    }
}
