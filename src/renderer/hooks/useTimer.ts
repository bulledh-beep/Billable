import { useState, useEffect, useCallback, useRef } from 'react'
import type { TimeEntry, TimerState } from '@shared/types'

function formatElapsed(entry: TimeEntry | null) {
  if (!entry) return ''

  const accumulatedSeconds = Math.max(0, Number(entry.duration_minutes) || 0) * 60
  const activeSince = new Date(entry.active_since || entry.start_time).getTime()
  const runningSeconds = entry.paused_at
    ? 0
    : Math.max(0, Date.now() - activeSince) / 1000
  const totalSeconds = Math.floor(accumulatedSeconds + runningSeconds)
  const h = Math.floor(totalSeconds / 3600)
  const m = Math.floor((totalSeconds % 3600) / 60)
  const s = totalSeconds % 60

  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

export function useTimer() {
  // activeEntry contains either the currently running or currently paused
  // timer. Keeping one current entry makes the resume action available from
  // every screen without duplicating timer lookup logic.
  const [activeEntry, setActiveEntry] = useState<TimeEntry | null>(null)
  const [elapsed, setElapsed] = useState('')
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const updateElapsed = useCallback(() => {
    setElapsed(formatElapsed(activeEntry))
  }, [activeEntry])

  const checkActive = async () => {
    const state = await window.api.time.state() as TimerState
    setActiveEntry(state.active ?? state.paused ?? null)
  }

  const pauseTimer = async () => {
    const entry = await window.api.time.pause()
    setActiveEntry(entry)
    return entry
  }

  const resumeTimer = async () => {
    const entry = await window.api.time.resume()
    setActiveEntry(entry)
    return entry
  }

  useEffect(() => {
    checkActive()

    const unsub = window.api.on('toggle-timer', () => {
      if (activeEntry?.paused_at) {
        resumeTimer()
      } else if (activeEntry) {
        pauseTimer()
      }
    })

    // Listen for state changes from main process (tray actions, global shortcuts,
    // native menu actions, and profile switches).
    const unsubStateChange = window.api.on('timer:state-changed', () => {
      checkActive()
    })

    return () => { unsub?.(); unsubStateChange?.() }
  }, [activeEntry])

  useEffect(() => {
    if (activeEntry) {
      updateElapsed()
      if (!activeEntry.paused_at) {
        intervalRef.current = setInterval(updateElapsed, 1000)
      }
    } else {
      setElapsed('')
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current)
        intervalRef.current = null
      }
    }
  }, [activeEntry, updateElapsed])

  const startTimer = async (projectId: number, description: string = '') => {
    // Stop any existing running timer first. A paused timer is finalized by
    // the main process if the user starts a different project.
    if (activeEntry && !activeEntry.paused_at) {
      await window.api.time.stop()
    }
    const entry = await window.api.time.start(projectId, description)
    setActiveEntry(entry)
    return entry
  }

  const stopTimer = async () => {
    if (!activeEntry) return null
    const entry = await window.api.time.stop()
    setActiveEntry(null)
    return entry
  }

  return {
    activeEntry,
    elapsed,
    isRunning: !!activeEntry && !activeEntry.paused_at,
    isPaused: !!activeEntry?.paused_at,
    startTimer,
    stopTimer,
    pauseTimer,
    resumeTimer,
    checkActive,
  }
}
