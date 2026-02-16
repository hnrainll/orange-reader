import { app, BrowserWindow, shell, protocol, net, nativeImage } from 'electron'
import { join } from 'path'
import { is } from '@electron-toolkit/utils'
import { initDatabase } from './database'
import { registerIpcHandlers } from './ipc-handlers'

function createWindow(): void {
  const mainWindow = new BrowserWindow({
    width: 1200,
    height: 800,
    minWidth: 800,
    minHeight: 600,
    show: false,
    icon: join(__dirname, '../../build/icon.icns'),
    titleBarStyle: 'hiddenInset',
    trafficLightPosition: { x: 16, y: 16 },
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false,
      webSecurity: false
    }
  })

  mainWindow.on('ready-to-show', () => {
    mainWindow.show()
  })

  mainWindow.webContents.setWindowOpenHandler((details) => {
    shell.openExternal(details.url)
    return { action: 'deny' }
  })

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    mainWindow.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

protocol.registerSchemesAsPrivileged([
  { scheme: 'safe-file', privileges: { bypassCSP: true, supportFetchAPI: true } }
])

app.whenReady().then(() => {
  // Set dock icon in dev mode (packaged app uses .app bundle icon automatically)
  if (process.platform === 'darwin' && is.dev) {
    const icon = nativeImage.createFromPath(join(__dirname, '../../icon_macos.png'))
    if (!icon.isEmpty()) app.dock.setIcon(icon)
  }

  protocol.handle('safe-file', (request) => {
    const filePath = decodeURIComponent(request.url.replace('safe-file://', ''))
    return net.fetch('file://' + filePath)
  })

  initDatabase()
  registerIpcHandlers()
  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
