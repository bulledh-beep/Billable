import { app, BrowserWindow, globalShortcut } from 'electron'
import path from 'path'
import { initDatabase } from './database'
import { registerIpcHandlers } from './ipc'
import { createTray } from './tray'
import { createMenu } from './menu'
import { TimerManager } from './timer-manager'

let mainWindow: BrowserWindow | null = null
let timerManager: TimerManager

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 960,
    minHeight: 600,
    titleBarStyle: 'hiddenInset',
    trafficLightPosition: { x: 16, y: 18 },
    backgroundColor: '#0F0F11',
    show: false,
    webPreferences: {
      preload: path.join(__dirname, '../preload/preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
    },
  })

  if (process.env.VITE_DEV_SERVER_URL) {
    mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL)
  } else {
    mainWindow.loadFile(path.join(__dirname, '../../dist/index.html'))
  }

  mainWindow.once('ready-to-show', () => {
    mainWindow?.show()
  })

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

app.whenReady().then(() => {
  initDatabase()
  createWindow()
  createMenu(mainWindow!)

  // Create TimerManager (picks up any running timer from DB)
  timerManager = new TimerManager()

  // Create tray and wire it to TimerManager
  createTray(mainWindow!, timerManager)

  // Pass TimerManager to IPC so start/stop go through it
  registerIpcHandlers(timerManager)

  // Global shortcut: Cmd+Shift+Space to toggle timer
  globalShortcut.register('CommandOrControl+Shift+Space', () => {
    if (timerManager.getActive()) {
      timerManager.stop()
    } else {
      // Can't start without a project — show main window
      mainWindow?.show()
      mainWindow?.focus()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})

app.on('activate', () => {
  if (mainWindow === null) {
    createWindow()
  }
})

app.on('will-quit', () => {
  globalShortcut.unregisterAll()
})

export function getMainWindow() {
  return mainWindow
}
