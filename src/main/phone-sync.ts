import { app, BrowserWindow, net, powerMonitor, safeStorage } from 'electron'
import { execFile } from 'child_process'
import crypto from 'crypto'
import fs from 'fs'
import os from 'os'
import path from 'path'
import * as db from './database'
import { getActiveProfile, getActiveProfileId, getProfileDir } from './profiles'
import { buildSnapshot } from './phone-snapshot'
import { applyAction, type ActionResult, type IncomingAction } from './phone-actions'
import type { TimerManager } from './timer-manager'

// Phone sync with Content HQ. The Mac stays the source of truth:
//   - it sends Content HQ a snapshot of Billable whenever something changes,
//   - it picks up changes made on the phone (the "actions"), applies them,
//     and acknowledges each one in the same request as a snapshot that
//     already includes its effect.
// It checks in every 20 seconds while Billable is running, right away when
// the Mac wakes or unlocks, and soon after anything changes on the Mac.
//
// The connection belongs to the profile that was open when it was made and
// only ever talks to the Content HQ address that issued it. Its token is
// encrypted with the macOS Keychain and never logged or shown.

const CONFIG_FILE = 'phone-sync.json'
const TOKEN_PATTERN = /^bhq_[A-Za-z0-9_-]{40,64}$/
const MIN_GAP_MS = 5_200 // Content HQ allows one request every 5 seconds
const DEFAULT_INTERVAL_S = 20
const REQUEST_TIMEOUT_MS = 15_000
const CHANGE_POLL_MS = 3_000

export type PhoneSyncState =
  | 'off' | 'syncing' | 'ok' | 'offline' | 'unavailable' | 'revoked' | 'mismatch' | 'error'

export interface PhoneSyncStatus {
  connected: boolean
  state: PhoneSyncState
  host: string | null
  profileName: string | null
  connectedAt: string | null
  lastSyncAt: string | null
  message: string | null
}

interface StoredConfig {
  version: 1
  origin: string
  token: string // base64 of the Keychain-encrypted token
  profileId: string
  connectedAt: string
}

interface Connection {
  profileId: string
  origin: string
  token: string
  connectedAt: string
}

let timer: TimerManager
let conn: Connection | null = null
let loadedProfileId: string | null = null
let status: PhoneSyncStatus = {
  connected: false, state: 'off', host: null, profileName: null, connectedAt: null, lastSyncAt: null, message: null,
}

let lastSentHash: string | null = null
let needsSnapshot = true
let inFlight = false
let rerun = false
let nextTimer: NodeJS.Timeout | null = null
let nextDueAt = 0
let lastRequestAt = 0
let notBefore = 0
let failures = 0
let fastRetryUntil = 0
let lastChanges = -1
let deviceName: string | null = null

// ---------- Storage ----------

function configPath(profileId: string) {
  return path.join(getProfileDir(profileId), CONFIG_FILE)
}

function readConnection(profileId: string): Connection | null {
  try {
    const raw = JSON.parse(fs.readFileSync(configPath(profileId), 'utf-8')) as StoredConfig
    if (raw.version !== 1 || raw.profileId !== profileId || !raw.token || !raw.origin) return null
    if (!safeStorage.isEncryptionAvailable()) return null
    const token = safeStorage.decryptString(Buffer.from(raw.token, 'base64'))
    if (!TOKEN_PATTERN.test(token)) return null
    return { profileId, origin: raw.origin, token, connectedAt: raw.connectedAt }
  } catch {
    return null
  }
}

function writeConnection(c: Connection) {
  const stored: StoredConfig = {
    version: 1,
    origin: c.origin,
    token: safeStorage.encryptString(c.token).toString('base64'),
    profileId: c.profileId,
    connectedAt: c.connectedAt,
  }
  fs.writeFileSync(configPath(c.profileId), JSON.stringify(stored, null, 2), { mode: 0o600 })
}

function forgetConnection(profileId: string) {
  try { fs.unlinkSync(configPath(profileId)) } catch { /* already gone */ }
}

// ---------- Status ----------

