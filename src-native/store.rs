use crate::epub::Epub;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::{Path, PathBuf}};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: String,
    pub file_path: PathBuf,
    pub cover_path: Option<PathBuf>,
    pub fraction: f64,
}

#[derive(Clone, Debug)]
pub struct Annotation {
    pub id: i64,
    pub chapter: usize,
    pub paragraph: usize,
    pub text: String,
    pub color: String,
}

#[derive(Clone, Debug)]
pub struct WebAnnotation {
    pub id: i64,
    pub cfi: String,
    pub color: String,
    pub kind: String,
    pub text: Option<String>,
}

pub struct Store {
    conn: Connection,
    root: PathBuf,
}

impl Store {
    pub fn open() -> Result<Self> {
        let root = if let Some(path) = std::env::var_os("ORANGE_READER_DATA_DIR") {
            PathBuf::from(path)
        } else {
            let base = if cfg!(target_os = "windows") {
                dirs::data_dir().context("Data directory unavailable")?
            } else {
                dirs::config_dir().context("Config directory unavailable")?
            };
            ["Orange Reader", "orange-reader"].into_iter()
                .map(|name| base.join(name))
                .find(|path| path.join("orange-reader.db").exists())
                .unwrap_or_else(|| base.join("orange-reader"))
        };
        fs::create_dir_all(root.join("books"))?;
        fs::create_dir_all(root.join("covers"))?;
        let conn = Connection::open(root.join("orange-reader.db"))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS books (
            id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, author TEXT DEFAULT '',
            cover_path TEXT, file_path TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')));
            CREATE TABLE IF NOT EXISTS reading_progress (
            book_id INTEGER PRIMARY KEY, cfi TEXT NOT NULL, fraction REAL DEFAULT 0,
            updated_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE);
            CREATE TABLE IF NOT EXISTS annotations (
            id INTEGER PRIMARY KEY AUTOINCREMENT, book_id INTEGER NOT NULL,
            cfi TEXT NOT NULL, color TEXT NOT NULL DEFAULT '#FFE066',
            type TEXT NOT NULL DEFAULT 'highlight', text TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE);
            CREATE TABLE IF NOT EXISTS native_progress (
            book_id INTEGER PRIMARY KEY, chapter INTEGER NOT NULL, paragraph INTEGER NOT NULL,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE);
            CREATE TABLE IF NOT EXISTS native_annotations (
            id INTEGER PRIMARY KEY AUTOINCREMENT, book_id INTEGER NOT NULL,
            chapter INTEGER NOT NULL, paragraph INTEGER NOT NULL,
            text TEXT NOT NULL, color TEXT NOT NULL, legacy_id INTEGER UNIQUE,
            FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE);
            CREATE TABLE IF NOT EXISTS native_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
        Ok(Self { conn, root })
    }

    pub fn books(&self) -> Result<Vec<Book>> {
        let mut stmt = self.conn.prepare("SELECT b.id,b.title,b.author,b.file_path,b.cover_path,
            COALESCE(p.fraction,0) FROM books b LEFT JOIN reading_progress p ON p.book_id=b.id
            ORDER BY b.created_at DESC,b.id DESC")?;
        let rows = stmt.query_map([], |r| Ok(Book {
            id: r.get(0)?, title: r.get(1)?, author: r.get(2)?,
            file_path: PathBuf::from(r.get::<_, String>(3)?),
            cover_path: r.get::<_, Option<String>>(4)?.map(PathBuf::from), fraction: r.get(5)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn import(&self, source: &Path, epub: &Epub) -> Result<()> {
        let title = &epub.title;
        let exists: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM books WHERE title=?1)",
            [title], |r| r.get(0))?;
        if exists { anyhow::bail!("Book already exists: {title}"); }
        let file_path = self.root.join("books").join(format!("{}.epub", Uuid::new_v4()));
        fs::copy(source, &file_path)?;
        let result = self.conn.execute("INSERT INTO books(title,author,file_path) VALUES(?1,?2,?3)",
            params![epub.title, epub.author, file_path.to_string_lossy()]);
        let id = match result { Ok(_) => self.conn.last_insert_rowid(), Err(e) => {
            let _ = fs::remove_file(&file_path); return Err(e.into());
        }};
        if let Some(bytes) = &epub.cover {
            let cover_path = self.root.join("covers").join(format!("{id}.jpg"));
            fs::write(&cover_path, bytes)?;
            self.conn.execute("UPDATE books SET cover_path=?1 WHERE id=?2",
                params![cover_path.to_string_lossy(), id])?;
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let book = self.books()?.into_iter().find(|b| b.id == id).context("Book not found")?;
        self.conn.execute("DELETE FROM books WHERE id=?1", [id])?;
        if book.file_path.starts_with(self.root.join("books")) { let _ = fs::remove_file(book.file_path); }
        if let Some(cover) = book.cover_path {
            if cover.starts_with(self.root.join("covers")) { let _ = fs::remove_file(cover); }
        }
        Ok(())
    }

    pub fn position(&self, book_id: i64) -> Result<Option<(usize, usize)>> {
        let mut stmt = self.conn.prepare("SELECT chapter,paragraph FROM native_progress WHERE book_id=?1")?;
        let mut rows = stmt.query([book_id])?;
        if let Some(row) = rows.next()? { Ok(Some((row.get(0)?, row.get(1)?))) }
        else { Ok(None) }
    }

    pub fn save_position(&self, book_id: i64, chapter: usize, paragraph: usize, fraction: f64) -> Result<()> {
        self.conn.execute("INSERT INTO native_progress(book_id,chapter,paragraph) VALUES(?1,?2,?3)
            ON CONFLICT(book_id) DO UPDATE SET chapter=excluded.chapter,paragraph=excluded.paragraph",
            params![book_id, chapter, paragraph])?;
        self.conn.execute("INSERT INTO reading_progress(book_id,cfi,fraction) VALUES(?1,?2,?3)
            ON CONFLICT(book_id) DO UPDATE SET fraction=excluded.fraction,updated_at=datetime('now')",
            params![book_id, "native", fraction])?;
        Ok(())
    }

    pub fn web_progress(&self, book_id: i64) -> Result<Option<(String, f64)>> {
        self.conn.query_row("SELECT cfi,fraction FROM reading_progress WHERE book_id=?1",
            [book_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional().map_err(Into::into)
    }

    pub fn save_web_progress(&self, book_id: i64, cfi: &str, fraction: f64) -> Result<()> {
        self.conn.execute("INSERT INTO reading_progress(book_id,cfi,fraction) VALUES(?1,?2,?3)
            ON CONFLICT(book_id) DO UPDATE SET cfi=excluded.cfi,fraction=excluded.fraction,
            updated_at=datetime('now')", params![book_id, cfi, fraction])?;
        Ok(())
    }

    pub fn web_annotations(&self, book_id: i64) -> Result<Vec<WebAnnotation>> {
        let mut stmt = self.conn.prepare("SELECT id,cfi,color,type,text FROM annotations
            WHERE book_id=?1 ORDER BY id ASC")?;
        let rows = stmt.query_map([book_id], |row| Ok(WebAnnotation {
            id: row.get(0)?, cfi: row.get(1)?, color: row.get(2)?,
            kind: row.get(3)?, text: row.get(4)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn add_web_annotation(&self, book_id: i64, cfi: &str, color: &str, text: &str) -> Result<i64> {
        self.conn.execute("INSERT INTO annotations(book_id,cfi,color,type,text)
            VALUES(?1,?2,?3,'highlight',?4)", params![book_id, cfi, color, text])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn recolor_web_annotation(&self, book_id: i64, cfi: &str, color: &str) -> Result<()> {
        self.conn.execute("UPDATE annotations SET color=?3 WHERE book_id=?1 AND cfi=?2",
            params![book_id, cfi, color])?;
        Ok(())
    }

    pub fn delete_web_annotation(&self, book_id: i64, cfi: &str) -> Result<()> {
        self.conn.execute("DELETE FROM annotations WHERE book_id=?1 AND cfi=?2",
            params![book_id, cfi])?;
        Ok(())
    }

    pub fn annotations(&self, book_id: i64) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare("SELECT id,chapter,paragraph,text,color FROM native_annotations WHERE book_id=?1")?;
        let rows = stmt.query_map([book_id], |r| Ok(Annotation {
            id: r.get(0)?, chapter: r.get(1)?, paragraph: r.get(2)?, text: r.get(3)?, color: r.get(4)?,
        }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn migrate_legacy_annotations(&self, book_id: i64, epub: &Epub) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT id,text,color FROM annotations WHERE book_id=?1 AND text IS NOT NULL")?;
        let rows = stmt.query_map([book_id], |r| Ok((r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (legacy_id, text, color) = row?;
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() { continue; }
            if let Some((chapter, paragraph)) = epub.chapters.iter().enumerate().find_map(|(c, section)|
                section.paragraphs.iter().enumerate().find(|(_, p)| p.contains(&text))
                    .map(|(p, _)| (c, p))) {
                self.conn.execute("INSERT OR IGNORE INTO native_annotations
                    (book_id,chapter,paragraph,text,color,legacy_id) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![book_id, chapter, paragraph, text, color, legacy_id])?;
            }
        }
        Ok(())
    }

    pub fn add_annotation(&self, book_id: i64, chapter: usize, paragraph: usize, text: &str, color: &str) -> Result<()> {
        self.conn.execute("INSERT INTO native_annotations(book_id,chapter,paragraph,text,color) VALUES(?1,?2,?3,?4,?5)",
            params![book_id,chapter,paragraph,text,color])?;
        Ok(())
    }

    pub fn delete_annotation(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM native_annotations WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM native_settings WHERE key=?1", [key], |r| r.get(0)).ok()
    }

    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO native_settings(key,value) VALUES(?1,?2)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        Ok(())
    }
}

#[cfg(test)]
mod web_tests {
    use super::Store;
    use rusqlite::Connection;
    use std::path::PathBuf;

    #[test]
    fn web_progress_and_annotations_round_trip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE reading_progress (
            book_id INTEGER PRIMARY KEY, cfi TEXT NOT NULL, fraction REAL DEFAULT 0,
            updated_at TEXT DEFAULT (datetime('now')));
            CREATE TABLE annotations (
            id INTEGER PRIMARY KEY AUTOINCREMENT, book_id INTEGER NOT NULL,
            cfi TEXT NOT NULL, color TEXT NOT NULL, type TEXT NOT NULL, text TEXT);")
            .unwrap();
        let store = Store { conn, root: PathBuf::new() };
        let cfi = "epubcfi(/6/12!/4/42,/1:53,/1:97)";

        assert!(store.web_progress(7).unwrap().is_none());
        store.save_web_progress(7, cfi, 0.25).unwrap();
        assert_eq!(store.web_progress(7).unwrap(), Some((cfi.to_owned(), 0.25)));
        store.save_web_progress(7, "epubcfi(/6/14!/4)", 0.5).unwrap();
        assert_eq!(store.web_progress(7).unwrap(), Some(("epubcfi(/6/14!/4)".to_owned(), 0.5)));

        let id = store.add_web_annotation(7, cfi, "#FFE066", "选中的文字").unwrap();
        let annotations = store.web_annotations(7).unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].id, id);
        assert_eq!(annotations[0].cfi, cfi);
        assert_eq!(annotations[0].color, "#FFE066");
        assert_eq!(annotations[0].kind, "highlight");
        assert_eq!(annotations[0].text.as_deref(), Some("选中的文字"));
        store.recolor_web_annotation(7, cfi, "#A8E6CF").unwrap();
        assert_eq!(store.web_annotations(7).unwrap()[0].color, "#A8E6CF");
        store.delete_web_annotation(7, cfi).unwrap();
        assert!(store.web_annotations(7).unwrap().is_empty());
    }
}
