use crate::epub;
use anyhow::{Context, Result, bail};
use roxmltree::Document;
use std::{
    cmp::Ordering,
    io::{Cursor, Read},
    path::Path,
};
use zip::ZipArchive;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookFormat {
    Epub,
    Mobi,
    Cbz,
    Fb2,
    Fbz,
}

impl BookFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        if name.ends_with(".epub") {
            Some(Self::Epub)
        } else if name.ends_with(".azw3")
            || name.ends_with(".mobi")
            || name.ends_with(".azw")
            || name.ends_with(".prc")
        {
            Some(Self::Mobi)
        } else if name.ends_with(".cbz") {
            Some(Self::Cbz)
        } else if name.ends_with(".fb2.zip") || name.ends_with(".fbz") {
            Some(Self::Fbz)
        } else if name.ends_with(".fb2") {
            Some(Self::Fb2)
        } else {
            None
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Epub => "application/epub+zip",
            Self::Mobi => "application/x-mobipocket-ebook",
            Self::Cbz => "application/vnd.comicbook+zip",
            Self::Fb2 => "application/x-fictionbook+xml",
            Self::Fbz => "application/x-zip-compressed-fb2",
        }
    }
}

pub fn is_supported_book_path(path: &Path) -> bool {
    BookFormat::from_path(path).is_some()
}

pub fn fallback_title(path: &Path) -> String {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".fb2.zip") && name.len() > ".fb2.zip".len() {
            let stem = name[..name.len() - ".fb2.zip".len()].trim();
            if !stem.is_empty() {
                return stem.to_owned();
            }
        }
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Untitled")
        .to_owned()
}

#[derive(Clone, Debug)]
pub struct BookMeta {
    pub title: String,
    pub author: String,
    pub cover: Option<Vec<u8>>,
}

pub fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn val(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut clean = Vec::with_capacity(input.len());
    for byte in input.bytes() {
        if !byte.is_ascii_whitespace() {
            clean.push(byte);
        }
    }
    if clean.is_empty() || clean.len() % 4 != 0 {
        return None;
    }

    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks_exact(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2])?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = val(chunk[3])?;
                out.push((c << 6) | d);
            }
        } else if chunk[3] != b'=' {
            return None;
        }
    }
    Some(out)
}

fn natural_cmp(a: &str, b: &str) -> Ordering {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let (mut i, mut j) = (0, 0);

    while i < ab.len() && j < bb.len() {
        if ab[i].is_ascii_digit() && bb[j].is_ascii_digit() {
            let i_start = i;
            while i < ab.len() && ab[i].is_ascii_digit() {
                i += 1;
            }
            let j_start = j;
            while j < bb.len() && bb[j].is_ascii_digit() {
                j += 1;
            }
            let a_num = &ab[i_start..i];
            let b_num = &bb[j_start..j];
            let a_trim = a_num
                .iter()
                .position(|&c| c != b'0')
                .map_or(&b""[..], |pos| &a_num[pos..]);
            let b_trim = b_num
                .iter()
                .position(|&c| c != b'0')
                .map_or(&b""[..], |pos| &b_num[pos..]);
            let ord = a_trim
                .len()
                .cmp(&b_trim.len())
                .then_with(|| a_trim.cmp(b_trim))
                .then_with(|| a_num.len().cmp(&b_num.len()));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            let ord = ab[i]
                .to_ascii_lowercase()
                .cmp(&bb[j].to_ascii_lowercase());
            if ord != Ordering::Equal {
                return ord;
            }
            i += 1;
            j += 1;
        }
    }
    ab.len().cmp(&bb.len())
}

fn is_comic_image(name: &str) -> bool {
    if name.starts_with("__MACOSX/") || name.contains("/__MACOSX/") {
        return false;
    }
    let base = name.rsplit('/').next().unwrap_or(name);
    if base.starts_with("._") {
        return false;
    }
    let lower = base.to_ascii_lowercase();
    [
        ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".svg", ".jxl", ".avif",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

fn parse_cbz(path: &Path, bytes: &[u8]) -> Result<BookMeta> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut images = Vec::new();
    let mut comic_info_name = None;

    for idx in 0..zip.len() {
        let entry = zip.by_index(idx)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        if comic_info_name.is_none()
            && name
                .rsplit('/')
                .next()
                .is_some_and(|base| base.eq_ignore_ascii_case("ComicInfo.xml"))
        {
            comic_info_name = Some(name.clone());
        }
        if is_comic_image(&name) {
            images.push(name);
        }
    }

    if images.is_empty() {
        bail!("CBZ 压缩包中未找到支持的图片页面");
    }
    images.sort_by(|a, b| natural_cmp(a, b));

    let mut title = fallback_title(path);
    let mut author = String::new();
    if let Some(info_name) = comic_info_name {
        if let Ok(mut entry) = zip.by_name(&info_name) {
            let mut xml = String::new();
            if entry.read_to_string(&mut xml).is_ok() {
                if let Ok(doc) = Document::parse(&xml) {
                    if let Some(t) = doc
                        .descendants()
                        .find(|n| n.tag_name().name() == "Title")
                        .and_then(|n| n.text())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        title = t.to_owned();
                    }
                    if let Some(w) = doc
                        .descendants()
                        .find(|n| n.tag_name().name() == "Writer")
                        .and_then(|n| n.text())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        author = w.to_owned();
                    }
                }
            }
        }
    }

    let mut cover = Vec::new();
    let cover = if zip
        .by_name(&images[0])
        .and_then(|mut entry| {
            entry.read_to_end(&mut cover)?;
            Ok(())
        })
        .is_ok()
        && !cover.is_empty()
    {
        Some(cover)
    } else {
        None
    };

    Ok(BookMeta {
        title,
        author,
        cover,
    })
}

