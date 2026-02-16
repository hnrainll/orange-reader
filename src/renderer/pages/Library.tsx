import { useState, useEffect, useCallback } from 'react'
import type { Book } from '../../shared/types'
import BookGrid from '../components/BookGrid'
import { useTheme } from '../hooks/useTheme'
import styles from '../styles/Library.module.css'

function Library(): JSX.Element {
  const [books, setBooks] = useState<Book[]>([])
  const [progressMap, setProgressMap] = useState<Map<number, number>>(new Map())
  const { theme, toggleTheme } = useTheme()
  const [isDragging, setIsDragging] = useState(false)
  const [importing, setImporting] = useState(false)

  const loadBooks = useCallback(async () => {
    const allBooks = await window.api.getAllBooks() as Book[]
    setBooks(allBooks)

    const allProgress = await window.api.getAllReadingProgress() as Array<{ book_id: number; fraction: number }>
    const map = new Map<number, number>()
    for (const p of allProgress) {
      map.set(p.book_id, p.fraction)
    }
    setProgressMap(map)
  }, [])

  useEffect(() => {
    loadBooks()
  }, [loadBooks])

  const importFiles = useCallback(async (filePaths: string[]) => {
    if (importing || filePaths.length === 0) return
    setImporting(true)

    try {
      for (const sourcePath of filePaths) {
        // Read the source file to parse metadata first
        const fileBuffer = await window.api.readFile(sourcePath)
        const file = new File([fileBuffer], sourcePath.split('/').pop() || 'book.epub', {
          type: 'application/epub+zip'
        })

        let title = file.name.replace(/\.epub$/i, '')
        let author = ''
        let coverBlob: Blob | null = null

        try {
          const { makeBook } = await import('../../../foliate-js/view.js')
          const book = await makeBook(file)
          if (book.metadata) {
            title = book.metadata.title || title
            const raw = book.metadata.author
            if (raw) {
              author = Array.isArray(raw)
                ? raw.map((a: unknown) => typeof a === 'string' ? a : (a as { name?: string }).name || '').join(', ')
                : typeof raw === 'string' ? raw : (raw as { name?: string }).name || ''
            }
          }
          if (book.getCover) {
            coverBlob = await book.getCover()
          }
        } catch (e) {
          console.warn('Failed to parse epub metadata:', e)
        }

        // Check duplicate by title
        const isDuplicate = await window.api.checkDuplicate(title)
        if (isDuplicate) {
          console.log('Skipping duplicate:', title)
          continue
        }

        // Copy file to userData
        const destPath = await window.api.importBookFile(sourcePath)

        // Add to database
        const newBook = await window.api.addBook(title, author, null, destPath) as Book

        // Save cover if available
        if (coverBlob && newBook.id) {
          try {
            const arrayBuffer = await coverBlob.arrayBuffer()
            const coverPath = await window.api.saveCover(newBook.id, arrayBuffer)
            await window.api.updateCover(newBook.id, coverPath)
          } catch (e) {
            console.warn('Failed to save cover:', e)
          }
        }
      }
    } catch (e) {
      console.error('Import error:', e)
    } finally {
      setImporting(false)
      loadBooks()
    }
  }, [importing, loadBooks])

  const handleBookClick = (book: Book): void => {
    window.api.openReader(book.id)
  }

  const handleBookDelete = async (book: Book): Promise<void> => {
    await window.api.deleteBook(book.id)
    loadBooks()
  }

  const handleImport = async (): Promise<void> => {
    const filePaths = await window.api.openFileDialog()
    if (filePaths.length > 0) {
      await importFiles(filePaths)
    }
  }

  const handleDragOver = (e: React.DragEvent): void => {
    e.preventDefault()
    setIsDragging(true)
  }

  const handleDragLeave = (e: React.DragEvent): void => {
    e.preventDefault()
    if (e.currentTarget === e.target || !e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsDragging(false)
    }
  }

  const handleDrop = async (e: React.DragEvent): Promise<void> => {
    e.preventDefault()
    setIsDragging(false)
    const files = Array.from(e.dataTransfer.files)
    const epubPaths = files
      .filter((f) => f.name.endsWith('.epub'))
      .map((f) => window.api.getPathForFile(f))
    if (epubPaths.length > 0) {
      await importFiles(epubPaths)
    }
  }

  return (
    <div
      className={styles.container}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      <div className={styles.toolbar}>
        <span className={styles.toolbarTitle}>Library</span>
        <div className={styles.toolbarActions}>
          <button className={styles.iconButton} onClick={toggleTheme} title="Toggle theme">
            {theme === 'light' ? '🌙' : '☀️'}
          </button>
          <button className={styles.iconButton} onClick={handleImport} title="Import books" disabled={importing}>
            {importing ? '⏳' : '＋'}
          </button>
        </div>
      </div>

      <div className={styles.content}>
        {books.length === 0 ? (
          <div className={styles.emptyState}>
            <div className={styles.emptyIcon}>📚</div>
            <div className={styles.emptyTitle}>Your Library is Empty</div>
            <div className={styles.emptyText}>
              Import epub files to start reading.<br />
              You can also drag and drop files here.
            </div>
            <button className={styles.importButton} onClick={handleImport} disabled={importing}>
              {importing ? 'Importing...' : 'Import Books'}
            </button>
          </div>
        ) : (
          <BookGrid books={books} progressMap={progressMap} onBookClick={handleBookClick} onBookDelete={handleBookDelete} />
        )}
      </div>

      {isDragging && (
        <div className={styles.dropOverlay}>
          <div className={styles.dropMessage}>Drop epub files to import</div>
        </div>
      )}
    </div>
  )
}

export default Library
