import { app, shell, session, BrowserWindow, type WebContents } from 'electron'
import { getSettings } from './database'
import { requestConnect } from './phone-sync'

// Content HQ runs inside Billable in a <webview>. It is a separate website, so
// it gets no preload, no Node, a sandbox, and its own saved sign-in, and it
// may only move between Content HQ and the services it signs in with.

export const CONTENT_HQ_DEFAULT_URL = 'https://content-hq-live.vercel.app'
export const CONTENT_HQ_PARTITION = 'persist:contenthq'

function contentHqOrigin(): URL | null {
  try {
    const configured = String(getSettings()?.content_hq_url || '').trim()
    return new URL(configured || CONTENT_HQ_DEFAULT_URL)
  } catch {
    return null
  }
}

/**
 * Google's sign-in hops between its own domains to set cookies: the country
 * site (accounts.google.ca), accounts.youtube.com and the consent page. All of
 * them have to stay inside the view, or the half-finished sign-in lands in the
 * default browser and Google answers "400. That's an error."
 */
function isGoogleSignInHost(host: string): boolean {
  if (host === 'accounts.youtube.com') return true
  return /^(accounts|consent)\.google\.(com|[a-z]{2}|co\.[a-z]{2}|com\.[a-z]{2})$/.test(host)
}

/** Content HQ itself, its Supabase backend, and Google sign-in. */
export function isAllowedContentUrl(raw: string): boolean {
  let url: URL
  try { url = new URL(raw) } catch { return false }
  const home = contentHqOrigin()
  const isLocalDev = url.protocol === 'http:' && (url.hostname === 'localhost' || url.hostname === '127.0.0.1')
  if (url.protocol !== 'https:' && !isLocalDev) return false
  if (home && url.host === home.host) return true
  if (url.hostname.endsWith('.supabase.co')) return true
  if (isGoogleSignInHost(url.hostname)) return true
  return false
}

/**
 * A cookie Content HQ can read to know it's open inside Billable, so it can
 * show its switch back to Billable.
 */
async function markEmbedded() {
  const home = contentHqOrigin()
  if (!home) return
  const oneYear = Math.floor(Date.now() / 1000) + 365 * 24 * 3600
  try {
    await session.fromPartition(CONTENT_HQ_PARTITION).cookies.set({
      url: home.origin,
      name: 'billable_shell',
      value: '1',
      secure: home.protocol === 'https:',
      sameSite: 'lax',
      expirationDate: oneYear,
    })
  } catch {
    // Not fatal: Content HQ just won't show its switch
  }
}

function openOutside(url: string) {
  if (/^https?:\/\//i.test(url)) shell.openExternal(url)
}

/**
 * billable:// links, from Content HQ inside the window or from a browser.
 *   billable://switch?to=billable   show Billable
 *   billable://switch?to=content    show Content HQ
 *   billable://connect?token=…      connect phone sync (only from Content HQ
 *                                   inside Billable's window; see below)
 * Anything else just brings Billable forward.
 */
export function handleBillableLink(raw: string) {
  const win = BrowserWindow.getAllWindows()[0]
  if (!win) return
  let route = ''
  try {
    const url = new URL(raw)
    const target = url.searchParams.get('to')
    if (url.hostname === 'switch') route = target === 'content' ? '/content' : 'last-billable'
  } catch {
    // Unreadable link: just show the app
  }
  if (win.isMinimized()) win.restore()
  win.show()
  win.focus()
  if (route) win.webContents.send('suite:switch', route === '/content' ? 'content' : 'billable')
}

const isBillableLink = (url: string) => /^billable:/i.test(url)

/**
 * A connect link is honoured only when it comes from Content HQ's own page in
 * Billable's window. From anywhere else (a browser, another site) it could
 * hand Billable someone else's token and send your data to their account.
 */
function handleLinkFromView(contents: WebContents, raw: string) {
  let url: URL
  try { url = new URL(raw) } catch { return }
  if (url.hostname !== 'connect') return handleBillableLink(raw)
  const home = contentHqOrigin()
  let page: URL
  try { page = new URL(contents.getURL()) } catch { return }
  if (!home || page.origin !== home.origin) return
  const token = url.searchParams.get('token') || ''
  requestConnect(token, page.origin)
}

/** Apply the rules to the main window and to every Content HQ web view. */
export function secureContentHq(mainWindow: BrowserWindow) {
  markEmbedded()
  mainWindow.webContents.on('will-attach-webview', (event, webPreferences, params) => {
    delete (webPreferences as { preload?: string }).preload
    webPreferences.nodeIntegration = false
    webPreferences.contextIsolation = true
    webPreferences.sandbox = true
    if (params.partition !== CONTENT_HQ_PARTITION || !isAllowedContentUrl(params.src)) event.preventDefault()
  })
}

app.on('web-contents-created', (_event, contents: WebContents) => {
  if (contents.getType() !== 'webview') return
  // New windows and links elsewhere open in the default browser
  contents.setWindowOpenHandler(({ url }) => {
    if (isBillableLink(url)) handleLinkFromView(contents, url)
    else openOutside(url)
    return { action: 'deny' }
  })
  const guard = (event: { preventDefault: () => void }, url: string) => {
    if (isAllowedContentUrl(url)) return
    event.preventDefault()
    if (isBillableLink(url)) handleLinkFromView(contents, url)
    else openOutside(url)
  }
  contents.on('will-navigate', guard)
  contents.on('will-redirect', guard)
})

export async function signOutOfContentHq() {
  await session.fromPartition(CONTENT_HQ_PARTITION).clearStorageData()
  await markEmbedded()
}

export function openContentUrlInBrowser(url: string) {
  if (isAllowedContentUrl(url)) openOutside(url)
}

// A browser opening billable:// launches or focuses Billable. Links that
// arrive before the window exists wait until it's ready.
let pendingLink: string | null = null
app.on('open-url', (event, url) => {
  event.preventDefault()
  if (BrowserWindow.getAllWindows().length === 0 || !app.isReady()) pendingLink = url
  else handleBillableLink(url)
})

export function registerBillableProtocol(mainWindow: BrowserWindow) {
  if (app.isPackaged) app.setAsDefaultProtocolClient('billable')
  mainWindow.webContents.once('did-finish-load', () => {
    if (pendingLink) { handleBillableLink(pendingLink); pendingLink = null }
  })
}
