import { BrowserWindow } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'

const readerWindows = new Map<number, BrowserWindow>()

export function createReaderWindow(bookId: number): void {
  // If window already open for this book, focus it
  const existing = readerWindows.get(bookId)
  if (existing && !existing.isDestroyed()) {
    existing.focus()
    return
  }

  const readerWindow = new BrowserWindow({
    width: 900,
    height: 700,
    minWidth: 600,
    minHeight: 500,
    show: false,
    titleBarStyle: 'hiddenInset',
    trafficLightPosition: { x: 16, y: 16 },
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false,
      webSecurity: false
    }
  })

  readerWindows.set(bookId, readerWindow)

  readerWindow.on('ready-to-show', () => {
    readerWindow.show()
  })

  readerWindow.on('closed', () => {
    readerWindows.delete(bookId)
  })

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    readerWindow.loadURL(`${process.env['ELECTRON_RENDERER_URL']}?view=reader&bookId=${bookId}`)
  } else {
    readerWindow.loadFile(join(__dirname, '../renderer/index.html'), {
      query: { view: 'reader', bookId: String(bookId) }
    })
  }
}
