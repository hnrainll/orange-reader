#[path = "../../../src-native/epub.rs"]
mod epub;
#[path = "../../../src-native/store.rs"]
mod store;

use gpui_kit::component::{button::Button, h_flex, v_flex};
use gpui_kit::*;
use gpui_webview::WebView;
use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    rc::Rc,
    sync::Arc,
    thread,
};
use store::{Book, Store};

struct LibraryView {
    store: Rc<Store>,
    books: Vec<Book>,
    error: Option<String>,
}

impl LibraryView {
    fn new(store: Rc<Store>, cx: &mut App) -> Entity<Self> {
        let books = store.books().unwrap_or_default();
        cx.new(|_| Self {
            store,
            books,
            error: None,
        })
    }

    fn open_book(&mut self, book: Book, _: &mut Window, cx: &mut Context<Self>) {
        let bytes = match fs::read(&book.file_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.error = Some(format!("无法读取《{}》：{error}", book.title));
                cx.notify();
                return;
            }
        };
        let url = start_server(Some(bytes));
        if let Err(error) = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            Spike::new(window, cx, &url)
        }) {
            self.error = Some(format!("无法打开阅读窗口：{error}"));
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
}

impl Render for LibraryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child("Orange Reader · GPUI Kit 书库验证")
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
            .children(self.books.iter().cloned().map(|book| {
                let title = book.title.clone();
                let author = book.author.clone();
                div()
                    .id(("book", book.id as usize))
                    .cursor_pointer()
                    .p_2()
                    .child(format!("{title}  ·  {author}"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_book(book.clone(), window, cx)
                    }))
            }))
    }
}

struct Spike {
    webview: Entity<WebView>,
}

impl Spike {
    fn new(window: &mut Window, cx: &mut App, url: &str) -> Entity<Self> {
        let url = url.to_owned();
        let webview = cx.new(|cx| {
            let builder = wry::WebViewBuilder::new()
                .with_url(&url)
                .with_ipc_handler(|request| eprintln!("[foliate-js] {}", request.body()));
            WebView::build(builder, window, cx).expect("无法创建 macOS WebView")
        });
        cx.new(|_| Self { webview })
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
                    .child("在下方选择 EPUB；页面事件会输出到终端"),
            )
            .child(div().flex_1().child(self.webview.clone()))
    }
}

fn serve(mut stream: TcpStream, foliate_dir: &Path, book: Option<&[u8]>) -> std::io::Result<()> {
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

fn start_server(book: Option<Vec<u8>>) -> String {
    let foliate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../foliate-js");
    let listener = TcpListener::bind("127.0.0.1:0").expect("无法启动本地阅读页服务器");
    let address = listener.local_addr().expect("无法获取本地端口");
    let book = Arc::new(book);
    thread::spawn(move || {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    if let Err(error) = serve(stream, &foliate_dir, book.as_deref()) {
                        eprintln!("资源请求失败：{error}");
                    }
                }
                Err(error) => eprintln!("本地连接失败：{error}"),
            }
        }
    });

    let url = format!("http://{address}/");
    eprintln!("阅读页：{url}");
    url
}

fn main() {
    let book_path = env::args_os().nth(1).map(PathBuf::from);
    let direct_url = book_path.map(|path| {
        let bytes =
            fs::read(&path).unwrap_or_else(|error| panic!("无法读取 {}: {error}", path.display()));
        start_server(Some(bytes))
    });
    let store = if direct_url.is_none() {
        Some(Rc::new(Store::open().expect("无法打开现有书库")))
    } else {
        None
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Some(url) = &direct_url {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    Spike::new(window, cx, url)
                })
                .expect("无法打开 GPUI 阅读窗口");
            } else if let Some(store) = &store {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    LibraryView::new(store.clone(), cx)
                })
                .expect("无法打开 GPUI 书库窗口");
            }
        });
}
