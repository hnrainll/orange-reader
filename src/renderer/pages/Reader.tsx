import { useEffect, useRef, useState, useCallback } from 'react'
import type { Book, Annotation } from '../../shared/types'
import TOCSidebar from '../components/TOCSidebar'
import SettingsPanel from '../components/SettingsPanel'
import HighlightPopup from '../components/HighlightPopup'
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

interface SelectionPopup {
  x: number
  y: number
  // Create mode
  cfi?: string
  text?: string
  // Edit mode (clicking existing annotation)
  existingAnnotationId?: number
  existingCfi?: string
  existingText?: string
}

function getReaderCSS(
  fontSize: number,
  theme: 'light' | 'dark',
  lineHeight: number,
  fontFamily: 'original' | 'serif' | 'sans-serif',
  letterSpacing: number,
  paragraphSpacing: number
): string {
  const darkCSS = theme === 'dark' ? `
    html, body {
      background-color: #1c1c1e !important;
      color: #f5f5f7 !important;
    }
    a { color: #ff8c5a !important; }
  ` : ''

  const fontFamilyCSS = fontFamily === 'serif'
    ? `font-family: "Georgia", "STSong", "SimSun", serif !important;`
    : fontFamily === 'sans-serif'
    ? `font-family: "system-ui", "-apple-system", "PingFang SC", "Microsoft YaHei", sans-serif !important;`
    : ''

  return `
    html, body {
      font-size: ${fontSize}px !important;
      -webkit-text-size-adjust: none;
      text-size-adjust: none;
      line-height: ${lineHeight} !important;
      letter-spacing: ${letterSpacing}em !important;
      ${fontFamilyCSS}
    }
    p { margin-bottom: ${paragraphSpacing}em !important; }
    ${darkCSS}
  `
}

