import { useCallback, useEffect, useState } from 'react'
import type { UpdateStatus, UpdateProgress } from '@shared/types'
import toast from 'react-hot-toast'

const STORAGE_DISMISSED = 'billable.update.dismissed'
const RECHECK_INTERVAL_MS = 6 * 60 * 60 * 1000 // 6 hours

export type DownloadState = 'idle' | 'downloading' | 'installing' | 'done' | 'error'

export function useUpdater() {
  const [status, setStatus] = useState<UpdateStatus | null>(null)
  const [checking, setChecking] = useState(false)
  const [downloadState, setDownloadState] = useState<DownloadState>('idle')
  const [progress, setProgress] = useState<UpdateProgress | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [canInstall, setCanInstall] = useState(false)
  const [dismissed, setDismissedState] = useState<string | null>(() => {
    try {
      return localStorage.getItem(STORAGE_DISMISSED)
    } catch {
      return null
    }
  })

  // On mount: load cached status, ask main if silent install is supported,
  // then trigger a fresh check (server-side cached for 5min)
  useEffect(() => {
    let cancelled = false
    const init = async () => {
      window.api.updater.canInstall().then(v => { if (!cancelled) setCanInstall(!!v) })
      const cached = await window.api.updater.cached()
      if (!cancelled && cached) setStatus(cached)
      try {
        setChecking(true)
        const fresh = await window.api.updater.check(false)
        if (!cancelled) setStatus(fresh)
      } catch {
        // Silent — network errors on launch shouldn't surface
      } finally {
        if (!cancelled) setChecking(false)
      }
    }
    init()
    return () => { cancelled = true }
  }, [])

  // Re-check every 6 hours so a long-running session picks up new releases
  useEffect(() => {
    const interval = setInterval(() => {
      window.api.updater.check(false).then(fresh => setStatus(fresh)).catch(() => {})
    }, RECHECK_INTERVAL_MS)
    return () => clearInterval(interval)
  }, [])

  // Listen for download progress
  useEffect(() => {
    const unsub = window.api.on('updater:progress', (p: UpdateProgress) => {
      setProgress(p)
    })
    return () => { unsub?.() }
  }, [])

  const checkNow = useCallback(async () => {
    setChecking(true)
    setError(null)
    try {
      const fresh = await window.api.updater.check(true)
      setStatus(fresh)
      if (!fresh.update_available) {
        toast.success(`You're on the latest version (${fresh.current_version})`)
      }
      return fresh
    } catch (err: any) {
      const msg = err?.message || String(err)
      setError(msg)
      toast.error(`Update check failed: ${msg}`)
      throw err
    } finally {
      setChecking(false)
    }
  }, [])

  const download = useCallback(async () => {
    if (!status?.download_url) {
      toast.error('No download available for this release')
      return
    }
    setDownloadState('downloading')
    setProgress(null)
    setError(null)
    try {
      await window.api.updater.download(status.download_url)
      setDownloadState('done')
      toast.success('Update downloaded — drag Billable into Applications to install')
    } catch (err: any) {
      const msg = err?.message || String(err)
      setError(msg)
      setDownloadState('error')
      toast.error(`Download failed: ${msg}`)
    }
  }, [status])

  /**
   * Silent install: downloads + replaces + relaunches the app via a detached
   * helper. Falls back to opening the DMG if the app's bundle isn't writable
   * (e.g. running from a read-only volume).
   */
  const install = useCallback(async () => {
    if (!status?.download_url) {
      toast.error('No download available for this release')
      return
    }
    if (!canInstall) {
      // Dev build or unknown bundle path — fall back to download
      return download()
    }
    setDownloadState('downloading')
    setProgress(null)
    setError(null)
    try {
      await window.api.updater.install(status.download_url)
      setDownloadState('installing')
      toast.success('Update installed — Billable will quit and relaunch')
      // App will quit shortly via main process
    } catch (err: any) {
      const msg = err?.message || String(err)
      setError(msg)
      setDownloadState('error')
      toast.error(`Install failed: ${msg}`)
    }
  }, [status, canInstall, download])

  // Listen for the "Check for Updates…" app menu item — registered AFTER
  // checkNow is defined so the closure picks it up on first render.
  useEffect(() => {
    const unsub = window.api.on('menu:check-updates', () => {
      checkNow().catch(() => {})
    })
    return () => { unsub?.() }
  }, [checkNow])

  const dismiss = useCallback(() => {
    if (!status?.latest_version) return
    try {
      localStorage.setItem(STORAGE_DISMISSED, status.latest_version)
    } catch {
      // ignore
    }
    setDismissedState(status.latest_version)
  }, [status])

  // Banner is shown when an update is available AND the user hasn't dismissed
  // THIS specific version
  const showBanner = !!(
    status?.update_available &&
    status.latest_version &&
    dismissed !== status.latest_version
  )

  return {
    status,
    checking,
    downloadState,
    progress,
    error,
    showBanner,
    canInstall,
    checkNow,
    download,
    install,
    dismiss,
  }
}
