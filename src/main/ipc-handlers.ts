import { ipcMain, dialog, app } from 'electron'
import { createReaderWindow } from './window'
import { join } from 'path'
import { copyFileSync, existsSync, unlinkSync, writeFileSync, mkdirSync } from 'fs'
import { randomUUID } from 'crypto'
import {
  getAllBooks,
  addBook,
  deleteBook,
  getBookById,
  saveReadingProgress,
  getReadingProgress,
  getAllReadingProgress,
  updateBookCover,
  bookExistsByTitle,
  addAnnotation,
  getAnnotations,
  deleteAnnotation
} from './database'

export function registerIpcHandlers(): void {
  ipcMain.handle('db:getAllBooks', () => {
    return getAllBooks()
  })

  ipcMain.handle('db:addBook', (_event, title: string, author: string, coverPath: string | null, filePath: string) => {
    return addBook(title, author, coverPath, filePath)
  })

  ipcMain.handle('db:deleteBook', (_event, id: number) => {
    const book = getBookById(id) as { file_path?: string; cover_path?: string } | undefined
    if (book) {
      // Clean up files
      if (book.file_path && existsSync(book.file_path)) {
        try { unlinkSync(book.file_path) } catch { /* ignore */ }
      }
      if (book.cover_path && existsSync(book.cover_path)) {
        try { unlinkSync(book.cover_path) } catch { /* ignore */ }
      }
      deleteBook(id)
    }
    return true
  })

  ipcMain.handle('db:getBookById', (_event, id: number) => {
    return getBookById(id)
  })

  ipcMain.handle('db:saveReadingProgress', (_event, bookId: number, cfi: string, fraction: number) => {
    saveReadingProgress(bookId, cfi, fraction)
    return true
  })

  ipcMain.handle('db:getReadingProgress', (_event, bookId: number) => {
    return getReadingProgress(bookId)
  })

  ipcMain.handle('db:getAllReadingProgress', () => {
    return getAllReadingProgress()
  })

  ipcMain.handle('dialog:openFile', async () => {
    const result = await dialog.showOpenDialog({
      properties: ['openFile', 'multiSelections'],
      filters: [
        { name: 'eBooks', extensions: ['epub'] }
      ]
    })
    return result.canceled ? [] : result.filePaths
  })

  ipcMain.handle('app:getPath', (_event, name: string) => {
    return app.getPath(name as 'userData')
  })

  ipcMain.handle('book:import', (_event, sourcePath: string) => {
    const userDataPath = app.getPath('userData')
    const booksDir = join(userDataPath, 'books')
    mkdirSync(booksDir, { recursive: true })

    const ext = sourcePath.substring(sourcePath.lastIndexOf('.'))
    const fileName = `${randomUUID()}${ext}`
    const destPath = join(booksDir, fileName)

    copyFileSync(sourcePath, destPath)
    return destPath
  })

  ipcMain.handle('book:saveCover', (_event, bookId: number, coverData: ArrayBuffer) => {
    const userDataPath = app.getPath('userData')
    const coversDir = join(userDataPath, 'covers')
    mkdirSync(coversDir, { recursive: true })

    const coverPath = join(coversDir, `${bookId}.jpg`)
    writeFileSync(coverPath, Buffer.from(coverData))
    return coverPath
  })

  ipcMain.handle('book:checkDuplicate', (_event, title: string) => {
    return bookExistsByTitle(title)
  })

  ipcMain.handle('book:updateCover', (_event, id: number, coverPath: string) => {
    updateBookCover(id, coverPath)
    return true
  })

  ipcMain.handle('book:readFile', async (_event, filePath: string) => {
    const { readFileSync } = require('fs')
    const buffer = readFileSync(filePath)
    return buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength)
  })

  ipcMain.handle('reader:open', (_event, bookId: number) => {
    createReaderWindow(bookId)
    return true
  })

  ipcMain.handle('db:addAnnotation', (_event, bookId: number, cfi: string, color: string, type: string, text: string | null) => {
    return addAnnotation(bookId, cfi, color, type, text)
  })

  ipcMain.handle('db:getAnnotations', (_event, bookId: number) => {
    return getAnnotations(bookId)
  })

  ipcMain.handle('db:deleteAnnotation', (_event, id: number) => {
    deleteAnnotation(id)
    return true
  })
}
