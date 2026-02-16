import { contextBridge, ipcRenderer } from 'electron'

const api = {
  // Database operations
  getAllBooks: (): Promise<unknown[]> => ipcRenderer.invoke('db:getAllBooks'),
  addBook: (title: string, author: string, coverPath: string | null, filePath: string): Promise<unknown> =>
    ipcRenderer.invoke('db:addBook', title, author, coverPath, filePath),
  deleteBook: (id: number): Promise<boolean> => ipcRenderer.invoke('db:deleteBook', id),
  getBookById: (id: number): Promise<unknown> => ipcRenderer.invoke('db:getBookById', id),

  // Reading progress
  saveReadingProgress: (bookId: number, cfi: string, fraction: number): Promise<boolean> =>
    ipcRenderer.invoke('db:saveReadingProgress', bookId, cfi, fraction),
  getReadingProgress: (bookId: number): Promise<unknown> =>
    ipcRenderer.invoke('db:getReadingProgress', bookId),
  getAllReadingProgress: (): Promise<unknown[]> =>
    ipcRenderer.invoke('db:getAllReadingProgress'),

  // Dialogs
  openFileDialog: (): Promise<string[]> => ipcRenderer.invoke('dialog:openFile'),

  // App paths
  getPath: (name: string): Promise<string> => ipcRenderer.invoke('app:getPath', name),

  // Book import
  importBookFile: (sourcePath: string): Promise<string> => ipcRenderer.invoke('book:import', sourcePath),
  saveCover: (bookId: number, coverData: ArrayBuffer): Promise<string> =>
    ipcRenderer.invoke('book:saveCover', bookId, coverData),
  checkDuplicate: (title: string): Promise<boolean> => ipcRenderer.invoke('book:checkDuplicate', title),
  updateCover: (id: number, coverPath: string): Promise<boolean> => ipcRenderer.invoke('book:updateCover', id, coverPath),
  readFile: (filePath: string): Promise<ArrayBuffer> => ipcRenderer.invoke('book:readFile', filePath),

  // Reader
  openReader: (bookId: number): Promise<boolean> => ipcRenderer.invoke('reader:open', bookId)
}

contextBridge.exposeInMainWorld('api', api)

export type ApiType = typeof api
