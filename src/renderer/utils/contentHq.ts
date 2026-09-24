import type { WebviewTag } from 'electron'

export const CONTENT_HQ_DEFAULT_URL = 'https://content-hq-live.vercel.app'

/** The address from Settings, or the live workspace. */
export function contentHqUrl(settings: any): string {
  const raw = String(settings?.content_hq_url || '').trim()
  try {
    const url = new URL(raw || CONTENT_HQ_DEFAULT_URL)
    return url.protocol === 'https:' || url.hostname === 'localhost' || url.hostname === '127.0.0.1' ? url.toString() : CONTENT_HQ_DEFAULT_URL
  } catch {
    return CONTENT_HQ_DEFAULT_URL
  }
}

// The one embedded Content HQ view, so the toolbar can drive it
let current: WebviewTag | null = null
export function registerContentHqView(view: WebviewTag | null) { current = view }
export function contentHqView() { return current }

export const CONTENT_HQ_NAV = 'billable:contenthq-nav'
