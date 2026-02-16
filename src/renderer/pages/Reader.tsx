import { useEffect, useRef, useState, useCallback } from 'react'
import type { Book } from '../../shared/types'
import TOCSidebar from '../components/TOCSidebar'
import SettingsPanel from '../components/SettingsPanel'
import { useTheme } from '../hooks/useTheme'
import styles from '../styles/Reader.module.css'

interface TOCItem {
  label: string
  href: string
  subitems?: TOCItem[]
}

interface RelocateDetail {
  fraction?: number
  cfi?: string
  tocItem?: { label?: string; href?: string }
}

function Reader(): JSX.Element {
  const params = new URLSearchParams(window.location.search)
  const bookId = Number(params.get('bookId'))

  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<HTMLElement | null>(null)
  const [book, setBook] = useState<Book | null>(null)
  const [loading, setLoading] = useState(true)
  const [progress, setProgress] = useState(0)
  const [chapterTitle, setChapterTitle] = useState('')
  const [currentHref, setCurrentHref] = useState<string | undefined>()
  const [toc, setToc] = useState<TOCItem[]>([])
  const [showTOC, setShowTOC] = useState(false)
  const [showSettings, setShowSettings] = useState(false)
  const { theme, toggleTheme } = useTheme()
  const [fontSize, setFontSize] = useState(() => {
    const saved = localStorage.getItem('readerFontSize')
    return saved ? parseInt(saved, 10) : 18
  })

  const applyFontSize = useCallback((size: number) => {
    const view = viewRef.current as any
    if (view?.renderer) {
      view.renderer.setStyles?.({
        fontSize: `${size}px`
      })
    }
  }, [])

  const handleFontSizeChange = useCallback((size: number) => {
    setFontSize(size)
    localStorage.setItem('readerFontSize', String(size))
    applyFontSize(size)
  }, [applyFontSize])

  const handleThemeChange = useCallback((newTheme: 'light' | 'dark') => {
    if (newTheme !== theme) {
      toggleTheme()
    }
  }, [theme, toggleTheme])

  const initReader = useCallback(async () => {
    if (!bookId) return

    const bookData = await window.api.getBookById(bookId) as Book | null
    if (!bookData) return
    setBook(bookData)

    // Read the epub file
    const fileBuffer = await window.api.readFile(bookData.file_path)
    const file = new File([fileBuffer], 'book.epub', { type: 'application/epub+zip' })

    // Import foliate-js view module
    await import('../../../foliate-js/view.js')

    // Create and configure the view element
    const view = document.createElement('foliate-view')
    view.style.display = 'block'
    view.style.width = '100%'
    view.style.height = '100%'
    viewRef.current = view
    containerRef.current?.appendChild(view)

    // Forward keyboard events from iframe documents to the main window
    view.addEventListener('load', ((e: CustomEvent<{ doc: Document }>) => {
      const doc = e.detail.doc
      doc.addEventListener('keydown', (ke: KeyboardEvent) => {
        switch (ke.key) {
          case 'ArrowLeft':
            (view as any).goLeft()
            break
          case 'ArrowRight':
            (view as any).goRight()
            break
        }
      })
    }) as EventListener)

    // Listen for relocate events
    view.addEventListener('relocate', ((e: CustomEvent<RelocateDetail>) => {
      const detail = e.detail
      if (detail.fraction != null) {
        setProgress(Math.round(detail.fraction * 100))
      }
      if (detail.tocItem?.label) {
        setChapterTitle(detail.tocItem.label)
      }
      if (detail.tocItem?.href) {
        setCurrentHref(detail.tocItem.href)
      }
    }) as EventListener)

    // Open the book
    await (view as any).open(file)

    // Get TOC
    const bookObj = (view as any).book
    if (bookObj?.toc) {
      setToc(bookObj.toc)
    }

    // Restore reading progress
    const savedProgress = await window.api.getReadingProgress(bookId) as { cfi?: string } | null
    await (view as any).init({
      lastLocation: savedProgress?.cfi || undefined,
      showTextStart: !savedProgress?.cfi
    })

    setLoading(false)
  }, [bookId])

  useEffect(() => {
    initReader()
    return () => {
      if (viewRef.current) {
        (viewRef.current as any).close?.()
      }
    }
  }, [initReader])

  // Save progress on relocate
  useEffect(() => {
    const view = viewRef.current
    if (!view || !bookId) return

    const saveProgress = (e: Event): void => {
      const detail = (e as CustomEvent<RelocateDetail>).detail
      if (detail.cfi != null && detail.fraction != null) {
        window.api.saveReadingProgress(bookId, detail.cfi, detail.fraction)
      }
    }

    view.addEventListener('relocate', saveProgress)
    return () => view.removeEventListener('relocate', saveProgress)
  }, [bookId, loading])

  // Keyboard navigation
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent): void => {
      const view = viewRef.current as any
      if (!view) return

      switch (e.key) {
        case 'ArrowLeft':
          view.goLeft()
          break
        case 'ArrowRight':
          view.goRight()
          break
        case 'Escape':
          setShowTOC(false)
          setShowSettings(false)
          break
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  const handleGoLeft = (): void => {
    (viewRef.current as any)?.goLeft()
  }

  const handleGoRight = (): void => {
    (viewRef.current as any)?.goRight()
  }

  const handleTOCNavigate = (href: string): void => {
    const view = viewRef.current as any
    if (view) {
      view.goTo(href)
      setShowTOC(false)
    }
  }

  return (
    <div className={styles.container}>
      <div className={styles.toolbar}>
        <div className={styles.toolbarLeft}>
          <button className={styles.iconButton} onClick={() => setShowTOC(!showTOC)} title="Table of Contents">
            ☰
          </button>
        </div>
        <div className={styles.toolbarCenter}>
          <div className={styles.toolbarTitle}>{book?.title || 'Loading...'}</div>
          {book?.author && <div className={styles.toolbarAuthor}>{book.author}</div>}
        </div>
        <div className={styles.toolbarRight}>
          <button className={styles.iconButton} onClick={() => setShowSettings(!showSettings)} title="Settings">
            ⚙
          </button>
        </div>
      </div>

      <div className={styles.readerArea}>
        {loading && <div className={styles.loading}>Loading book...</div>}
        <div ref={containerRef} className={styles.viewContainer} />
        <button className={`${styles.navButton} ${styles.navLeft}`} onClick={handleGoLeft} aria-label="Previous page" />
        <button className={`${styles.navButton} ${styles.navRight}`} onClick={handleGoRight} aria-label="Next page" />
      </div>

      <div className={styles.bottomBar}>
        <span className={styles.progressText}>{chapterTitle || ''}</span>
        <span className={styles.progressText}>{progress}%</span>
      </div>

      {showTOC && (
        <TOCSidebar
          toc={toc}
          currentHref={currentHref}
          onNavigate={handleTOCNavigate}
          onClose={() => setShowTOC(false)}
        />
      )}

      {showSettings && (
        <SettingsPanel
          fontSize={fontSize}
          theme={theme}
          onFontSizeChange={handleFontSizeChange}
          onThemeChange={handleThemeChange}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  )
}

export default Reader
