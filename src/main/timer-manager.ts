import { BrowserWindow } from 'electron'
import * as db from './database'

type TrayCallbacks = {
  updateIcon: (state: 'idle' | 'active' | 'active-pulse') => void
  updateTitle: (text: string) => void
  rebuildMenu: () => void
}

type MenuCallbacks = {
  updateTimerMenu: () => void
}

export class TimerManager {
  private activeEntry: any = null
  private pausedEntry: any = null
  private tickInterval: NodeJS.Timeout | null = null
  private iconToggle = false
  private trayCallbacks: TrayCallbacks | null = null
  private menuCallbacks: MenuCallbacks | null = null

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

  setMenuCallbacks(callbacks: MenuCallbacks) {
    this.menuCallbacks = callbacks
  }

  syncFromDatabase() {
    this.stopTicking()
    this.activeEntry = db.getActiveTimer()
    this.pausedEntry = db.getPausedTimer()

    if (this.activeEntry && this.trayCallbacks) {
      this.startTicking()
    } else if (this.trayCallbacks) {
      // No active timer — make sure the tray menu reflects whatever DB is open now
      // (important after a profile switch that swapped the DB beneath us)
      this.trayCallbacks.updateIcon('idle')
      this.trayCallbacks.updateTitle('')
      this.trayCallbacks.rebuildMenu()
    }

    this.menuCallbacks?.updateTimerMenu()
  }

  start(projectId: number, description?: string) {
    // Stop any running timer first
    if (this.activeEntry) {
      this.stopInternal()
    }
    // Starting another project while paused finishes the paused entry before
    // creating the new one. The explicit Resume action keeps the paused entry.
    if (this.pausedEntry) {
      this.stopPausedInternal()
    }

    const entry = db.startTimer(projectId, description)
    this.activeEntry = entry
    this.pausedEntry = null
    this.startTicking()
    this.broadcastStateChange()
    this.menuCallbacks?.updateTimerMenu()
    return entry
  }

  stop() {
    if (this.activeEntry) {
      const entry = this.stopInternal()
      this.broadcastStateChange()
      this.menuCallbacks?.updateTimerMenu()
      return entry
    }

    if (this.pausedEntry) {
      const entry = this.stopPausedInternal()
      this.broadcastStateChange()
      this.menuCallbacks?.updateTimerMenu()
      return entry
    }

    return null
  }

  pause() {
    if (!this.activeEntry) return null

    const entry = db.pauseTimer(this.activeEntry.id)
    this.activeEntry = null
    this.pausedEntry = entry
    this.stopTicking()

    if (this.trayCallbacks) {
      this.trayCallbacks.updateIcon('idle')
      this.trayCallbacks.updateTitle('')
      this.trayCallbacks.rebuildMenu()
    }

    this.broadcastStateChange()
    this.menuCallbacks?.updateTimerMenu()
    return entry
  }

  resume() {
    if (!this.pausedEntry) return null

    const entry = db.resumeTimer(this.pausedEntry.id)
    this.pausedEntry = null
    this.activeEntry = entry
    this.startTicking()
    this.broadcastStateChange()
    this.menuCallbacks?.updateTimerMenu()
    return entry
  }

  togglePause() {
    return this.activeEntry ? this.pause() : this.pausedEntry ? this.resume() : null
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

  private stopPausedInternal() {
    if (!this.pausedEntry) return null
    const entry = db.stopTimer(this.pausedEntry.id)
    this.pausedEntry = null

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

  getPaused() {
    return this.pausedEntry
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

    const elapsed = this.formatElapsed(
      this.activeEntry.start_time,
      this.activeEntry.duration_minutes,
      this.activeEntry.active_since,
    )
    this.trayCallbacks.updateTitle(elapsed)

    // Toggle icon for pulse animation
    this.iconToggle = !this.iconToggle
    this.trayCallbacks.updateIcon(this.iconToggle ? 'active' : 'active-pulse')

    // Rebuild menu to update elapsed time label
    this.trayCallbacks.rebuildMenu()
  }

  private formatElapsed(startTime: string, accumulatedMinutes = 0, activeSince?: string | null): string {
    const accumulatedSeconds = Math.max(0, Number(accumulatedMinutes) || 0) * 60
    const since = activeSince === null ? null : new Date(activeSince || startTime).getTime()
    const runningSeconds = since === null ? 0 : Math.max(0, Date.now() - since) / 1000
    const totalSeconds = Math.floor(accumulatedSeconds + runningSeconds)
    const h = Math.floor(totalSeconds / 3600)
    const m = Math.floor((totalSeconds % 3600) / 60)
    const s = totalSeconds % 60
    return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
  }

  formatElapsedReadable(startTime: string, accumulatedMinutes = 0, activeSince?: string | null): string {
    const accumulatedSeconds = Math.max(0, Number(accumulatedMinutes) || 0) * 60
    const since = activeSince === null ? null : new Date(activeSince || startTime).getTime()
    const runningSeconds = since === null ? 0 : Math.max(0, Date.now() - since) / 1000
    const totalSeconds = Math.floor(accumulatedSeconds + runningSeconds)
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
