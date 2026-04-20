import { BrowserWindow } from 'electron'
import * as db from './database'

type TrayCallbacks = {
  updateIcon: (state: 'idle' | 'active' | 'active-pulse') => void
  updateTitle: (text: string) => void
  rebuildMenu: () => void
}

export class TimerManager {
  private activeEntry: any = null
  private tickInterval: NodeJS.Timeout | null = null
  private iconToggle = false
  private trayCallbacks: TrayCallbacks | null = null

  constructor() {
    this.syncFromDatabase()
  }

  setTrayCallbacks(callbacks: TrayCallbacks) {
    this.trayCallbacks = callbacks
    // If a timer was already running (picked up from DB), start ticking
    if (this.activeEntry) {
      this.startTicking()
    }
  }

  syncFromDatabase() {
    this.activeEntry = db.getActiveTimer()
    if (this.activeEntry && this.trayCallbacks) {
      this.startTicking()
    }
  }

  start(projectId: number, description?: string) {
    // Stop any running timer first
    if (this.activeEntry) {
      this.stopInternal()
    }

    const entry = db.startTimer(projectId, description)
    this.activeEntry = entry
    this.startTicking()
    this.broadcastStateChange()
    return entry
  }

  stop() {
    if (!this.activeEntry) return null
    const entry = this.stopInternal()
    this.broadcastStateChange()
    return entry
  }

  private stopInternal() {
    if (!this.activeEntry) return null
    const entry = db.stopTimer(this.activeEntry.id)
    this.activeEntry = null
    this.stopTicking()

    if (this.trayCallbacks) {
      this.trayCallbacks.updateIcon('idle')
      this.trayCallbacks.updateTitle('')
      this.trayCallbacks.rebuildMenu()
    }

    return entry
  }

  getActive() {
    return this.activeEntry
  }

  private startTicking() {
    this.stopTicking()
    this.iconToggle = false
    this.onTick() // immediate first tick
    this.tickInterval = setInterval(() => this.onTick(), 1000)
  }

  private stopTicking() {
    if (this.tickInterval) {
      clearInterval(this.tickInterval)
      this.tickInterval = null
    }
  }

  private onTick() {
    if (!this.activeEntry || !this.trayCallbacks) return

    const elapsed = this.formatElapsed(this.activeEntry.start_time)
    this.trayCallbacks.updateTitle(elapsed)

    // Toggle icon for pulse animation
    this.iconToggle = !this.iconToggle
    this.trayCallbacks.updateIcon(this.iconToggle ? 'active' : 'active-pulse')

    // Rebuild menu to update elapsed time label
    this.trayCallbacks.rebuildMenu()
  }

  private formatElapsed(startTime: string): string {
    const start = new Date(startTime).getTime()
    const diff = Math.max(0, Date.now() - start)
    const totalSeconds = Math.floor(diff / 1000)
    const h = Math.floor(totalSeconds / 3600)
    const m = Math.floor((totalSeconds % 3600) / 60)
    const s = totalSeconds % 60
    return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
  }

  formatElapsedReadable(startTime: string): string {
    const start = new Date(startTime).getTime()
    const diff = Math.max(0, Date.now() - start)
    const totalSeconds = Math.floor(diff / 1000)
    const h = Math.floor(totalSeconds / 3600)
    const m = Math.floor((totalSeconds % 3600) / 60)
    const s = totalSeconds % 60
    if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m ${String(s).padStart(2, '0')}s`
    return `${m}m ${String(s).padStart(2, '0')}s`
  }

  private broadcastStateChange() {
    for (const win of BrowserWindow.getAllWindows()) {
      win.webContents.send('timer:state-changed')
    }
  }
}
