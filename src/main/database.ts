import Database from 'better-sqlite3'
import { app } from 'electron'
import { join } from 'path'
import { mkdirSync } from 'fs'

let db: Database.Database

export function initDatabase(): void {
  const userDataPath = app.getPath('userData')
  const dbPath = join(userDataPath, 'orange-reader.db')

  // Ensure books and covers directories exist
  mkdirSync(join(userDataPath, 'books'), { recursive: true })
  mkdirSync(join(userDataPath, 'covers'), { recursive: true })

  db = new Database(dbPath)
  db.pragma('journal_mode = WAL')
  db.pragma('foreign_keys = ON')

  db.exec(`
    CREATE TABLE IF NOT EXISTS books (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      title TEXT NOT NULL,
      author TEXT DEFAULT '',
      cover_path TEXT,
      file_path TEXT NOT NULL,
      created_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS reading_progress (
      book_id INTEGER PRIMARY KEY,
      cfi TEXT NOT NULL,
      fraction REAL DEFAULT 0,
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS annotations (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      book_id INTEGER NOT NULL,
      cfi TEXT NOT NULL,
      color TEXT NOT NULL DEFAULT '#FFE066',
      type TEXT NOT NULL DEFAULT 'highlight',
      text TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
    );
  `)
}

export function getDatabase(): Database.Database {
  return db
}

export function getAllBooks(): unknown[] {
  return db.prepare('SELECT * FROM books ORDER BY created_at DESC').all()
}

export function addBook(title: string, author: string, coverPath: string | null, filePath: string): unknown {
  const stmt = db.prepare('INSERT INTO books (title, author, cover_path, file_path) VALUES (?, ?, ?, ?)')
  const result = stmt.run(title, author, coverPath, filePath)
  return db.prepare('SELECT * FROM books WHERE id = ?').get(result.lastInsertRowid)
}

export function deleteBook(id: number): void {
  db.prepare('DELETE FROM books WHERE id = ?').run(id)
}

export function getBookById(id: number): unknown {
  return db.prepare('SELECT * FROM books WHERE id = ?').get(id)
}

export function saveReadingProgress(bookId: number, cfi: string, fraction: number): void {
  db.prepare(`
    INSERT INTO reading_progress (book_id, cfi, fraction, updated_at)
    VALUES (?, ?, ?, datetime('now'))
    ON CONFLICT(book_id) DO UPDATE SET cfi = ?, fraction = ?, updated_at = datetime('now')
  `).run(bookId, cfi, fraction, cfi, fraction)
}

export function getReadingProgress(bookId: number): unknown {
  return db.prepare('SELECT * FROM reading_progress WHERE book_id = ?').get(bookId)
}

export function getAllReadingProgress(): unknown[] {
  return db.prepare('SELECT * FROM reading_progress').all()
}

export function updateBookCover(id: number, coverPath: string): void {
  db.prepare('UPDATE books SET cover_path = ? WHERE id = ?').run(coverPath, id)
}

export function bookExistsByTitle(title: string): boolean {
  const row = db.prepare('SELECT 1 FROM books WHERE title = ?').get(title) as unknown
  return !!row
}

export function addAnnotation(bookId: number, cfi: string, color: string, type: string, text: string | null): unknown {
  const stmt = db.prepare('INSERT INTO annotations (book_id, cfi, color, type, text) VALUES (?, ?, ?, ?, ?)')
  const result = stmt.run(bookId, cfi, color, type, text)
  return db.prepare('SELECT * FROM annotations WHERE id = ?').get(result.lastInsertRowid)
}

export function getAnnotations(bookId: number): unknown[] {
  return db.prepare('SELECT * FROM annotations WHERE book_id = ? ORDER BY created_at ASC').all(bookId)
}

export function deleteAnnotation(id: number): void {
  db.prepare('DELETE FROM annotations WHERE id = ?').run(id)
}
