import { useEffect, useState, useCallback } from 'react'

export type ThemePreference = 'dark' | 'light' | 'auto'
export type ResolvedTheme = 'dark' | 'light'

const STORAGE_KEY = 'billable.theme'
const CHANGE_EVENT = 'billable:theme-change'

function readStoredPreference(): ThemePreference {
  try {
    const v = localStorage.getItem(STORAGE_KEY)
    if (v === 'dark' || v === 'light' || v === 'auto') return v
  } catch {
    // localStorage unavailable
  }
  return 'auto'
}

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' &&
    !!window.matchMedia &&
    window.matchMedia('(prefers-color-scheme: dark)').matches
}

function resolve(pref: ThemePreference): ResolvedTheme {
  if (pref === 'dark') return 'dark'
  if (pref === 'light') return 'light'
  return systemPrefersDark() ? 'dark' : 'light'
}

function applyToDocument(theme: ResolvedTheme) {
  const root = document.documentElement
  root.classList.toggle('dark', theme === 'dark')
  root.classList.toggle('light', theme === 'light')
}

/** Tell the main process so the sidebar material and native menus match. */
function syncNative(pref: ThemePreference) {
  try {
    window.api?.appearance?.set(pref)?.catch?.(() => {})
  } catch {
    // Not running inside Electron
  }
}

/**
 * Theme controller for the Settings toggle. The preference lives in
 * localStorage so it survives profile switches and restarts.
 */
export function useTheme() {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => readStoredPreference())
  const [resolved, setResolved] = useState<ResolvedTheme>(() => resolve(readStoredPreference()))

  useEffect(() => {
    const onChange = () => {
      const pref = readStoredPreference()
      setPreferenceState(pref)
      setResolved(resolve(pref))
    }
    window.addEventListener(CHANGE_EVENT, onChange)
    return () => window.removeEventListener(CHANGE_EVENT, onChange)
  }, [])

  const setPreference = useCallback((pref: ThemePreference) => {
    try {
      localStorage.setItem(STORAGE_KEY, pref)
    } catch {
      // ignore
    }
    // Native first: in auto mode the page's color-scheme follows it
    syncNative(pref)
    applyToDocument(resolve(pref))
    setPreferenceState(pref)
    setResolved(resolve(pref))
    window.dispatchEvent(new Event(CHANGE_EVENT))
  }, [])

  return { preference, resolved, setPreference }
}

/**
 * Apply the stored theme before React mounts, and keep following the Mac's
 * appearance while the preference is Auto.
 */
export function applyInitialTheme() {
  const pref = readStoredPreference()
  syncNative(pref)
  applyToDocument(resolve(pref))
  window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (readStoredPreference() === 'auto') {
      applyToDocument(resolve('auto'))
      window.dispatchEvent(new Event(CHANGE_EVENT))
    }
  })
}
