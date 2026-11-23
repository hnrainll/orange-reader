use anyhow::{Context, Result};
use percent_encoding::percent_decode_str;
use roxmltree::Document;
use std::{collections::HashMap, fs::File, io::Read, path::Path};
use zip::ZipArchive;

pub struct Epub {
    pub title: String,
    pub author: String,
    pub cover: Option<Vec<u8>>,
}

fn read_entry(zip: &mut ZipArchive<File>, path: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    zip.by_name(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_zip(base: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    let decoded =
        percent_decode_str(relative.split('#').next().unwrap_or(relative)).decode_utf8_lossy();
    for part in decoded.split('/') {
        match part {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

pub fn parse(path: &Path) -> Result<Epub> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    let container = String::from_utf8(read_entry(&mut zip, "META-INF/container.xml")?)?;
    let container_doc = Document::parse(&container)?;
    let opf_path = container_doc
        .descendants()
        .find(|node| node.tag_name().name() == "rootfile")
        .and_then(|node| node.attribute("full-path"))
        .context("EPUB 中缺少 OPF 文件")?
        .to_owned();
    let opf = String::from_utf8(read_entry(&mut zip, &opf_path)?)?;
    let doc = Document::parse(&opf)?;
    let title = doc
        .descendants()
        .find(|node| node.tag_name().name() == "title")
        .and_then(|node| node.text())
        .unwrap_or("Untitled")
        .trim()
        .to_owned();
    let author = doc
        .descendants()
        .find(|node| node.tag_name().name() == "creator")
        .and_then(|node| node.text())
        .unwrap_or("")
        .trim()
        .to_owned();
    let base = opf_path
        .rsplit_once('/')
        .map(|(base, _)| base)
        .unwrap_or("");
    let manifest: HashMap<String, (String, String)> = doc
        .descendants()
        .filter(|node| node.tag_name().name() == "item")
        .filter_map(|node| {
            Some((
                node.attribute("id")?.to_owned(),
                (
                    node.attribute("href")?.to_owned(),
                    node.attribute("properties").unwrap_or("").to_owned(),
                ),
            ))
        })
        .collect();
    let cover_id = doc
        .descendants()
        .find(|node| node.tag_name().name() == "meta" && node.attribute("name") == Some("cover"))
        .and_then(|node| node.attribute("content"));
    let cover_href = cover_id.and_then(|id| manifest.get(id)).or_else(|| {
        manifest
            .values()
            .find(|(_, props)| props.split_whitespace().any(|prop| prop == "cover-image"))
    });
    let cover = cover_href.and_then(|(href, _)| read_entry(&mut zip, &join_zip(base, href)).ok());
    Ok(Epub {
        title,
        author,
        cover,
    })
}

#[cfg(test)]
mod tests {
    use super::{join_zip, parse};
    use std::{fs::File, io::Write};
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn resolves_package_relative_cover() {
        assert_eq!(
            join_zip("OPS/content", "../images/cover.png#front"),
            "OPS/images/cover.png"
        );
    }

    #[test]
    fn imports_image_only_book() {
        let path =
            std::env::temp_dir().join(format!("orange-reader-{}.epub", uuid::Uuid::new_v4()));
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default();
        for (name, body) in [
            (
                "META-INF/container.xml",
                "<container><rootfiles><rootfile full-path='OPS/book.opf'/></rootfiles></container>",
            ),
            (
                "OPS/book.opf",
                "<package><metadata><title>Picture Book</title><creator>Artist</creator></metadata><manifest><item id='page' href='page.xhtml'/><item id='cover' href='../images/cover.png' properties='cover-image'/></manifest><spine><itemref idref='page'/></spine></package>",
            ),
            (
                "OPS/page.xhtml",
                "<html><body><img src='page.png' /></body></html>",
            ),
            ("images/cover.png", "cover-image-bytes"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let epub = parse(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(epub.title, "Picture Book");
        assert_eq!(epub.author, "Artist");
        assert_eq!(epub.cover.as_deref(), Some(b"cover-image-bytes".as_slice()));
    }
}
