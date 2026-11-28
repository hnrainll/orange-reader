use crate::book::BookMeta;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub id: i64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub file_hash: String,
    pub file_path: PathBuf,
    pub title: String,
    pub author: String,
    pub cover_path: Option<PathBuf>,
    pub cfi: Option<String>,
    pub fraction: f64,
    pub chapter_title: Option<String>,
    pub last_opened_at: String,
}

#[derive(Clone, Debug)]
pub struct Annotation {
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

pub fn hash_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

pub fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn cover_extension(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "gif"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "webp"
    } else {
        "jpg"
    }
}

fn map_history_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        file_hash: row.get(1)?,
        file_path: PathBuf::from(row.get::<_, String>(2)?),
        title: row.get(3)?,
        author: row.get(4)?,
        cover_path: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
        cfi: row.get(6)?,
        fraction: row.get(7)?,
        chapter_title: row.get(8)?,
        last_opened_at: row.get(9)?,
    })
}

const HISTORY_SELECT: &str = "SELECT id, file_hash, file_path, title, author, cover_path, \
    cfi, fraction, chapter_title, last_opened_at FROM reading_history";

impl Store {
    pub fn open() -> Result<Self> {
        let root = if let Some(path) = std::env::var_os("VERSO_DATA_DIR") {
            PathBuf::from(path)
        } else {
            let base = if cfg!(target_os = "windows") {
                dirs::data_dir().context("Data directory unavailable")?
            } else {
                dirs::config_dir().context("Config directory unavailable")?
            };
            let target = base.join("verso");
            let legacy = base.join("orange-reader");
            if !target.join("history.db").exists() && legacy.join("history.db").exists() {
                let _ = fs::create_dir_all(&target);
                for file in ["history.db", "history.db-wal", "history.db-shm"] {
                    let src = legacy.join(file);
                    if src.exists() {
                        let _ = fs::copy(src, target.join(file));
                    }
                }
            }
            target
        };
        Self::open_in(root)
    }

    pub fn open_in(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(root.join("covers"))?;
        let conn = Connection::open(root.join("history.db"))?;
        Self::init(conn, root)
    }

