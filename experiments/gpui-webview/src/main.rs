use gpui_kit::component::{button::Button, h_flex, v_flex};
use gpui_kit::*;
use gpui_webview::WebView;
use std::{
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    sync::Arc,
    thread,
};

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

fn main() {
    let book = env::args_os().nth(1).map(|path| {
        let path = PathBuf::from(path);
        fs::read(&path).unwrap_or_else(|error| panic!("无法读取 {}: {error}", path.display()))
    });
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
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                Spike::new(window, cx, &url)
            })
            .expect("无法打开 GPUI 窗口");
        });
}