function Reader(): JSX.Element {
  const params = new URLSearchParams(window.location.search)
  const bookId = Number(params.get('bookId'))

  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<HTMLElement | null>(null)
  const initGenRef = useRef(0)
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
  const [lineHeight, setLineHeight] = useState(() =>
    parseFloat(localStorage.getItem('readerLineHeight') || '1.6'))
  const [fontFamily, setFontFamily] = useState<'original' | 'serif' | 'sans-serif'>(() =>
    (localStorage.getItem('readerFontFamily') as 'original' | 'serif' | 'sans-serif') || 'original')
  const [letterSpacing, setLetterSpacing] = useState(() =>
    parseFloat(localStorage.getItem('readerLetterSpacing') || '0'))
  const [paragraphSpacing, setParagraphSpacing] = useState(() =>
    parseFloat(localStorage.getItem('readerParagraphSpacing') || '1'))
  const [contentWidth, setContentWidth] = useState(() =>
    parseInt(localStorage.getItem('readerContentWidth') || '720', 10))

  // Highlight state
  const [selectionPopup, setSelectionPopup] = useState<SelectionPopup | null>(null)
  // Refs for annotation data — used in event listeners to avoid stale closures
  const annotationsRef = useRef<Annotation[]>([])
  const currentSectionRef = useRef<{ doc: Document; index: number } | null>(null)
  // Overlayer static methods — populated after dynamic import
  const OverlayerRef = useRef<{ highlight: Function; underline: Function } | null>(null)

  const initReader = useCallback(async () => {
    if (!bookId) return

    const gen = ++initGenRef.current

    const bookData = await window.api.getBookById(bookId) as Book | null
    if (gen !== initGenRef.current || !bookData) return
    setBook(bookData)

    const fileBuffer = await window.api.readFile(bookData.file_path)
    if (gen !== initGenRef.current) return
    const file = new File([fileBuffer], 'book.epub', { type: 'application/epub+zip' })

    await import('../../../foliate-js/view.js')
    if (gen !== initGenRef.current) return

    // Import Overlayer for draw functions
    const { Overlayer: Ol } = await import('../../../foliate-js/overlayer.js') as any
    if (gen !== initGenRef.current) return
    OverlayerRef.current = Ol

    // Clean up container before appending new view
    if (viewRef.current) {
      (viewRef.current as any).close?.()
      viewRef.current.remove()
      viewRef.current = null
    }
    if (containerRef.current) {
      containerRef.current.innerHTML = ''
    }

    const view = document.createElement('foliate-view')
    view.style.display = 'block'
    view.style.width = '100%'
    view.style.height = '100%'
    viewRef.current = view

    containerRef.current?.appendChild(view)

    // ── Annotation: draw-annotation ──────────────────────────────────────
    // Called by foliate-js when an annotation needs to be rendered onto the Overlayer SVG.
    // We provide the draw function (highlight/underline) and the color from the annotation data.
    view.addEventListener('draw-annotation', ((e: CustomEvent) => {
      const { draw, annotation } = e.detail
      const Ol = OverlayerRef.current as any
      if (!Ol) return
      const color = annotation.color || '#FFE066'
      const type = annotation.type || 'highlight'
      draw(type === 'underline' ? Ol.underline : Ol.highlight, { color })
    }) as EventListener)

    // ── Annotation: show-annotation ──────────────────────────────────────
    // Called when user clicks on an existing annotation in the iframe.
    // We find the annotation by CFI and show an edit popup at the click position.
    view.addEventListener('show-annotation', ((e: CustomEvent) => {
      const { value, range } = e.detail
      const ann = annotationsRef.current.find(a => a.cfi === value)
      if (!ann) return
      const section = currentSectionRef.current
      if (!section) return
      const iframe = section.doc.defaultView?.frameElement as HTMLIFrameElement | null
      if (!iframe) return
      const iframeRect = iframe.getBoundingClientRect()
      const rangeRect = range.getBoundingClientRect()
      setSelectionPopup({
        x: iframeRect.left + rangeRect.left + rangeRect.width / 2,
        y: iframeRect.top + rangeRect.top,
        existingAnnotationId: ann.id,
        existingCfi: ann.cfi,
        existingText: ann.text || undefined,
      })
    }) as EventListener)

    // ── Section load: keyboard + selection + apply annotations ───────────
    view.addEventListener('load', ((e: CustomEvent<{ doc: Document; index: number }>) => {
      const { doc, index } = e.detail
      currentSectionRef.current = { doc, index }

      // Forward keyboard navigation from inside the iframe
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

      // Show highlight popup on mouseup when text is selected
      doc.addEventListener('mouseup', () => {
        const sel = doc.getSelection()
        if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
          setSelectionPopup(null)
          return
        }
        const range = sel.getRangeAt(0)
        const text = sel.toString().trim()
        if (!text) {
          setSelectionPopup(null)
          return
        }
        const iframe = doc.defaultView?.frameElement as HTMLIFrameElement | null
        if (!iframe) return
        const iframeRect = iframe.getBoundingClientRect()
        const rangeRect = range.getBoundingClientRect()
        const cfi = (viewRef.current as any)?.getCFI?.(index, range)
        if (!cfi) return
        setSelectionPopup({
          x: iframeRect.left + rangeRect.left + rangeRect.width / 2,
          y: iframeRect.top + rangeRect.top,
          cfi,
          text,
        })
      })

      // Hide popup when selection collapses (user clicked without selecting)
      doc.addEventListener('selectionchange', () => {
        const sel = doc.getSelection()
        if (!sel || sel.isCollapsed) {
          // Only clear create-mode popup; preserve edit-mode popup (user clicked annotation)
          setSelectionPopup(prev => (prev?.existingAnnotationId != null ? prev : null))
        }
      })

      // Apply any saved annotations that belong to this section.
      // addAnnotation resolves the CFI and only draws if the section's overlayer exists.
      const currentView = viewRef.current as any
      for (const ann of annotationsRef.current) {
        currentView?.addAnnotation?.({ value: ann.cfi, color: ann.color, type: ann.type })
      }
    }) as EventListener)

    // Listen for relocate events (progress saving)
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
      if (bookId && detail.cfi != null && detail.fraction != null) {
        window.api.saveReadingProgress(bookId, detail.cfi, detail.fraction)
      }
    }) as EventListener)

    await (view as any).open(file)
    if (gen !== initGenRef.current) return

    // Apply initial styles
    const savedSize = localStorage.getItem('readerFontSize')
    const currentSize = savedSize ? parseInt(savedSize, 10) : 18
    const savedTheme = (localStorage.getItem('theme') as 'light' | 'dark') || 'light'
    const savedLineHeight = parseFloat(localStorage.getItem('readerLineHeight') || '1.6')
    const savedFontFamily = (localStorage.getItem('readerFontFamily') as 'original' | 'serif' | 'sans-serif') || 'original'
    const savedLetterSpacing = parseFloat(localStorage.getItem('readerLetterSpacing') || '0')
    const savedParagraphSpacing = parseFloat(localStorage.getItem('readerParagraphSpacing') || '1')
    const savedContentWidth = parseInt(localStorage.getItem('readerContentWidth') || '720', 10)
    ;(view as any).renderer.setStyles?.(getReaderCSS(currentSize, savedTheme, savedLineHeight, savedFontFamily, savedLetterSpacing, savedParagraphSpacing))
    ;(view as any).renderer.setAttribute?.('max-inline-size', `${savedContentWidth}px`)

    const bookObj = (view as any).book
    if (bookObj?.toc) {
      setToc(bookObj.toc)
    }

    // Restore reading progress
    const savedProgress = await window.api.getReadingProgress(bookId) as { cfi?: string } | null
    const cfi = savedProgress?.cfi
    try {
      await (view as any).init({
        lastLocation: cfi || undefined,
        showTextStart: !cfi
      })
    } catch (e) {
      console.warn('Failed to restore reading position, starting from beginning:', e)
      await (view as any).init({ showTextStart: true })
    }

    if (gen !== initGenRef.current) return

    // Load saved annotations from DB and apply to the current section
    const savedAnnotations = await window.api.getAnnotations(bookId) as Annotation[]
    if (gen !== initGenRef.current) return
    annotationsRef.current = savedAnnotations
    for (const ann of savedAnnotations) {
      (view as any).addAnnotation?.({ value: ann.cfi, color: ann.color, type: ann.type })
    }

    setLoading(false)
  }, [bookId])

  // ── Highlight handlers ─────────────────────────────────────────────────

  // Called when user clicks a color swatch in the popup
  const handleSelectColor = useCallback(async (color: string) => {
    if (!selectionPopup) return
    const view = viewRef.current as any

    if (selectionPopup.existingAnnotationId != null && selectionPopup.existingCfi) {
      // Edit mode: delete old annotation, create new with chosen color
      const { existingAnnotationId: id, existingCfi: oldCfi, existingText: text } = selectionPopup
      setSelectionPopup(null)
      await window.api.deleteAnnotation(id)
      view?.deleteAnnotation?.({ value: oldCfi })
      const newAnn = await window.api.addAnnotation(bookId, oldCfi, color, 'highlight', text || null) as Annotation
      const updated = [...annotationsRef.current.filter(a => a.id !== id), newAnn]
      annotationsRef.current = updated
      view?.addAnnotation?.({ value: oldCfi, color, type: 'highlight' })
    } else if (selectionPopup.cfi) {
      // Create mode: save new annotation with the chosen color
      const { cfi, text } = selectionPopup
      setSelectionPopup(null)
      const newAnn = await window.api.addAnnotation(bookId, cfi, color, 'highlight', text || null) as Annotation
      annotationsRef.current = [...annotationsRef.current, newAnn]
      view?.addAnnotation?.({ value: cfi, color, type: 'highlight' })
      // Clear the text selection so it doesn't interfere with further interaction
      currentSectionRef.current?.doc?.getSelection()?.removeAllRanges()
    }
  }, [selectionPopup, bookId])

  // Called when user clicks the delete button in the popup
  const handleDeleteAnnotation = useCallback(async () => {
    if (!selectionPopup?.existingAnnotationId || !selectionPopup?.existingCfi) return
    const { existingAnnotationId: id, existingCfi: oldCfi } = selectionPopup
    setSelectionPopup(null)
    await window.api.deleteAnnotation(id)
    const view = viewRef.current as any
    view?.deleteAnnotation?.({ value: oldCfi })
    annotationsRef.current = annotationsRef.current.filter(a => a.id !== id)
  }, [selectionPopup])

  // ── Settings handlers ──────────────────────────────────────────────────

  const handleFontSizeChange = useCallback((size: number) => {
    setFontSize(size)
    localStorage.setItem('readerFontSize', String(size))
    const view = viewRef.current as any
    if (!view?.renderer) return
    view.renderer.setStyles(getReaderCSS(size, theme, lineHeight, fontFamily, letterSpacing, paragraphSpacing))
  }, [theme, lineHeight, fontFamily, letterSpacing, paragraphSpacing])

  const handleThemeChange = useCallback((newTheme: 'light' | 'dark') => {
    if (newTheme !== theme) {
      toggleTheme()
    }
    const view = viewRef.current as any
    if (!view?.renderer) return
    view.renderer.setStyles(getReaderCSS(fontSize, newTheme, lineHeight, fontFamily, letterSpacing, paragraphSpacing))
  }, [theme, toggleTheme, fontSize, lineHeight, fontFamily, letterSpacing, paragraphSpacing])

  const handleLineHeightChange = useCallback((val: number) => {
    setLineHeight(val)
    localStorage.setItem('readerLineHeight', String(val))
    const view = viewRef.current as any
    if (!view?.renderer) return
    view.renderer.setStyles(getReaderCSS(fontSize, theme, val, fontFamily, letterSpacing, paragraphSpacing))
  }, [fontSize, theme, fontFamily, letterSpacing, paragraphSpacing])

  const handleFontFamilyChange = useCallback((val: 'original' | 'serif' | 'sans-serif') => {
    setFontFamily(val)
    localStorage.setItem('readerFontFamily', val)
    const view = viewRef.current as any
    if (!view?.renderer) return
    view.renderer.setStyles(getReaderCSS(fontSize, theme, lineHeight, val, letterSpacing, paragraphSpacing))
  }, [fontSize, theme, lineHeight, letterSpacing, paragraphSpacing])

  const handleLetterSpacingChange = useCallback((val: number) => {
    setLetterSpacing(val)
    localStorage.setItem('readerLetterSpacing', String(val))
    const view = viewRef.current as any
    if (!view?.renderer) return
    view.renderer.setStyles(getReaderCSS(fontSize, theme, lineHeight, fontFamily, val, paragraphSpacing))
  }, [fontSize, theme, lineHeight, fontFamily, paragraphSpacing])

  const handleParagraphSpacingChange = useCallback((val: number) => {
    setParagraphSpacing(val)
    localStorage.setItem('readerParagraphSpacing', String(val))
    const view = viewRef.current as any
    if (!view?.renderer) return
    view.renderer.setStyles(getReaderCSS(fontSize, theme, lineHeight, fontFamily, letterSpacing, val))
  }, [fontSize, theme, lineHeight, fontFamily, letterSpacing])

  const handleContentWidthChange = useCallback((val: number) => {
    setContentWidth(val)
    localStorage.setItem('readerContentWidth', String(val))
    const view = viewRef.current as any
    view?.renderer?.setAttribute?.('max-inline-size', `${val}px`)
  }, [])

  useEffect(() => {
    initReader()
    return () => {
      if (viewRef.current) {
        (viewRef.current as any).close?.()
        viewRef.current.remove()
        viewRef.current = null
      }
    }
  }, [initReader])

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
          setSelectionPopup(null)
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
          lineHeight={lineHeight}
          fontFamily={fontFamily}
          letterSpacing={letterSpacing}
          paragraphSpacing={paragraphSpacing}
          contentWidth={contentWidth}
          onFontSizeChange={handleFontSizeChange}
          onThemeChange={handleThemeChange}
          onLineHeightChange={handleLineHeightChange}
          onFontFamilyChange={handleFontFamilyChange}
          onLetterSpacingChange={handleLetterSpacingChange}
          onParagraphSpacingChange={handleParagraphSpacingChange}
          onContentWidthChange={handleContentWidthChange}
          onClose={() => setShowSettings(false)}
        />
      )}

      {selectionPopup && (
        <HighlightPopup
          x={selectionPopup.x}
          y={selectionPopup.y}
          hasExisting={selectionPopup.existingAnnotationId != null}
          onSelectColor={handleSelectColor}
          onDelete={selectionPopup.existingAnnotationId != null ? handleDeleteAnnotation : undefined}
        />
      )}
    </div>
  )
}

export default Reader
