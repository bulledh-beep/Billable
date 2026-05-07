import { useEffect, useState, useCallback } from 'react'

export type ThemePreference = 'dark' | 'light' | 'auto'
export type ResolvedTheme = 'dark' | 'light'

const STORAGE_KEY = 'billable.theme'

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

/**
 * Theme controller. Stores the user's preference in localStorage so it survives
 * profile switches and app restarts. Returns the active preference, the resolved
 * theme, and a setter.
 */
export function useTheme() {
  const [preference, setPreferenceState] = useState<ThemePreference>(() => readStoredPreference())
  const [resolved, setResolved] = useState<ResolvedTheme>(() => resolve(readStoredPreference()))

  // Apply on mount + whenever preference changes
  useEffect(() => {
    const r = resolve(preference)
    setResolved(r)
    applyToDocument(r)
  }, [preference])

  // Listen for system theme changes when in 'auto'
  useEffect(() => {
    if (preference !== 'auto') return
    const mql = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => {
      const r: ResolvedTheme = mql.matches ? 'dark' : 'light'
      setResolved(r)
      applyToDocument(r)
    }
    mql.addEventListener('change', onChange)
    return () => mql.removeEventListener('change', onChange)
  }, [preference])

  const setPreference = useCallback((pref: ThemePreference) => {
    try {
      localStorage.setItem(STORAGE_KEY, pref)
    } catch {
      // ignore
    }
    setPreferenceState(pref)
  }, [])

  return { preference, resolved, setPreference }
}

/**
 * Apply the stored theme as early as possible (called from App entry).
 * Avoids a flash of wrong theme before React mounts.
 */
export function applyInitialTheme() {
  applyToDocument(resolve(readStoredPreference()))
}
