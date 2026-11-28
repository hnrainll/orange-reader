use rust_embed::RustEmbed;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Component, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(RustEmbed)]
#[folder = "foliate-js/"]
struct FoliateAssets;

pub struct ReaderServer {
    pub url: String,
    running: Arc<AtomicBool>,
}

impl Drop for ReaderServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}

fn serve(
    mut stream: TcpStream,
    book: Option<(&[u8], &str)>,
    session: Option<&str>,
) -> std::io::Result<()> {
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
    } else if path == "/book" || path == "/book.epub" {
        match book {
            Some((bytes, mime)) => ("200 OK", mime, bytes.to_vec()),
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

pub fn start_server(
    book: Option<(Vec<u8>, &'static str)>,
    session: Option<String>,
) -> ReaderServer {
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
                    let book_ref = (*book)
                        .as_ref()
                        .map(|(bytes, mime)| (bytes.as_slice(), *mime));
                    if let Err(error) = serve(stream, book_ref, session.as_deref()) {
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

#[cfg(test)]
mod tests {
    use super::FoliateAssets;

    #[test]
    fn embeds_book_renderers() {
        assert!(FoliateAssets::get("view.js").is_some());
        assert!(FoliateAssets::get("epub.js").is_some());
        assert!(FoliateAssets::get("mobi.js").is_some());
        assert!(FoliateAssets::get("comic-book.js").is_some());
        assert!(FoliateAssets::get("fb2.js").is_some());
        assert!(FoliateAssets::get("vendor/zip.js").is_some());
        assert!(FoliateAssets::get("vendor/fflate.js").is_some());
    }
}
