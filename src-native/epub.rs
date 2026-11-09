use anyhow::{anyhow, Context, Result};
use percent_encoding::percent_decode_str;
use roxmltree::Document;
use scraper::{Html, Selector};
use std::{collections::HashMap, fs::File, io::Read, path::Path};
use zip::ZipArchive;

#[derive(Clone, Debug)]
pub struct Chapter {
    pub title: String,
    pub paragraphs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Epub {
    pub title: String,
    pub author: String,
    pub chapters: Vec<Chapter>,
    pub cover: Option<Vec<u8>>,
}

fn read_entry(zip: &mut ZipArchive<File>, path: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    zip.by_name(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_zip(base: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    let decoded = percent_decode_str(relative.split('#').next().unwrap_or(relative)).decode_utf8_lossy();
    for part in decoded.split('/') {
        match part {
            ".." => { parts.pop(); }
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
    let opf_path = container_doc.descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .context("EPUB has no package document")?
        .to_owned();
    let opf = String::from_utf8(read_entry(&mut zip, &opf_path)?)?;
    let doc = Document::parse(&opf)?;
    let title = doc.descendants().find(|n| n.tag_name().name() == "title")
        .and_then(|n| n.text()).unwrap_or("Untitled").trim().to_owned();
    let author = doc.descendants().find(|n| n.tag_name().name() == "creator")
        .and_then(|n| n.text()).unwrap_or("").trim().to_owned();
    let base = opf_path.rsplit_once('/').map(|(b, _)| b).unwrap_or("");
    let manifest: HashMap<String, (String, String)> = doc.descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter_map(|n| Some((n.attribute("id")?.to_owned(),
            (n.attribute("href")?.to_owned(), n.attribute("properties").unwrap_or("").to_owned()))))
        .collect();
    let cover_id = doc.descendants().find(|n| n.has_tag_name("meta") && n.attribute("name") == Some("cover"))
        .and_then(|n| n.attribute("content"));
    let cover_href = cover_id.and_then(|id| manifest.get(id)).or_else(||
        manifest.values().find(|(_, props)| props.split_whitespace().any(|p| p == "cover-image")));
    let cover = cover_href.and_then(|(href, _)| read_entry(&mut zip, &join_zip(base, href)).ok());

    let mut nav_titles = HashMap::new();
    if let Some((nav_href, _)) = manifest.values().find(|(_, props)|
        props.split_whitespace().any(|p| p == "nav")) {
        let nav_path = join_zip(base, nav_href);
        if let Ok(bytes) = read_entry(&mut zip, &nav_path) {
            let nav = Html::parse_document(&String::from_utf8_lossy(&bytes));
            let links = Selector::parse("nav a[href]").unwrap();
            let nav_base = nav_path.rsplit_once('/').map(|(b, _)| b).unwrap_or("");
            for link in nav.select(&links) {
                let Some(href) = link.value().attr("href") else { continue };
                let title = link.text().collect::<Vec<_>>().join(" ");
                if !title.trim().is_empty() {
                    nav_titles.entry(join_zip(nav_base, href)).or_insert(title.trim().to_owned());
                }
            }
        }
    }

    let paragraph_selector = Selector::parse("h1,h2,h3,h4,p,li,blockquote,pre").unwrap();
    let heading_selector = Selector::parse("h1,h2,h3").unwrap();
    let mut chapters = Vec::new();
    for itemref in doc.descendants().filter(|n| n.has_tag_name("itemref")) {
        let Some(idref) = itemref.attribute("idref") else { continue };
        let Some((href, _)) = manifest.get(idref) else { continue };
        let item_path = join_zip(base, href);
        let bytes = read_entry(&mut zip, &item_path)
            .with_context(|| format!("Cannot read chapter {href}"))?;
        let html = Html::parse_document(&String::from_utf8_lossy(&bytes));
        let mut paragraphs = Vec::new();
        for el in html.select(&paragraph_selector) {
            let value = el.text().collect::<Vec<_>>().join(" ");
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if !value.is_empty() { paragraphs.push(value); }
        }
        if paragraphs.is_empty() {
            let body = Selector::parse("body").unwrap();
            if let Some(body) = html.select(&body).next() {
                let value = body.text().collect::<Vec<_>>().join(" ");
                let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
                if !value.is_empty() { paragraphs.push(value); }
            }
        }
        if paragraphs.is_empty() { continue; }
        let chapter_title = nav_titles.get(&item_path).cloned().or_else(|| html.select(&heading_selector).next()
            .map(|e| e.text().collect::<Vec<_>>().join(" "))
            .filter(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| format!("Chapter {}", chapters.len() + 1));
        chapters.push(Chapter { title: chapter_title, paragraphs });
    }
    if chapters.is_empty() { return Err(anyhow!("EPUB contains no readable chapters")); }
    Ok(Epub { title, author, chapters, cover })
}

#[cfg(test)]
mod tests {
    use super::{join_zip, parse};
    use std::{fs::File, io::Write};
    use zip::{write::SimpleFileOptions, ZipWriter};
    #[test]
    fn package_relative_paths() {
        assert_eq!(join_zip("OPS/content", "../images/cover.jpg"), "OPS/images/cover.jpg");
    }

    #[test]
    fn parses_spine_in_order() {
        let path = std::env::temp_dir().join(format!("orange-reader-test-{}.epub", uuid::Uuid::new_v4()));
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default();
        for (name, body) in [
            ("META-INF/container.xml", "<container><rootfiles><rootfile full-path='OPS/book.opf'/></rootfiles></container>"),
            ("OPS/book.opf", "<package><metadata><title>Example</title><creator>Author</creator></metadata><manifest><item id='a' href='chapters/a.xhtml'/><item id='b' href='chapters/b.xhtml'/><item id='nav' href='nav.xhtml' properties='nav'/></manifest><spine><itemref idref='b'/><itemref idref='a'/></spine></package>"),
            ("OPS/nav.xhtml", "<html><body><nav><a href='chapters/b.xhtml#start'>Navigation title</a></nav></body></html>"),
            ("OPS/chapters/a.xhtml", "<html><body><h1>First</h1><p>Alpha paragraph.</p></body></html>"),
            ("OPS/chapters/b.xhtml", "<html><body><h1>Second</h1><p>Beta paragraph.</p></body></html>"),
        ] {
            zip.start_file(name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let result = parse(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(result.title, "Example");
        assert_eq!(result.author, "Author");
        assert_eq!(result.chapters[0].title, "Navigation title");
        assert_eq!(result.chapters[1].title, "First");
    }
}
