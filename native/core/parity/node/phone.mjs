// Phone parity: runs ../scenarios/phone/phone.json through the Electron app's
// phone-snapshot.ts and phone-actions.ts (plus a copy of phone-sync.ts's
// applyIncoming, which isn't exported) and writes ../out/phone/results.json
// for tests/phone_parity.rs. Snapshots are kept as the exact JSON text.
//
//   node phone.mjs
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'

const require = createRequire(import.meta.url)
const here = path.dirname(fileURLToPath(import.meta.url))
const bundlePath = path.join(here, 'dist', 'phone.cjs')
await build({
  entryPoints: [path.join(here, 'phone-entry.ts')],
  outfile: bundlePath,
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['better-sqlite3'],
  alias: { electron: path.join(here, 'electron-stub.js') },
  logLevel: 'warning',
})

const sc = JSON.parse(fs.readFileSync(path.join(here, '..', 'scenarios', 'phone', 'phone.json'), 'utf8'))
process.env.TZ = sc.tz
const RealDate = Date
let nowMs = RealDate.parse(sc.now)
class FakeDate extends RealDate {
  constructor(...args) { if (args.length === 0) super(nowMs); else super(...args) }
  static now() { return nowMs }
}
globalThis.Date = FakeDate
globalThis.setInterval = () => 0
globalThis.clearInterval = () => {}

const outDir = path.join(here, '..', 'out', 'phone')
fs.rmSync(outDir, { recursive: true, force: true })
const userData = path.join(outDir, 'userData')
fs.mkdirSync(path.join(userData, 'profiles', 'default'), { recursive: true })
process.env.BILLABLE_USER_DATA = userData
const B = require(bundlePath)
B.db.initDatabase('default')
const database = B.db.getDatabase()
for (const s of sc.seed) database.exec(s)
const timer = new B.TimerManager()

// ---- phone-sync.ts applyIncoming, verbatim apart from `timer` and logging ----
function applyIncoming(list) {
  const find = database.prepare('SELECT id FROM phone_actions WHERE id = ?')
  const reack = database.prepare('UPDATE phone_actions SET acked = 0 WHERE id = ?')
  const insert = database.prepare(`
    INSERT OR IGNORE INTO phone_actions (id, kind, status, summary, message, created_billable_id, occurred_at, applied_at, acked)
    VALUES (@id, @kind, @status, @summary, @message, @created, @occurred, @applied, 0)
  `)
  const record = (a, r) => insert.run({
    id: a.id,
    kind: a.kind.slice(0, 40),
    status: r.status,
    summary: r.summary.slice(0, 200),
    message: r.message ? r.message.slice(0, 200) : null,
    created: r.createdBillableId || null,
    occurred: typeof a.occurredAt === 'string' ? a.occurredAt.slice(0, 40) : null,
    applied: new Date().toISOString(),
  })
  class RolledBack extends Error { constructor(result) { super('rolled back'); this.result = result } }
  const applyOne = database.transaction(a => {
    const r = B.applyAction(a, timer)
    if (r.status === 'rejected') throw new RolledBack(r)
    record(a, r)
    return r
  })
  const results = []
  for (const raw of list.slice(0, 100)) {
    const a = raw
    if (!a || typeof a.id !== 'string' || !/^[0-9a-f-]{36}$/i.test(a.id) || typeof a.kind !== 'string') continue
    a.id = a.id.toLowerCase()
    if (find.get(a.id)) { reack.run(a.id); continue }
    let r
    try {
      r = applyOne(a)
    } catch (err) {
      r = err instanceof RolledBack
        ? err.result
        : { status: 'rejected', summary: 'Change from your phone', message: 'Billable hit a problem applying this change.' }
      record(a, r)
      if (a.kind.startsWith('timer.')) timer.syncFromDatabase()
    }
    results.push({ status: r.status, summary: r.summary, message: r.message })
  }
  return results
}

function unackedRows() {
  return database.prepare(`
    SELECT id, status, message, created_billable_id FROM phone_actions
    WHERE acked = 0 ORDER BY applied_at LIMIT 200
  `).all().map(r => {
    const ack = { actionId: r.id, status: r.status }
    if (r.message) ack.message = String(r.message).slice(0, 200)
    if (r.created_billable_id) ack.createdBillableId = String(r.created_billable_id)
    return ack
  })
}

const steps = []
for (const [op, ...args] of sc.steps) {
  let value
  if (op === 'snapshot') value = JSON.stringify(B.buildSnapshot(timer))
  else if (op === 'batch') { nowMs = RealDate.parse(args[0]); value = applyIncoming(JSON.parse(JSON.stringify(args[1]))) }
  else if (op === 'acks') value = unackedRows()
  else if (op === 'ack') {
    const mark = database.prepare('UPDATE phone_actions SET acked = 1 WHERE id = ?')
    database.transaction(() => { for (const id of args[0]) mark.run(id) })()
    value = null
  } else if (op === 'activity') {
    value = database.prepare(`
      SELECT id, kind, status, summary, message, occurred_at, applied_at
      FROM phone_actions ORDER BY applied_at DESC LIMIT ?
    `).all(Math.min(100, Math.max(1, args[0])))
  } else if (op === 'sql') value = database.prepare(args[0]).all()
  else throw new Error(`unknown op ${op}`)
  steps.push({ op, value: JSON.parse(JSON.stringify(value ?? null)) })
}
B.db.closeDatabase()
fs.writeFileSync(path.join(outDir, 'results.json'), JSON.stringify(steps, null, 2))
console.log(`phone: ${steps.length} steps → out/phone/results.json`)