function broadcast(channel: string, ...args: unknown[]) {
  for (const win of BrowserWindow.getAllWindows()) {
    if (!win.isDestroyed()) win.webContents.send(channel, ...args)
  }
}

function setStatus(patch: Partial<PhoneSyncStatus>) {
  status = { ...status, ...patch }
  broadcast('phone:status', status)
}

export function getPhoneSyncStatus(): PhoneSyncStatus {
  return status
}

/** Pick up the connection for whichever profile is open now. */
function loadForActiveProfile() {
  const profileId = getActiveProfileId()
  if (profileId === loadedProfileId) return
  loadedProfileId = profileId
  conn = readConnection(profileId)
  lastSentHash = null
  needsSnapshot = true
  failures = 0
  lastChanges = -1
  setStatus({
    connected: !!conn,
    state: conn ? 'syncing' : (status.state === 'revoked' ? 'revoked' : 'off'),
    host: conn ? new URL(conn.origin).host : null,
    profileName: getActiveProfile()?.name || null,
    connectedAt: conn?.connectedAt || null,
    lastSyncAt: null,
    message: conn ? null : status.state === 'revoked' ? status.message : null,
  })
}

// ---------- Scheduling ----------

function schedule(delayMs: number) {
  if (!conn) return
  const now = Date.now()
  const due = Math.max(now + delayMs, lastRequestAt + MIN_GAP_MS, notBefore)
  // Keep an earlier run if one is already set
  if (nextTimer && nextDueAt <= due) return
  if (nextTimer) clearTimeout(nextTimer)
  nextDueAt = due
  nextTimer = setTimeout(() => { nextTimer = null; nextDueAt = 0; void tick() }, due - now)
}

function syncSoon() { schedule(0) }

function backoffMs() {
  if (Date.now() < fastRetryUntil) return 5_000
  return Math.min(300_000, 20_000 * 2 ** Math.min(failures - 1, 4))
}

// ---------- Talking to Content HQ ----------

async function computerName(): Promise<string> {
  if (deviceName) return deviceName
  const fallback = os.hostname().replace(/\.local$/i, '')
  deviceName = await new Promise<string>(resolve => {
    execFile('/usr/sbin/scutil', ['--get', 'ComputerName'], { timeout: 2000 }, (err, out) => {
      resolve(!err && out.trim() ? out.trim() : fallback)
    })
  })
  deviceName = (deviceName || 'Mac').slice(0, 60)
  return deviceName
}

