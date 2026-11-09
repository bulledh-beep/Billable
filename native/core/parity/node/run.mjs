// Runs parity scenarios through the Electron app's own database.ts,
// profiles.ts and timer-manager.ts (bundled by build.mjs) and records every
// result, so the Rust port can be checked against them.
//
//   node run.mjs ../scenarios/*.json
//
// Writes ../out/<scenario>/results.json and leaves the final data folder in
// ../out/<scenario>/userData. Never touches the real Billable data: the "dev"
// setup copies from BILLABLE_DEV_DATA (default ~/Library/Application
// Support/Billable Dev) into the out folder.

import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const require = createRequire(import.meta.url)
const here = path.dirname(fileURLToPath(import.meta.url))
const outRoot = path.resolve(here, '..', 'out')
const bundlePath = path.join(here, 'dist', 'billable.cjs')
const Database = require('better-sqlite3')

const RealDate = Date
let nowMs = 0
class FakeDate extends RealDate {
  constructor(...args) {
    if (args.length === 0) super(nowMs)
    else super(...args)
  }
  static now() { return nowMs }
}
globalThis.Date = FakeDate
// TimerManager ticks with setInterval; nothing to tick here.
globalThis.setInterval = () => 0
globalThis.clearInterval = () => {}

export function devDataDir() {
  return process.env.BILLABLE_DEV_DATA || path.join(os.homedir(), 'Library', 'Application Support', 'Billable Dev')
}

function setup(spec, userData) {
  fs.mkdirSync(userData, { recursive: true })
  if (spec === 'fresh') return true
  if (spec === 'dev') {
    const src = devDataDir()
    if (path.basename(src) === 'Billable') throw new Error('Refusing to read the real Billable data folder')
    if (!fs.existsSync(path.join(src, 'profiles.json'))) return false
    fs.copyFileSync(path.join(src, 'profiles.json'), path.join(userData, 'profiles.json'))
    for (const id of fs.readdirSync(path.join(src, 'profiles'))) {
      const from = path.join(src, 'profiles', id)
      if (!fs.statSync(from).isDirectory()) continue
      fs.mkdirSync(path.join(userData, 'profiles', id), { recursive: true })
      for (const f of ['billable.db', 'billable.db-wal', 'billable.db-shm']) {
        if (fs.existsSync(path.join(from, f))) fs.copyFileSync(path.join(from, f), path.join(userData, 'profiles', id, f))
      }
    }
    return true
  }
  if (spec && spec.legacy_sql) {
    const db = new Database(path.join(userData, 'billable.db'))
    for (const s of spec.legacy_sql) db.exec(s)
    db.close()
    return true
  }
  throw new Error(`unknown setup ${JSON.stringify(spec)}`)
}

