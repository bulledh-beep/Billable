import { useState, useEffect, useCallback, useRef } from 'react'
import type { TimeEntry } from '@shared/types'

export function useTimer() {
  const [activeEntry, setActiveEntry] = useState<TimeEntry | null>(null)
  const [elapsed, setElapsed] = useState('')
  const intervalRef = useRef<NodeJS.Timeout | null>(null)

  const updateElapsed = useCallback(() => {
    if (!activeEntry?.start_time) return
    const start = new Date(activeEntry.start_time).getTime()
    const diff = Math.max(0, Date.now() - start)
    const totalSeconds = Math.floor(diff / 1000)
    const h = Math.floor(totalSeconds / 3600)
    const m = Math.floor((totalSeconds % 3600) / 60)
    const s = totalSeconds % 60
    setElapsed(`${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`)
  }, [activeEntry])

  useEffect(() => {
    checkActive()

    const unsub = window.api.on('toggle-timer', () => {
      if (activeEntry) {
        stopTimer()
      }
    })

    // Listen for state changes from main process (tray actions, global shortcuts)
    const unsubStateChange = window.api.on('timer:state-changed', () => {
      checkActive()
    })

    return () => { unsub?.(); unsubStateChange?.() }
  }, [])

  useEffect(() => {
    if (activeEntry) {
      updateElapsed()
      intervalRef.current = setInterval(updateElapsed, 1000)
    } else {
      setElapsed('')
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [activeEntry, updateElapsed])

  const checkActive = async () => {
    const entry = await window.api.time.active()
    setActiveEntry(entry)
  }

  const startTimer = async (projectId: number, description: string = '') => {
    // Stop any existing timer first
    if (activeEntry) {
      await window.api.time.stop(activeEntry.id)
    }
    const entry = await window.api.time.start(projectId, description)
    setActiveEntry(entry)
    return entry
  }

  const stopTimer = async () => {
    if (!activeEntry) return null
    const entry = await window.api.time.stop(activeEntry.id)
    setActiveEntry(null)
    return entry
  }

  return {
    activeEntry,
    elapsed,
    isRunning: !!activeEntry,
    startTimer,
    stopTimer,
    checkActive,
  }
}