async function post(c: Connection, pathname: string, body: unknown) {
  lastRequestAt = Date.now()
  return net.fetch(new URL(pathname, c.origin).toString(), {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${c.token}`,
      'Content-Type': 'application/json',
      Accept: 'application/json',
    },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  })
}

function unackedRows(): Array<{ actionId: string; status: string; message?: string; createdBillableId?: string }> {
  const rows = db.getDatabase().prepare(`
    SELECT id, status, message, created_billable_id FROM phone_actions
    WHERE acked = 0 ORDER BY applied_at LIMIT 200
  `).all() as any[]
  return rows.map(r => {
    const ack: { actionId: string; status: string; message?: string; createdBillableId?: string } = { actionId: r.id, status: r.status }
    if (r.message) ack.message = String(r.message).slice(0, 200)
    if (r.created_billable_id) ack.createdBillableId = String(r.created_billable_id)
    return ack
  })
}

function hashSnapshot(snapshot: ReturnType<typeof buildSnapshot>) {
  const { generatedAt: _ignored, ...rest } = snapshot
  return crypto.createHash('sha256').update(JSON.stringify(rest)).digest('hex')
}

function totalChanges(): number {
  try {
    return (db.getDatabase().prepare('SELECT total_changes() AS n').get() as any).n as number
  } catch {
    return lastChanges
  }
}

async function tick() {
  if (inFlight) { rerun = true; return }
  loadForActiveProfile()
  const c = conn
  if (!c) return
  // Content HQ asked us to wait (429 Retry-After)
  if (Date.now() < notBefore) { schedule(notBefore - Date.now()); return }
  inFlight = true
  let followUp: number | null = DEFAULT_INTERVAL_S * 1000

  try {
    setStatus({ state: status.state === 'ok' ? 'ok' : 'syncing' })
    const acks = unackedRows()
    const snapshot = buildSnapshot(timer)
    const hash = hashSnapshot(snapshot)
    const includeSnapshot = needsSnapshot || hash !== lastSentHash || acks.length > 0
    const profile = getActiveProfile()

    const res = await post(c, '/api/billable/sync', {
      schemaVersion: 1,
      app: {
        version: app.getVersion(),
        profileId: c.profileId,
        profileName: (profile?.name || c.profileId).slice(0, 200),
        deviceName: await computerName(),
      },
      snapshot: includeSnapshot ? snapshot : null,
      acks,
    })
    // The profile may have been switched while the request was out
    if (conn !== c) return

    if (res.status === 401) {
      forgetConnection(c.profileId)
      conn = null
      setStatus({
        connected: false, state: 'revoked', host: null, connectedAt: null,
        message: 'This Mac was disconnected in Content HQ. Connect it again from Content HQ’s settings.',
      })
      followUp = null
      return
    }
    if (res.status === 409) {
      setStatus({ state: 'mismatch', message: 'Content HQ is linked to a different Billable profile. Disconnect and connect again from this profile.' })
      followUp = 10 * 60_000
      return
    }
    if (res.status === 404) {
      setStatus({ state: 'unavailable', message: 'Content HQ doesn’t have phone sync turned on yet.' })
      followUp = 5 * 60_000
      return
    }
    if (res.status === 429) {
      const wait = Math.min(300, Math.max(5, Number(res.headers.get('retry-after')) || 10))
      notBefore = Date.now() + wait * 1000
      followUp = wait * 1000
      return
    }
    if (!res.ok) {
      const detail = (await res.text().catch(() => '')).slice(0, 500)
      console.error(`[phone] sync refused (${res.status}):`, detail)
      failures++
      setStatus({ state: 'error', message: `Content HQ didn’t accept the sync (error ${res.status}). Billable will keep trying.` })
      followUp = Math.max(60_000, backoffMs())
      return
    }

    const json = await res.json().catch(() => null) as any
    if (!json || json.ok !== true || !Array.isArray(json.actions)) {
      failures++
      setStatus({ state: 'error', message: 'Content HQ sent back something Billable couldn’t read.' })
      followUp = Math.max(60_000, backoffMs())
      return
    }

    // Accepted: this snapshot and these acks are stored
    if (includeSnapshot) lastSentHash = hash
    if (acks.length) {
      const mark = db.getDatabase().prepare('UPDATE phone_actions SET acked = 1 WHERE id = ?')
      db.getDatabase().transaction(() => { for (const a of acks) mark.run(a.actionId) })()
    }
    needsSnapshot = !!json.needsSnapshot
    failures = 0
    setStatus({ state: 'ok', message: null, lastSyncAt: new Date().toISOString() })

    const applied = applyIncoming(json.actions)
    if (applied.length) {
      broadcast('phone:applied', applied)
      followUp = 0 // report back straight away, with the new snapshot
    } else if (needsSnapshot) {
      followUp = 0
    } else {
      const next = Number(json.nextSyncSeconds)
      followUp = (Number.isFinite(next) ? Math.min(300, Math.max(10, next)) : DEFAULT_INTERVAL_S) * 1000
    }
  } catch (err) {
    if (conn !== c) return
    failures++
    const offline = !net.isOnline()
    console.warn('[phone] sync failed:', (err as Error)?.message || err)
    setStatus({
      state: 'offline',
      message: offline ? 'Your Mac is offline. Billable will sync when it’s back.' : 'Couldn’t reach Content HQ. Billable will keep trying.',
    })
    followUp = backoffMs()
  } finally {
    inFlight = false
    lastChanges = totalChanges()
    if (conn) {
      if (rerun) { rerun = false; syncSoon() } else if (followUp !== null) schedule(followUp)
    }
  }
}

/** Apply new actions in order. Ones already applied are only acknowledged again. */
function applyIncoming(list: unknown[]): Array<Pick<ActionResult, 'status' | 'summary' | 'message'>> {
  const database = db.getDatabase()
  const find = database.prepare('SELECT id FROM phone_actions WHERE id = ?')
  const reack = database.prepare('UPDATE phone_actions SET acked = 0 WHERE id = ?')
  const insert = database.prepare(`
    INSERT OR IGNORE INTO phone_actions (id, kind, status, summary, message, created_billable_id, occurred_at, applied_at, acked)
    VALUES (@id, @kind, @status, @summary, @message, @created, @occurred, @applied, 0)
  `)
  const record = (a: IncomingAction, r: ActionResult) => insert.run({
    id: a.id,
    kind: a.kind.slice(0, 40),
    status: r.status,
    summary: r.summary.slice(0, 200),
    message: r.message ? r.message.slice(0, 200) : null,
    created: r.createdBillableId || null,
    occurred: typeof a.occurredAt === 'string' ? a.occurredAt.slice(0, 40) : null,
    applied: new Date().toISOString(),
  })

  class RolledBack extends Error { constructor(public result: ActionResult) { super('rolled back') } }
  const applyOne = database.transaction((a: IncomingAction) => {
    const r = applyAction(a, timer)
    // Undo anything a rejected change half-did, then record the rejection
    if (r.status === 'rejected') throw new RolledBack(r)
    record(a, r)
    return r
  })

  const results: Array<Pick<ActionResult, 'status' | 'summary' | 'message'>> = []
  for (const raw of list.slice(0, 100)) {
    const a = raw as IncomingAction
    if (!a || typeof a.id !== 'string' || !/^[0-9a-f-]{36}$/i.test(a.id) || typeof a.kind !== 'string') continue
    a.id = a.id.toLowerCase()
    if (find.get(a.id)) { reack.run(a.id); continue }
    let r: ActionResult
    try {
      r = applyOne(a)
    } catch (err) {
      r = err instanceof RolledBack
        ? err.result
        : { status: 'rejected', summary: 'Change from your phone', message: 'Billable hit a problem applying this change.' }
      if (!(err instanceof RolledBack)) console.error('[phone] could not record action', err)
      record(a, r)
      if (a.kind.startsWith('timer.')) timer.syncFromDatabase()
    }
    results.push({ status: r.status, summary: r.summary, message: r.message })
  }
  return results
}

// ---------- Connecting ----------

const pendingConnects = new Map<string, { token: string; origin: string; expires: number }>()

/**
 * Content HQ, inside Billable's own window, handed over a connection token.
 * Nothing is stored until the person confirms in Billable.
 */
/**
 * Which Content HQ account a token belongs to, asked of Content HQ itself, so
 * the confirmation can name it. A page that slipped in someone else's token
 * would show their email. Content HQ versions without this route answer 404,
 * and the confirmation just goes without the email.
 */
async function accountForToken(token: string, origin: string): Promise<{ email: string | null; refused: boolean }> {
  try {
    lastRequestAt = Date.now()
    const res = await net.fetch(new URL('/api/billable/account', origin).toString(), {
      headers: { Authorization: `Bearer ${token}`, Accept: 'application/json' },
      signal: AbortSignal.timeout(8_000),
    })
    if (res.status === 401) return { email: null, refused: true }
    if (!res.ok) return { email: null, refused: false }
    const json = await res.json().catch(() => null) as any
    const email = json?.ok === true && typeof json.email === 'string' ? json.email.trim().slice(0, 200) : ''
    return { email: email || null, refused: false }
  } catch {
    return { email: null, refused: false }
  }
}

export async function requestConnect(token: string, origin: string) {
  if (!TOKEN_PATTERN.test(token)) return
  let url: URL
  try { url = new URL(origin) } catch { return }
  const local = url.protocol === 'http:' && (url.hostname === 'localhost' || url.hostname === '127.0.0.1')
  if (url.protocol !== 'https:' && !local) return
  const win = BrowserWindow.getAllWindows()[0]
  if (win) { if (win.isMinimized()) win.restore(); win.show(); win.focus() }

  const account = await accountForToken(token, url.origin)
  if (account.refused) {
    broadcast('phone:connect-error', 'Content HQ didn’t recognise that connection. Click Connect this Mac again.')
    return
  }
  for (const [key, value] of pendingConnects) if (value.expires < Date.now()) pendingConnects.delete(key)
  const requestId = crypto.randomUUID()
  pendingConnects.set(requestId, { token, origin: url.origin, expires: Date.now() + 5 * 60_000 })
  broadcast('phone:connect-request', {
    requestId,
    host: url.host,
    account: account.email,
    profileName: getActiveProfile()?.name || null,
  })
}

export function confirmConnect(requestId: string, accept: boolean): PhoneSyncStatus {
  const pending = pendingConnects.get(requestId)
  pendingConnects.delete(requestId)
  if (!pending || !accept || pending.expires < Date.now()) return status
  if (!safeStorage.isEncryptionAvailable()) {
    setStatus({ state: 'error', message: 'Billable couldn’t use the Keychain to store the connection safely.' })
    return status
  }
  const profileId = getActiveProfileId()
  const c: Connection = { profileId, origin: pending.origin, token: pending.token, connectedAt: new Date().toISOString() }
  writeConnection(c)
  loadedProfileId = null // reload from disk
  loadForActiveProfile()
  syncSoon()
  return status
}

export async function disconnectPhone(): Promise<PhoneSyncStatus & { remoteDisconnected: boolean }> {
  loadForActiveProfile()
  const c = conn
  if (!c) return { ...status, remoteDisconnected: true }
  conn = null
  if (nextTimer) { clearTimeout(nextTimer); nextTimer = null; nextDueAt = 0 }
  forgetConnection(c.profileId)
  setStatus({ connected: false, state: 'off', host: null, connectedAt: null, lastSyncAt: null, message: null })
  // Tell Content HQ too, so it deletes its copy. If it can't be reached, the
  // person is told to disconnect there as well.
  let remoteDisconnected = false
  try {
    const res = await post(c, '/api/billable/disconnect', {})
    remoteDisconnected = res.ok || res.status === 401
  } catch { /* offline */ }
  return { ...status, remoteDisconnected }
}

export function syncPhoneNow(): PhoneSyncStatus {
  loadForActiveProfile()
  failures = 0
  syncSoon()
  return status
}

export function listPhoneActivity(limit = 20) {
  try {
    return db.getDatabase().prepare(`
      SELECT id, kind, status, summary, message, occurred_at, applied_at
      FROM phone_actions ORDER BY applied_at DESC LIMIT ?
    `).all(Math.min(100, Math.max(1, limit)))
  } catch {
    return []
  }
}

// ---------- Open at login ----------

export function getOpenAtLogin() {
  if (!app.isPackaged) return { available: false, enabled: false, needsApproval: false }
  const s = app.getLoginItemSettings() as Electron.LoginItemSettings & { status?: string }
  return { available: true, enabled: !!s.openAtLogin, needsApproval: s.status === 'requires-approval' }
}

export function setOpenAtLogin(enabled: boolean) {
  if (app.isPackaged) app.setLoginItemSettings({ openAtLogin: !!enabled })
  return getOpenAtLogin()
}

// ---------- Start ----------

export function startPhoneSync(timerManager: TimerManager) {
  timer = timerManager
  loadForActiveProfile()
  if (conn) syncSoon()

  // Anything changed on the Mac (or a profile switch) → sync soon
  setInterval(() => {
    const profileId = getActiveProfileId()
    if (profileId !== loadedProfileId) {
      if (nextTimer) { clearTimeout(nextTimer); nextTimer = null; nextDueAt = 0 }
      loadForActiveProfile()
      if (conn) syncSoon()
      return
    }
    if (!conn || inFlight) return
    const n = totalChanges()
    if (lastChanges !== -1 && n !== lastChanges && status.state !== 'offline') schedule(2_000)
    lastChanges = n
  }, CHANGE_POLL_MS)

  // Back from sleep or the lock screen: catch up now, retrying quickly while
  // the network comes back
  const wake = () => {
    fastRetryUntil = Date.now() + 2 * 60_000
    failures = 0
    schedule(2_000)
  }
  powerMonitor.on('resume', wake)
  powerMonitor.on('unlock-screen', wake)
}