fn parse_fb2_xml(path: &Path, xml: &str) -> BookMeta {
    let fallback = fallback_title(path);
    let Ok(doc) = Document::parse(xml) else {
        return BookMeta {
            title: fallback,
            author: String::new(),
            cover: None,
        };
    };

    let title_info = doc
        .descendants()
        .find(|n| n.tag_name().name() == "title-info");

    let title = title_info
        .and_then(|ti| {
            ti.descendants()
                .find(|n| n.tag_name().name() == "book-title")
                .and_then(|n| n.text())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or(&fallback)
        .to_owned();

    let authors: Vec<String> = title_info
        .into_iter()
        .flat_map(|ti| ti.children().filter(|n| n.tag_name().name() == "author"))
        .filter_map(|author_node| {
            let child_text = |tag: &str| {
                author_node
                    .children()
                    .find(|n| n.tag_name().name() == tag)
                    .and_then(|n| n.text())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            };
            let parts: Vec<&str> = ["first-name", "middle-name", "last-name"]
                .into_iter()
                .filter_map(child_text)
                .collect();
            if !parts.is_empty() {
                Some(parts.join(" "))
            } else {
                child_text("nickname").map(str::to_owned)
            }
        })
        .collect();
    let author = authors.join(", ");

    let cover_id = title_info
        .and_then(|ti| {
            ti.descendants()
                .find(|n| n.tag_name().name() == "coverpage")
        })
        .and_then(|cp| cp.descendants().find(|n| n.tag_name().name() == "image"))
        .and_then(|img| {
            img.attributes()
                .find(|attr| attr.name() == "href")
                .map(|attr| attr.value().trim().trim_start_matches('#'))
        })
        .filter(|id| !id.is_empty());

    let cover = cover_id.and_then(|id| {
        doc.descendants()
            .find(|n| n.tag_name().name() == "binary" && n.attribute("id") == Some(id))
            .and_then(|n| n.text())
            .and_then(decode_base64)
    });

    BookMeta {
        title,
        author,
        cover,
    }
}

fn parse_fbz(path: &Path, bytes: &[u8]) -> Result<BookMeta> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut target = None;
    let mut first_file = None;

    for idx in 0..zip.len() {
        let entry = zip.by_index(idx)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        if first_file.is_none() {
            first_file = Some(name.clone());
        }
        if name.to_ascii_lowercase().ends_with(".fb2") {
            target = Some(name);
            break;
        }
    }

    let entry_name = target
        .or(first_file)
        .context("FBZ 压缩包中未找到 FB2 文件")?;
    let mut raw = Vec::new();
    zip.by_name(&entry_name)?.read_to_end(&mut raw)?;

    Ok(std::str::from_utf8(&raw)
        .map(|xml| parse_fb2_xml(path, xml))
        .unwrap_or_else(|_| BookMeta {
            title: fallback_title(path),
            author: String::new(),
            cover: None,
        }))
}

fn parse_mobi(path: &Path, bytes: &[u8]) -> Result<BookMeta> {
    let magic = bytes.get(60..68);
    if !matches!(magic, Some(b"BOOKMOBI" | b"TEXtREAd")) {
        bail!("不是有效的 MOBI / AZW3 电子书文件");
    }
    Ok(BookMeta {
        title: fallback_title(path),
        author: String::new(),
        cover: None,
    })
}

pub fn parse_book(path: &Path, bytes: &[u8]) -> Result<BookMeta> {
    let format = BookFormat::from_path(path).context("不支持的电子书格式")?;
    match format {
        BookFormat::Epub => {
            let parsed = epub::parse(path)?;
            let title = if parsed.title.is_empty() || parsed.title == "Untitled" {
                fallback_title(path)
            } else {
                parsed.title
            };
            Ok(BookMeta {
                title,
                author: parsed.author,
                cover: parsed.cover,
            })
        }
        BookFormat::Cbz => parse_cbz(path, bytes),
        BookFormat::Fb2 => Ok(std::str::from_utf8(bytes)
            .map(|xml| parse_fb2_xml(path, xml))
            .unwrap_or_else(|_| BookMeta {
                title: fallback_title(path),
                author: String::new(),
                cover: None,
            })),
        BookFormat::Fbz => parse_fbz(path, bytes),
        BookFormat::Mobi => parse_mobi(path, bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::{BookFormat, decode_base64, fallback_title, parse_book};
    use std::{
        io::{Cursor, Write},
        path::Path,
    };
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn detects_supported_formats_case_insensitively() {
        assert_eq!(BookFormat::from_path(Path::new("a.epub")), Some(BookFormat::Epub));
        assert_eq!(BookFormat::from_path(Path::new("a.AZW3")), Some(BookFormat::Mobi));
        assert_eq!(BookFormat::from_path(Path::new("a.mobi")), Some(BookFormat::Mobi));
        assert_eq!(BookFormat::from_path(Path::new("a.azw")), Some(BookFormat::Mobi));
        assert_eq!(BookFormat::from_path(Path::new("a.prc")), Some(BookFormat::Mobi));
        assert_eq!(BookFormat::from_path(Path::new("a.CBZ")), Some(BookFormat::Cbz));
        assert_eq!(BookFormat::from_path(Path::new("a.fb2")), Some(BookFormat::Fb2));
        assert_eq!(BookFormat::from_path(Path::new("a.fbz")), Some(BookFormat::Fbz));
        assert_eq!(BookFormat::from_path(Path::new("a.FB2.ZIP")), Some(BookFormat::Fbz));
        assert_eq!(BookFormat::from_path(Path::new("a.pdf")), None);
        assert_eq!(BookFormat::from_path(Path::new("a.txt")), None);
        assert_eq!(BookFormat::from_path(Path::new("a.zip")), None);
        assert_eq!(fallback_title(Path::new("/tmp/War.fb2.zip")), "War");
    }

    #[test]
    fn decodes_base64_with_whitespace() {
        assert_eq!(decode_base64("SGVs\nbG8h\n"), Some(b"Hello!".to_vec()));
        assert_eq!(decode_base64("YQ=="), Some(b"a".to_vec()));
        assert_eq!(decode_base64("YWI="), Some(b"ab".to_vec()));
        assert_eq!(decode_base64("not!valid"), None);
    }

    #[test]
    fn parses_cbz_with_natural_cover_order_and_comic_info() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            let opts = SimpleFileOptions::default();
            for (name, data) in [
                ("__MACOSX/._page1.png", b"junk".as_slice()),
                ("page10.png", b"page-ten".as_slice()),
                ("page2.png", b"\x89PNG\r\n\x1a\npage-two".as_slice()),
                (
                    "ComicInfo.xml",
                    b"<ComicInfo><Title>Space Cat</Title><Writer>Jane</Writer></ComicInfo>"
                        .as_slice(),
                ),
            ] {
                zip.start_file(name, opts).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        let meta = parse_book(Path::new("/tmp/raw_name.cbz"), buf.get_ref()).unwrap();
        assert_eq!(meta.title, "Space Cat");
        assert_eq!(meta.author, "Jane");
        assert_eq!(meta.cover.as_deref(), Some(b"\x89PNG\r\n\x1a\npage-two".as_slice()));
    }

    #[test]
    fn parses_fb2_and_fbz_metadata_and_cover() {
        let fb2 = r##"<?xml version="1.0" encoding="utf-8"?>
        <FictionBook xmlns:l="http://www.w3.org/1999/xlink">
          <description>
            <title-info>
              <author><first-name>Leo</first-name><last-name>Tolstoy</last-name></author>
              <book-title>War and Peace</book-title>
              <coverpage><image l:href="#cover.png"/></coverpage>
            </title-info>
          </description>
          <binary id="cover.png" content-type="image/png">SGVsbG8h</binary>
        </FictionBook>"##;

        let meta = parse_book(Path::new("/tmp/book.fb2"), fb2.as_bytes()).unwrap();
        assert_eq!(meta.title, "War and Peace");
        assert_eq!(meta.author, "Leo Tolstoy");
        assert_eq!(meta.cover.as_deref(), Some(b"Hello!".as_slice()));

        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            zip.start_file("novel.fb2", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(fb2.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let fbz_meta = parse_book(Path::new("/tmp/book.fb2.zip"), buf.get_ref()).unwrap();
        assert_eq!(fbz_meta.title, "War and Peace");
        assert_eq!(fbz_meta.author, "Leo Tolstoy");
        assert_eq!(fbz_meta.cover.as_deref(), Some(b"Hello!".as_slice()));
    }

    #[test]
    fn validates_mobi_header() {
        let mut bytes = vec![0u8; 80];
        bytes[60..68].copy_from_slice(b"BOOKMOBI");
        let meta = parse_book(Path::new("/tmp/ThreeBody.azw3"), &bytes).unwrap();
        assert_eq!(meta.title, "ThreeBody");
        assert!(meta.cover.is_none());

        assert!(parse_book(Path::new("/tmp/broken.mobi"), b"not-a-mobi").is_err());
    }
}