function seededRandom(seed) {
  return () => {
    seed |= 0; seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/** JSON round trip, then data-folder paths → "<userData>" and secrets redacted. */
export function normalize(value, userData) {
  if (value === undefined) return null
  const json = JSON.parse(JSON.stringify(value))
  const walk = (v, key) => {
    if (typeof v === 'string') {
      if (key && /token|secret/i.test(key)) return '<redacted>'
      return v.split(userData).join('<userData>')
    }
    if (Array.isArray(v)) return v.map(x => walk(x))
    if (v && typeof v === 'object') {
      const o = {}
      for (const [k, x] of Object.entries(v)) {
        // settings rows: { key, value } pairs from raw SQL
        o[k] = walk(x, k === 'value' && typeof v.key === 'string' ? v.key : k)
      }
      return o
    }
    return v
  }
  return walk(json)
}

function resolveRefs(v, saved) {
  if (Array.isArray(v)) return v.map(x => resolveRefs(x, saved))
  if (v && typeof v === 'object') {
    const keys = Object.keys(v)
    if (keys.length === 1 && keys[0] === '$') {
      const [name, ...rest] = String(v.$).split('.')
      let cur = saved[name]
      for (const p of rest) cur = cur == null ? undefined : cur[p]
      if (cur === undefined) throw new Error(`unresolved ref ${v.$}`)
      return cur
    }
    const o = {}
    for (const [k, x] of Object.entries(v)) o[k] = resolveRefs(x, saved)
    return o
  }
  return v
}

function listFiles(root) {
  const out = []
  const walk = (dir, rel) => {
    for (const name of fs.readdirSync(dir).sort()) {
      const full = path.join(dir, name)
      const r = rel ? `${rel}/${name}` : name
      if (fs.statSync(full).isDirectory()) { out.push(`${r}/`); walk(full, r) }
      else if (!/-(wal|shm)$/.test(name)) out.push(r)
    }
  }
  if (fs.existsSync(root)) walk(root, '')
  return out
}

const AT_ARG = { startTimer: 2, stopTimer: 1, pauseTimer: 1, resumeTimer: 1, 'timer.start': 2, 'timer.stop': 0, 'timer.pause': 0, 'timer.resume': 0 }

function runScenario(file) {
  const sc = JSON.parse(fs.readFileSync(file, 'utf8'))
  process.env.TZ = sc.tz
  const outDir = path.join(outRoot, sc.name)
  fs.rmSync(outDir, { recursive: true, force: true })
  const userData = path.join(outDir, 'userData')
  if (!setup(sc.setup, userData)) {
    console.log(`${sc.name}: skipped (no dev data)`)
    fs.rmSync(outDir, { recursive: true, force: true })
    return
  }
  process.env.BILLABLE_USER_DATA = userData
  nowMs = RealDate.parse(sc.now)
  Math.random = seededRandom(42)
  delete require.cache[bundlePath]
  const B = require(bundlePath)
  let tm = null
  const saved = {}
  const steps = []
  const wallStart = RealDate.now()

  const harness = {
    now: iso => { nowMs = RealDate.parse(iso); return null },
    advance: ms => { nowMs += ms; return null },
    open: id => { B.db.initDatabase(id); return null },
    close: () => { B.db.closeDatabase(); return null },
    sql: (q, params = []) => B.db.getDatabase().prepare(q).all(...params),
    exec: q => { B.db.getDatabase().exec(q); return null },
    export: rel => {
      const p = path.join(userData, rel)
      if (fs.existsSync(p)) fs.unlinkSync(p)
      B.db.getDatabase().exec(`VACUUM INTO '${p.replace(/'/g, "''")}'`)
      return p
    },
    files: () => listFiles(userData),
    'timer.new': () => { tm = new B.TimerManager(); return null },
    'timer.start': (p, d, at) => tm.start(p, d, at),
    'timer.stop': at => tm.stop(at),
    'timer.pause': at => tm.pause(at),
    'timer.resume': at => tm.resume(at),
    'timer.toggle': () => tm.togglePause(),
    'timer.state': () => ({ active: tm.getActive(), paused: tm.getPaused() }),
    'profile.list': () => ({ profiles: B.profiles.listProfiles(), active: B.profiles.getActiveProfile() }),
  }

  sc.steps.forEach((raw, i) => {
    const step = Array.isArray(raw) ? { op: raw[0], args: raw.slice(1) } : raw
    const record = { i, op: step.op }
    try {
      const args = resolveRefs(step.args || [], saved)
      const at = AT_ARG[step.op]
      if (at !== undefined && typeof args[at] === 'string') args[at] = new Date(args[at])
      const fn = harness[step.op] || B.db[step.op] || B.profiles[step.op]
      if (typeof fn !== 'function') throw new Error(`unknown op ${step.op}`)
      const value = normalize(fn(...args), userData)
      record.ok = true
      record.value = value
      if (step.as) saved[step.as] = value
    } catch (err) {
      record.ok = false
      record.error = String(err && err.message || err)
    }
    steps.push(record)
  })

  try { B.db.closeDatabase() } catch {}
  const wallEnd = RealDate.now()
  const files = listFiles(userData)
  fs.writeFileSync(path.join(outDir, 'results.json'), JSON.stringify({
    name: sc.name, tz: sc.tz, wall: [wallStart, wallEnd], steps, files,
  }, null, 1))
  const failed = steps.filter(s => !s.ok).length
  console.log(`${sc.name}: ${steps.length} steps (${failed} errors, expected or not) → ${path.relative(process.cwd(), outDir)}`)
}

const files = process.argv.slice(2)
if (!fs.existsSync(bundlePath)) {
  console.error('Run `node build.mjs` first')
  process.exit(1)
}
for (const f of files) runScenario(f)