    fn init(conn: Connection, root: PathBuf) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reading_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_hash TEXT NOT NULL UNIQUE,
                file_path TEXT NOT NULL,
                title TEXT NOT NULL,
                author TEXT NOT NULL DEFAULT '',
                cover_path TEXT,
                cfi TEXT,
                fraction REAL NOT NULL DEFAULT 0,
                chapter_title TEXT,
                hidden INTEGER NOT NULL DEFAULT 0,
                last_opened_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
            );
            CREATE INDEX IF NOT EXISTS idx_reading_history_last_opened
                ON reading_history(last_opened_at DESC, id DESC);
            CREATE INDEX IF NOT EXISTS idx_reading_history_file_path
                ON reading_history(file_path);
            CREATE TABLE IF NOT EXISTS annotations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id INTEGER NOT NULL,
                cfi TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '#FFE066',
                type TEXT NOT NULL DEFAULT 'highlight',
                text TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
                FOREIGN KEY (book_id) REFERENCES reading_history(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        let has_hidden: bool = conn
            .prepare("PRAGMA table_info(reading_history)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|col| col == "hidden");
        if !has_hidden {
            conn.execute(
                "ALTER TABLE reading_history ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(Self { conn, root })
    }

    pub fn history(&self) -> Result<Vec<HistoryEntry>> {
        let mut stmt = self.conn.prepare(&format!(
            "{HISTORY_SELECT} WHERE hidden=0 ORDER BY last_opened_at DESC, id DESC"
        ))?;
        let rows = stmt.query_map([], map_history_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn get_entry(&self, id: i64) -> Result<Option<HistoryEntry>> {
        self.conn
            .query_row(&format!("{HISTORY_SELECT} WHERE id=?1"), [id], map_history_row)
            .optional()
            .map_err(Into::into)
    }

    fn save_cover_cache(&self, file_hash: &str, cover_bytes: Option<&[u8]>) -> Result<Option<PathBuf>> {
        let Some(bytes) = cover_bytes.filter(|b| !b.is_empty()) else {
            return Ok(None);
        };
        let trimmed = bytes
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .map_or(&b""[..], |pos| &bytes[pos..]);
        if trimmed.starts_with(b"<svg") || trimmed.starts_with(b"<?xml") {
            return Ok(None);
        }
        let path = self
            .root
            .join("covers")
            .join(format!("{file_hash}.{}", cover_extension(bytes)));
        if !path.exists() {
            fs::write(&path, bytes)?;
        }
        Ok(Some(path))
    }

    /// Record opening a supported ebook file on disk without copying it.
    ///
    /// Matching order:
    /// 1. Content SHA-256 hash: seamlessly resumes progress & highlights even if moved/renamed.
    /// 2. Same canonical file path + same title: updates hash if edited in place.
    /// 3. Otherwise creates a new history entry.
    pub fn record_open(&self, path: &Path, bytes: &[u8], book: &BookMeta) -> Result<HistoryEntry> {
        let canonical = normalize_path(path);
        let canonical_str = canonical.to_string_lossy().into_owned();
        let file_hash = hash_bytes(bytes);

        if let Some(existing) = self
            .conn
            .query_row(
                &format!("{HISTORY_SELECT} WHERE file_hash=?1"),
                [&file_hash],
                map_history_row,
            )
            .optional()?
        {
            let cover_path = match existing.cover_path {
                Some(path) if path.exists() => Some(path),
                _ => self.save_cover_cache(&file_hash, book.cover.as_deref())?,
            };
            let title = if book.author.is_empty() && book.cover.is_none() && !existing.title.is_empty() {
                &existing.title
            } else {
                &book.title
            };
            let author = if book.author.is_empty() {
                &existing.author
            } else {
                &book.author
            };
            self.conn.execute(
                "UPDATE reading_history
                 SET file_path=?2, title=?3, author=?4, cover_path=?5, hidden=0,
                     last_opened_at=datetime('now', 'localtime')
                 WHERE id=?1",
                params![
                    existing.id,
                    canonical_str,
                    title,
                    author,
                    cover_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                ],
            )?;
            return self
                .get_entry(existing.id)?
                .context("无法读取已更新的历史记录");
        }

        if let Some(existing) = self
            .conn
            .query_row(
                &format!(
                    "{HISTORY_SELECT} WHERE file_path=?1 AND title=?2
                     ORDER BY last_opened_at DESC, id DESC LIMIT 1"
                ),
                params![&canonical_str, &book.title],
                map_history_row,
            )
            .optional()?
        {
            let cover_path = self
                .save_cover_cache(&file_hash, book.cover.as_deref())?
                .or(existing.cover_path);
            let author = if book.author.is_empty() {
                &existing.author
            } else {
                &book.author
            };
            self.conn.execute(
                "UPDATE reading_history
                 SET file_hash=?2, author=?3, cover_path=?4, hidden=0,
                     last_opened_at=datetime('now', 'localtime')
                 WHERE id=?1",
                params![
                    existing.id,
                    file_hash,
                    author,
                    cover_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                ],
            )?;
            return self
                .get_entry(existing.id)?
                .context("无法读取已更新的历史记录");
        }

        let cover_path = self.save_cover_cache(&file_hash, book.cover.as_deref())?;
        self.conn.execute(
            "INSERT INTO reading_history(file_hash, file_path, title, author, cover_path)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                file_hash,
                canonical_str,
                book.title,
                book.author,
                cover_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_entry(id)?.context("无法读取新建的历史记录")
    }

    /// Relocate a missing history entry to a user-selected file path.
    /// If the new file matches another existing history entry by hash, returns that entry.
    pub fn relocate_entry(
        &self,
        id: i64,
        new_path: &Path,
        bytes: &[u8],
        book: &BookMeta,
    ) -> Result<HistoryEntry> {
        let canonical = normalize_path(new_path);
        let canonical_str = canonical.to_string_lossy().into_owned();
        let file_hash = hash_bytes(bytes);

        if let Some(existing_by_hash) = self
            .conn
            .query_row(
                &format!("{HISTORY_SELECT} WHERE file_hash=?1"),
                [&file_hash],
                map_history_row,
            )
            .optional()?
        {
            if existing_by_hash.id != id {
                return self.record_open(&canonical, bytes, book);
            }
        }

        let cover_path = self.save_cover_cache(&file_hash, book.cover.as_deref())?;
        self.conn.execute(
            "UPDATE reading_history
             SET file_hash=?2, file_path=?3, title=?4, author=?5,
                 cover_path=COALESCE(?6, cover_path), hidden=0,
                 last_opened_at=datetime('now', 'localtime')
             WHERE id=?1",
            params![
                id,
                file_hash,
                canonical_str,
                book.title,
                book.author,
                cover_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            ],
        )?;
        self.get_entry(id)?.context("无法读取重新定位后的历史记录")
    }

    /// Enrich title, author, or cover extracted asynchronously by foliate-js
    /// (e.g. for MOBI / AZW3 or non-UTF-8 FB2 books).
    pub fn update_metadata(
        &self,
        book_id: i64,
        title: Option<&str>,
        author: Option<&str>,
        cover: Option<&[u8]>,
    ) -> Result<bool> {
        let Some(existing) = self.get_entry(book_id)? else {
            return Ok(false);
        };
        let next_title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.title);
        let next_author = author
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&existing.author);
        let has_cover = existing.cover_path.as_ref().is_some_and(|p| p.exists());
        let new_cover = if !has_cover {
            self.save_cover_cache(&existing.file_hash, cover)?
        } else {
            None
        };
        let changed =
            next_title != existing.title || next_author != existing.author || new_cover.is_some();
        if !changed {
            return Ok(false);
        }
        self.conn.execute(
            "UPDATE reading_history
             SET title=?2, author=?3, cover_path=COALESCE(?4, cover_path),
                 updated_at=datetime('now', 'localtime')
             WHERE id=?1",
            params![
                book_id,
                next_title,
                next_author,
                new_cover.as_ref().map(|p| p.to_string_lossy().into_owned()),
            ],
        )?;
        Ok(true)
    }

    /// Hide a book from the Recent Reading list and clear its cached cover,
    /// while preserving reading progress and annotations for when it is opened again.
    /// Never deletes the user's EPUB file on disk.
    pub fn remove_history(&self, id: i64) -> Result<()> {
        let entry = self.get_entry(id)?;
        self.conn.execute(
            "UPDATE reading_history SET hidden=1, cover_path=NULL WHERE id=?1",
            [id],
        )?;
        let covers_dir = self.root.join("covers");
        if let Some(entry) = entry {
            if let Some(cover) = entry.cover_path {
                if cover.starts_with(&covers_dir) {
                    let _ = fs::remove_file(cover);
                }
            }
            let prefix = format!("{}.", entry.file_hash);
            if let Ok(entries) = fs::read_dir(&covers_dir) {
                for file in entries.flatten() {
                    if file
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.starts_with(&prefix))
                    {
                        let _ = fs::remove_file(file.path());
                    }
                }
            }
        }
        Ok(())
    }

    pub fn save_progress(
        &self,
        book_id: i64,
        cfi: &str,
        fraction: f64,
        chapter_title: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE reading_history
             SET cfi=?2, fraction=?3, chapter_title=COALESCE(?4, chapter_title),
                 updated_at=datetime('now', 'localtime')
             WHERE id=?1",
            params![book_id, cfi, fraction, chapter_title],
        )?;
        Ok(())
    }

    pub fn annotations(&self, book_id: i64) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, cfi, color, type, text FROM annotations
             WHERE book_id=?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([book_id], |row| {
            Ok(Annotation {
                id: row.get(0)?,
                cfi: row.get(1)?,
                color: row.get(2)?,
                kind: row.get(3)?,
                text: row.get(4)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn add_annotation(
        &self,
        book_id: i64,
        cfi: &str,
        color: &str,
        text: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO annotations(book_id, cfi, color, type, text)
             VALUES(?1, ?2, ?3, 'highlight', ?4)",
            params![book_id, cfi, color, text],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn recolor_annotation(&self, book_id: i64, cfi: &str, color: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE annotations SET color=?3 WHERE book_id=?1 AND cfi=?2",
            params![book_id, cfi, color],
        )?;
        Ok(())
    }

    pub fn delete_annotation(&self, book_id: i64, cfi: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM annotations WHERE book_id=?1 AND cfi=?2",
            params![book_id, cfi],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .ok()
    }

    pub fn save_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Store, cover_extension, hash_bytes};
    use crate::book::BookMeta;
    use std::fs;

    fn temp_dir() -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("verso-store-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample_book(title: &str) -> BookMeta {
        BookMeta {
            title: title.to_owned(),
            author: "Author".to_owned(),
            cover: Some(b"\x89PNG\r\n\x1a\ncover".to_vec()),
        }
    }

    #[test]
    fn hash_matches_across_moved_file_and_preserves_progress_and_annotations() {
        let root = temp_dir();
        let store = Store::open_in(root.clone()).unwrap();
        let book = sample_book("The Book");
        let bytes = b"fake-epub-binary-content";

        let path_a = root.join("folder_a").join("book.epub");
        let path_b = root.join("folder_b").join("renamed.epub");
        fs::create_dir_all(path_a.parent().unwrap()).unwrap();
        fs::create_dir_all(path_b.parent().unwrap()).unwrap();
        fs::write(&path_a, bytes).unwrap();

        let entry = store.record_open(&path_a, bytes, &book).unwrap();
        assert_eq!(entry.file_hash, hash_bytes(bytes));
        assert!(entry.cover_path.as_ref().is_some_and(|p| p.exists()));

        let cfi = "epubcfi(/6/12!/4/42,/1:10,/1:20)";
        store
            .save_progress(entry.id, cfi, 0.42, Some("第一章"))
            .unwrap();
        store
            .add_annotation(entry.id, cfi, "#FFE066", "选中的文字")
            .unwrap();

        // Move the file to a new path and open again
        fs::rename(&path_a, &path_b).unwrap();
        let reopened = store.record_open(&path_b, bytes, &book).unwrap();
        assert_eq!(reopened.id, entry.id);
        assert_eq!(reopened.file_path, fs::canonicalize(&path_b).unwrap());
        assert_eq!(reopened.cfi.as_deref(), Some(cfi));
        assert_eq!(reopened.fraction, 0.42);
        assert_eq!(reopened.chapter_title.as_deref(), Some("第一章"));

        let annotations = store.annotations(reopened.id).unwrap();
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].text.as_deref(), Some("选中的文字"));

        store
            .recolor_annotation(reopened.id, cfi, "#86EFAC")
            .unwrap();
        assert_eq!(store.annotations(reopened.id).unwrap()[0].color, "#86EFAC");

        // Hiding from history clears cached cover & hides from list,
        // but keeps progress, annotations, and the user's file on disk.
        let cached_cover = reopened.cover_path.clone().unwrap();
        assert!(cached_cover.exists());
        store.remove_history(reopened.id).unwrap();
        assert!(store.history().unwrap().is_empty());
        assert!(!cached_cover.exists());
        assert!(path_b.exists());

        // Re-opening brings the book back to history with fresh cover, progress, and annotations intact.
        let restored = store.record_open(&path_b, bytes, &book).unwrap();
        assert_eq!(restored.id, reopened.id);
        assert_eq!(restored.cfi.as_deref(), Some(cfi));
        assert_eq!(restored.fraction, 0.42);
        assert!(restored.cover_path.as_ref().is_some_and(|p| p.exists()));
        assert_eq!(store.history().unwrap().len(), 1);
        assert_eq!(store.annotations(restored.id).unwrap().len(), 1);

        store.delete_annotation(restored.id, cfi).unwrap();
        assert!(store.annotations(restored.id).unwrap().is_empty());

        // Async metadata enrichment (e.g. MOBI/AZW3) updates title, author, and caches cover.
        let mobi_path = root.join("folder_b").join("sample.azw3");
        let mobi_bytes = b"fake-azw3-binary";
        fs::write(&mobi_path, mobi_bytes).unwrap();
        let mobi_entry = store
            .record_open(
                &mobi_path,
                mobi_bytes,
                &BookMeta {
                    title: "sample".to_owned(),
                    author: String::new(),
                    cover: None,
                },
            )
            .unwrap();
        assert!(mobi_entry.cover_path.is_none());
        let changed = store
            .update_metadata(
                mobi_entry.id,
                Some("Real Title"),
                Some("Real Author"),
                Some(b"\x89PNG\r\n\x1a\nmobi-cover"),
            )
            .unwrap();
        assert!(changed);
        let enriched = store.get_entry(mobi_entry.id).unwrap().unwrap();
        assert_eq!(enriched.title, "Real Title");
        assert_eq!(enriched.author, "Real Author");
        assert!(enriched.cover_path.as_ref().is_some_and(|p| p.exists()));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_round_trip_and_cover_extension() {
        let root = temp_dir();
        let store = Store::open_in(root.clone()).unwrap();
        assert!(store.setting("theme").is_none());
        store.save_setting("theme", "dark").unwrap();
        assert_eq!(store.setting("theme").as_deref(), Some("dark"));
        store.save_setting("theme", "light").unwrap();
        assert_eq!(store.setting("theme").as_deref(), Some("light"));

        assert_eq!(cover_extension(b"\x89PNG\r\n\x1a\n..."), "png");
        assert_eq!(cover_extension(b"GIF89a..."), "gif");
        assert_eq!(cover_extension(b"\xff\xd8\xff..."), "jpg");

        let _ = fs::remove_dir_all(&root);
    }
}
