import { Tray, Menu, nativeImage, BrowserWindow, app } from 'electron'
import path from 'path'
import * as db from './database'
import { TimerManager } from './timer-manager'

let tray: Tray | null = null
let timerManager: TimerManager | null = null
let mainWindow: BrowserWindow | null = null
let idleIcon: Electron.NativeImage
let activeIcon: Electron.NativeImage

function getResourcePath() {
  if (app.isPackaged) {
    return process.resourcesPath
  }
  return path.join(__dirname, '../../resources')
}

function loadIcons() {
  const resPath = getResourcePath()
  idleIcon = nativeImage.createFromPath(path.join(resPath, 'trayIconTemplate.png'))
  idleIcon.setTemplateImage(true)
  activeIcon = nativeImage.createFromPath(path.join(resPath, 'trayIconActiveTemplate.png'))
  activeIcon.setTemplateImage(true)
}

export function createTray(window: BrowserWindow, manager: TimerManager) {
  mainWindow = window
  timerManager = manager

  loadIcons()

  tray = new Tray(idleIcon)
  tray.setToolTip('Billable')
  tray.setTitle('')

  rebuildTrayMenu()

  // Wire up tray callbacks so TimerManager can update the tray
  manager.setTrayCallbacks({
    updateIcon,
    updateTitle,
    rebuildMenu: rebuildTrayMenu,
  })
}

export function updateIcon(state: 'idle' | 'active' | 'active-pulse') {
  if (!tray) return
  if (state === 'idle') {
    tray.setImage(idleIcon)
  } else {
    // 'active' and 'active-pulse' alternate between the two icons
    tray.setImage(state === 'active' ? activeIcon : idleIcon)
  }
}

export function updateTitle(text: string) {
  if (!tray) return
  tray.setTitle(text)
}

export function rebuildTrayMenu() {
  if (!tray || !timerManager) return

  const active = timerManager.getActive()
  const projects = db.listProjects() as any[]
  const activeProjects = projects.filter((p: any) => p.status === 'active')
  const todayHours = db.getTodayHours()

  let template: Electron.MenuItemConstructorOptions[]

  if (active) {
    // Timer is running
    const elapsed = timerManager.formatElapsedReadable(active.start_time)
    const projectLabel = active.project_name || 'Unknown Project'
    const clientLabel = active.client_name ? ` — ${active.client_name}` : ''

    template = [
      {
        label: 'Stop Timer',
        click: () => timerManager!.stop(),
      },
      { type: 'separator' },
      {
        label: `\u25CF  ${projectLabel}${clientLabel}`,
        enabled: false,
      },
      {
        label: `     ${elapsed}`,
        enabled: false,
      },
      { type: 'separator' },
      {
        label: 'Switch Project',
        submenu: activeProjects
          .filter((p: any) => p.id !== active.project_id)
          .map((p: any) => ({
            label: `${p.name}${p.client_name ? ` (${p.client_name})` : ''}`,
            click: () => timerManager!.start(p.id),
          })),
      },
      { type: 'separator' },
      {
        label: 'Open Billable',
        click: () => showMainWindow(),
      },
      {
        label: `Today: ${todayHours.toFixed(1)} hrs tracked`,
        enabled: false,
      },
      { type: 'separator' },
      {
        label: 'Quit Billable',
        click: () => app.quit(),
      },
    ]
  } else {
    // No timer running
    const recentProjects = db.getRecentProjects(5)

    template = [
      {
        label: 'Start Timer',
        submenu: activeProjects.map((p: any) => ({
          label: `${p.name}${p.client_name ? ` (${p.client_name})` : ''}`,
          click: () => timerManager!.start(p.id),
        })),
      },
      { type: 'separator' },
    ]

    // Add recent projects for quick access
    if (recentProjects.length > 0) {
      for (const p of recentProjects) {
        template.push({
          label: `${p.name}`,
          click: () => timerManager!.start(p.id),
        })
      }
      template.push({ type: 'separator' })
    }

    template.push(
      {
        label: 'Open Billable',
        click: () => showMainWindow(),
      },
      {
        label: `Today: ${todayHours.toFixed(1)} hrs tracked`,
        enabled: false,
      },
      { type: 'separator' },
      {
        label: 'Quit Billable',
        click: () => app.quit(),
      },
    )
  }

  tray.setContextMenu(Menu.buildFromTemplate(template))
}

function showMainWindow() {
  if (mainWindow) {
    mainWindow.show()
    mainWindow.focus()
  }
}
