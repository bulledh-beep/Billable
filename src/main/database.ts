import Database from 'better-sqlite3'
import path from 'path'
import { getProfileDbPath, getActiveProfileId } from './profiles'

let db: Database.Database

// ============ Local dates ============
// The main process runs in the user's timezone, so these give the user's
// calendar day. Never use toISOString().slice(0, 10) for "today": that's UTC.

function pad2(n: number) { return String(n).padStart(2, '0') }

export function localDate(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`
}

export function localToday(): string {
  return localDate(new Date())
}

/** Year of a YYYY-MM-DD string without timezone drift. */
function yearOf(dateStr: string | null | undefined): number {
  const y = parseInt(String(dateStr || '').slice(0, 4))
  return Number.isFinite(y) && y > 1900 ? y : new Date().getFullYear()
}

/** Milliseconds for SQLite "YYYY-MM-DD HH:MM:SS" (UTC) or ISO strings. 0 if missing. */
function sqliteTimeMs(value: string | null | undefined): number {
  if (!value) return 0
  const s = String(value)
  const t = Date.parse(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(s) ? `${s.replace(' ', 'T')}Z` : s)
  return Number.isFinite(t) ? t : 0
}

/** UTC instants bounding a local-date range, end inclusive. */
function localRangeToUtc(startDate: string, endDate: string): [string, string] {
  const start = new Date(`${startDate}T00:00:00`)
  const end = new Date(`${endDate}T00:00:00`)
  end.setDate(end.getDate() + 1)
  return [start.toISOString(), end.toISOString()]
}

/** Invoice status as the user sees it: a sent invoice past its due date is overdue. */
export function effectiveInvoiceStatus(stored: string, dueDate: string | null, today = localToday()): string {
  if (stored === 'paid') return 'paid'
  if (stored === 'draft') return 'draft'
  if (dueDate && String(dueDate).slice(0, 10) < today) return 'overdue'
  return 'sent'
}

/** SQL for an unbilled, finished, billable time entry (alias te). */
const UNBILLED_SQL = `te.end_time IS NOT NULL AND te.is_billable = 1 AND te.is_invoiced = 0 AND te.invoice_id IS NULL`

/**
 * Open (or re-open) the database for a specific profile.
 * If a database is already open, it is closed first — safe to call when switching profiles.
 */
export function initDatabase(profileId?: string): Database.Database {
  closeDatabase()

  const id = profileId || getActiveProfileId()
  const dbPath = getProfileDbPath(id)
  db = new Database(dbPath)

  db.pragma('journal_mode = WAL')
  db.pragma('foreign_keys = ON')

  createTables()
  runMigrations()
  seedDefaults()

  return db
}

export function closeDatabase() {
  if (db) {
    try {
      db.close()
    } catch {
      // ignore close errors — better-sqlite3 occasionally throws if already closed
    }
  }
}

// Idempotent migrations — safe to re-run on every app launch.
function runMigrations() {
  // ----- Timer pause/resume state -----
  // Keep the original start time for reporting while active_since tracks the
  // current running segment. Existing open timers continue from start_time.
  addColumnIfMissing('time_entries', 'paused_at', 'TEXT')
  addColumnIfMissing('time_entries', 'active_since', 'TEXT')
  db.prepare(`
    UPDATE time_entries
    SET active_since = start_time
    WHERE end_time IS NULL AND paused_at IS NULL AND active_since IS NULL
  `).run()

  // ----- Phase 1: tax & expense tracking -----
  db.exec(`
    CREATE TABLE IF NOT EXISTS tax_settings (
      id INTEGER PRIMARY KEY CHECK (id = 1),
      business_name TEXT DEFAULT '',
      business_address TEXT DEFAULT '',
      gst_hst_number TEXT DEFAULT '',
      gst_hst_registered INTEGER DEFAULT 0,
      province TEXT DEFAULT '',
      fiscal_year_start TEXT DEFAULT '01-01',
      default_tax_rate REAL DEFAULT 0,
      income_tax_bracket REAL DEFAULT 25,
      currency TEXT DEFAULT 'CAD',
      updated_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS expenses (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      date TEXT NOT NULL,
      category TEXT NOT NULL DEFAULT 'other',
      description TEXT DEFAULT '',
      amount REAL DEFAULT 0,
      tax_year INTEGER NOT NULL,
      receipt_note TEXT DEFAULT '',
      receipt_id INTEGER,
      created_at TEXT DEFAULT (datetime('now'))
    );
  `)

  // Seed the single tax_settings row if missing
  db.prepare(`
    INSERT OR IGNORE INTO tax_settings (id, business_name) VALUES (1, '')
  `).run()

  // Add new tax/payment columns to invoices (idempotent)
  addColumnIfMissing('invoices', 'tax_year', 'INTEGER')
  addColumnIfMissing('invoices', 'payment_date', 'TEXT')
  addColumnIfMissing('invoices', 'payment_method', 'TEXT')
  addColumnIfMissing('invoices', 'currency', "TEXT DEFAULT 'CAD'")
  addColumnIfMissing('invoices', 'gst_hst_applicable', 'INTEGER DEFAULT 0')
  addColumnIfMissing('invoices', 'gst_hst_number', 'TEXT')
  addColumnIfMissing('invoices', 'gst_hst_rate', 'REAL DEFAULT 0')
  addColumnIfMissing('invoices', 'gst_hst_amount', 'REAL DEFAULT 0')

  // Backfill tax_year for any existing invoices
  db.prepare(`
    UPDATE invoices
    SET tax_year = CAST(strftime('%Y', issue_date) AS INTEGER)
    WHERE tax_year IS NULL
  `).run()

  // ----- Commission tracking (appointment-setting commissions) -----
  db.exec(`
    CREATE TABLE IF NOT EXISTS commissions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      client_name TEXT NOT NULL,
      job_type TEXT NOT NULL DEFAULT 'solar' CHECK(job_type IN ('solar', 'roofing')),
      appointment_date TEXT,
      closer_name TEXT DEFAULT '',
      status TEXT NOT NULL DEFAULT 'appointment_set'
        CHECK(status IN ('appointment_set','appointment_attended','closed_waiting','paid','lost','cancelled','needs_review')),
      payment_status TEXT NOT NULL DEFAULT 'unpaid'
        CHECK(payment_status IN ('unpaid','pending','paid')),
      system_size_kw REAL,
      contract_amount REAL,
      calculated_commission REAL DEFAULT 0,
      manual_override REAL,
      needs_review INTEGER DEFAULT 0,
      notes TEXT DEFAULT '',
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS commission_invoices (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      invoice_number TEXT NOT NULL,
      category TEXT NOT NULL DEFAULT 'mixed' CHECK(category IN ('solar','roofing','mixed')),
      date_from TEXT,
      date_to TEXT,
      status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','sent','paid','cancelled')),
      job_count INTEGER DEFAULT 0,
      total REAL DEFAULT 0,
      notes TEXT DEFAULT '',
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      paid_at TEXT
    );
  `)

  // Invoice-tracking fields on commission jobs (additive)
  addColumnIfMissing('commissions', 'invoice_id', 'INTEGER')
  addColumnIfMissing('commissions', 'invoice_status', "TEXT DEFAULT 'not_invoiced'")
  addColumnIfMissing('commissions', 'invoiced_at', 'TEXT')
  addColumnIfMissing('commissions', 'paid_at', 'TEXT')

  // ----- Billing links: every time entry knows which invoice it is on -----
  addColumnIfMissing('time_entries', 'invoice_id', 'INTEGER REFERENCES invoices(id) ON DELETE SET NULL')
  addColumnIfMissing('invoices', 'sent_at', 'TEXT')
  addColumnIfMissing('invoices', 'line_style', 'TEXT')
  addColumnIfMissing('invoice_items', 'project_id', 'INTEGER')
  addColumnIfMissing('invoice_items', 'source_key', 'TEXT')
  addColumnIfMissing('invoice_items', 'custom_description', 'INTEGER DEFAULT 0')
  db.exec('CREATE INDEX IF NOT EXISTS idx_time_entries_invoice ON time_entries(invoice_id)')
  db.exec('CREATE INDEX IF NOT EXISTS idx_time_entries_project ON time_entries(project_id)')
  backfillInvoiceLinks()
}

/**
 * One-time upgrade from the old yes/no "invoiced" flag to real links.
 * Each previously-invoiced entry is linked to its project's invoice. When a
 * project has several invoices, the entry goes to the first invoice issued on
 * or after the day the work happened. Entries with no matching invoice keep
 * their flag and show as "Invoiced" without a number.
 *
 * The database is copied to a backup file first; if that fails we skip the
 * upgrade this launch and try again next time.
 */
function backfillInvoiceLinks() {
  const done = db.prepare("SELECT value FROM settings WHERE key = 'billing_links_v1'").get()
  if (done) return

  const markDone = (note: string) =>
    db.prepare("INSERT OR REPLACE INTO settings (key, value) VALUES ('billing_links_v1', ?)")
      .run(`${new Date().toISOString()} ${note}`)

  // Nothing to upgrade in a fresh profile, so skip the backup
  const existing = db.prepare(`
    SELECT (SELECT COUNT(*) FROM invoices) + (SELECT COUNT(*) FROM time_entries WHERE is_invoiced = 1) AS n
  `).get() as { n: number }
  if (!existing.n) {
    markDone('empty')
    return
  }

  const backup = backupDatabase('before-billing-upgrade')
  if (!backup) return

  const tx = db.transaction(() => {
    const flagged = db.prepare(`
      SELECT te.id, te.project_id, te.created_at, te.end_time, p.client_id, p.name AS project_name
      FROM time_entries te JOIN projects p ON p.id = te.project_id
      WHERE te.is_invoiced = 1 AND te.invoice_id IS NULL
    `).all() as Array<{ id: number; project_id: number; created_at: string; end_time: string | null; client_id: number; project_name: string }>
    const invoices = db.prepare('SELECT id, client_id, project_id, created_at FROM invoices').all() as
      Array<{ id: number; client_id: number; project_id: number | null; created_at: string }>
    const lineText = db.prepare('SELECT description FROM invoice_items WHERE invoice_id = ?')
    const link = db.prepare('UPDATE time_entries SET invoice_id = ? WHERE id = ?')

    // The old multi-project builder stored no project on the invoice but
    // prefixed every line with "[Project name]", so read coverage from that.
    const coverage = new Map<number, string[]>()
    for (const inv of invoices) {
      if (inv.project_id != null) continue
      coverage.set(inv.id, (lineText.all(inv.id) as Array<{ description: string }>).map(r => String(r.description || '')))
    }
    const covers = (inv: { id: number; project_id: number | null }, e: { project_id: number; project_name: string }) =>
      inv.project_id === e.project_id ||
      (inv.project_id == null && (coverage.get(inv.id) || []).some(d => d.startsWith(`[${e.project_name}]`)))

    for (const e of flagged) {
      const candidates = invoices.filter(i => i.client_id === e.client_id && covers(i, e))
      if (candidates.length === 0) continue
      // The old code flagged time when an invoice was created, so the right
      // invoice is the first one made after this entry existed and had ended.
      const ready = Math.max(sqliteTimeMs(e.created_at), sqliteTimeMs(e.end_time))
      const after = candidates
        .filter(i => sqliteTimeMs(i.created_at) >= ready - 1000)
        .sort((a, b) => sqliteTimeMs(a.created_at) - sqliteTimeMs(b.created_at))
      const target = after[0] || (candidates.length === 1 ? candidates[0] : null)
      if (target) link.run(target.id, e.id)
    }

    // Older line items belong to their invoice's single project
    db.prepare(`
      UPDATE invoice_items
      SET project_id = (SELECT project_id FROM invoices WHERE invoices.id = invoice_items.invoice_id)
      WHERE project_id IS NULL
    `).run()
    // Multi-project lines name their project in a "[Project]" prefix
    const orphanLines = db.prepare(`
      SELECT it.id, it.description, i.client_id FROM invoice_items it
      JOIN invoices i ON i.id = it.invoice_id
      WHERE it.project_id IS NULL AND i.project_id IS NULL
    `).all() as Array<{ id: number; description: string; client_id: number }>
    const clientProjects = db.prepare('SELECT id, name FROM projects WHERE client_id = ?')
    const setLineProject = db.prepare('UPDATE invoice_items SET project_id = ? WHERE id = ?')
    for (const line of orphanLines) {
      const match = (clientProjects.all(line.client_id) as Array<{ id: number; name: string }>)
        .find(p => String(line.description || '').startsWith(`[${p.name}]`))
      if (match) setLineProject.run(match.id, line.id)
    }

    // "Overdue" is now worked out from the due date, so store it as sent
    db.prepare("UPDATE invoices SET status = 'sent' WHERE status = 'overdue'").run()

    markDone(`backup=${path.basename(backup)}`)
  })
  tx()
}

/**
 * Consistent copy of the open database next to it (works with WAL).
 * Returns the backup path, or null if it failed.
 */
export function backupDatabase(tag: string): string | null {
  try {
    const dir = path.dirname(db.name)
    const stamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19)
    const file = path.join(dir, `billable-backup-${tag}-${stamp}.db`)
    db.exec(`VACUUM INTO '${file.replace(/'/g, "''")}'`)
    return file
  } catch (err) {
    console.error('Database backup failed:', err)
    return null
  }
}

function addColumnIfMissing(table: string, column: string, def: string) {
  const cols = db.prepare(`PRAGMA table_info(${table})`).all() as Array<{ name: string }>
  if (!cols.some(c => c.name === column)) {
    db.exec(`ALTER TABLE ${table} ADD COLUMN ${column} ${def}`)
  }
}

export function getDatabase(): Database.Database {
  return db
}

function createTables() {
  db.exec(`
    CREATE TABLE IF NOT EXISTS clients (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL,
      company TEXT DEFAULT '',
      email TEXT DEFAULT '',
      address TEXT DEFAULT '',
      default_rate REAL DEFAULT 0,
      currency TEXT DEFAULT 'USD',
      created_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS projects (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      client_id INTEGER NOT NULL,
      name TEXT NOT NULL,
      description TEXT DEFAULT '',
      rate REAL DEFAULT 0,
      status TEXT DEFAULT 'active' CHECK(status IN ('active', 'paused', 'complete', 'archived')),
      color TEXT DEFAULT '#F5A623',
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (client_id) REFERENCES clients(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS time_entries (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER NOT NULL,
      description TEXT DEFAULT '',
      start_time TEXT NOT NULL,
      end_time TEXT,
      duration_minutes REAL DEFAULT 0,
      paused_at TEXT,
      active_since TEXT,
      is_billable INTEGER DEFAULT 1,
      is_invoiced INTEGER DEFAULT 0,
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS invoices (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      project_id INTEGER,
      client_id INTEGER NOT NULL,
      invoice_number TEXT NOT NULL UNIQUE,
      issue_date TEXT NOT NULL,
      due_date TEXT NOT NULL,
      status TEXT DEFAULT 'draft' CHECK(status IN ('draft', 'sent', 'paid', 'overdue')),
      subtotal REAL DEFAULT 0,
      tax_rate REAL DEFAULT 0,
      total REAL DEFAULT 0,
      notes TEXT DEFAULT '',
      pdf_path TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE SET NULL,
      FOREIGN KEY (client_id) REFERENCES clients(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS invoice_items (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      invoice_id INTEGER NOT NULL,
      description TEXT NOT NULL,
      quantity REAL DEFAULT 0,
      unit_price REAL DEFAULT 0,
      total REAL DEFAULT 0,
      FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
    );

    CREATE TABLE IF NOT EXISTS settings (
      key TEXT PRIMARY KEY,
      value TEXT
    );
  `)
}

function seedDefaults() {
  const defaults: Record<string, string> = {
    business_name: '',
    business_email: '',
    business_address: '',
    business_logo: '',
    tax_id: '',
    default_currency: 'USD',
    default_rate: '100',
    invoice_prefix: 'INV-',
    invoice_next_number: '1001',
    payment_methods: JSON.stringify([{ name: 'e-Transfer', email: '' }]),
    default_payment_method: 'e-Transfer',
    time_rounding: 'none',
    theme: 'dark',
  }

  const insert = db.prepare('INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)')
  for (const [key, value] of Object.entries(defaults)) {
    insert.run(key, value)
  }
}

// ============ Client Queries ============

const CLIENT_AGGREGATES = `
  (SELECT COUNT(*) FROM projects p WHERE p.client_id = c.id) AS project_count,
  (SELECT COUNT(*) FROM projects p WHERE p.client_id = c.id AND p.status = 'active') AS active_project_count,
  (SELECT COALESCE(SUM(te.duration_minutes * p.rate / 60.0), 0)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id AND ${UNBILLED_SQL}) AS unbilled_amount,
  (SELECT COALESCE(SUM(te.duration_minutes) / 60.0, 0)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id AND ${UNBILLED_SQL}) AS unbilled_hours,
  (SELECT MIN(te.start_time)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id AND ${UNBILLED_SQL}) AS oldest_unbilled,
  (SELECT COALESCE(SUM(i.total), 0) FROM invoices i
     WHERE i.client_id = c.id AND i.status IN ('sent', 'overdue')) AS outstanding_amount,
  (SELECT COALESCE(SUM(i.total), 0) FROM invoices i
     WHERE i.client_id = c.id AND i.status IN ('sent', 'overdue') AND i.due_date < @today) AS overdue_amount,
  (SELECT COALESCE(SUM(i.total), 0) FROM invoices i
     WHERE i.client_id = c.id AND i.status = 'paid') AS paid_amount,
  (SELECT COUNT(*) FROM invoices i WHERE i.client_id = c.id) AS invoice_count,
  (SELECT MAX(te.start_time)
     FROM time_entries te JOIN projects p ON p.id = te.project_id
     WHERE p.client_id = c.id) AS last_activity
`

export function listClients() {
  return db.prepare(`SELECT c.*, ${CLIENT_AGGREGATES} FROM clients c ORDER BY c.name COLLATE NOCASE`)
    .all({ today: localToday() })
}

export function getClient(id: number) {
  return db.prepare(`SELECT c.*, ${CLIENT_AGGREGATES} FROM clients c WHERE c.id = @id`)
    .get({ today: localToday(), id })
}

/**
 * Fold one client into another: projects and invoices move to the target,
 * then the source record is removed.
 */
export function mergeClients(sourceId: number, targetId: number) {
  if (!sourceId || !targetId || sourceId === targetId) throw new Error('Pick two different clients to merge')
  const tx = db.transaction(() => {
    const source = db.prepare('SELECT id FROM clients WHERE id = ?').get(sourceId)
    const target = db.prepare('SELECT id FROM clients WHERE id = ?').get(targetId)
    if (!source || !target) throw new Error('Client not found')
    const movedProjects = db.prepare('UPDATE projects SET client_id = ? WHERE client_id = ?').run(targetId, sourceId).changes
    const movedInvoices = db.prepare('UPDATE invoices SET client_id = ? WHERE client_id = ?').run(targetId, sourceId).changes
    db.prepare('DELETE FROM clients WHERE id = ?').run(sourceId)
    return { moved_projects: movedProjects, moved_invoices: movedInvoices }
  })
  return tx()
}

export function createClient(data: any) {
  const stmt = db.prepare(`
    INSERT INTO clients (name, company, email, address, default_rate, currency)
    VALUES (@name, @company, @email, @address, @default_rate, @currency)
  `)
  const result = stmt.run({
    name: data.name,
    company: data.company || '',
    email: data.email || '',
    address: data.address || '',
    default_rate: Number(data.default_rate) || 0,
    currency: data.currency || 'CAD',
  })
  return getClient(Number(result.lastInsertRowid))
}

const CLIENT_COLUMNS = ['name', 'company', 'email', 'address', 'default_rate', 'currency']

export function updateClient(id: number, data: any) {
  const existing = db.prepare('SELECT * FROM clients WHERE id = ?').get(id) as any
  if (!existing) return null

  const fields = Object.keys(data).filter(k => CLIENT_COLUMNS.includes(k))
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  if (sets) {
    db.prepare(`UPDATE clients SET ${sets} WHERE id = @id`).run({ ...data, id })
  }

  // If the default hourly rate changed, cascade the new rate to this client's
  // projects that were still inheriting the old rate. Past invoice amounts are
  // locked into invoice_items (unit_price), so this only affects future
  // invoicing of unbilled time — exactly "unpaid instances".
  let cascadedProjects = 0
  if (
    data.default_rate !== undefined &&
    Number(data.default_rate) !== Number(existing.default_rate)
  ) {
    const result = db.prepare(`
      UPDATE projects
      SET rate = ?
      WHERE client_id = ? AND rate = ?
    `).run(data.default_rate, id, existing.default_rate)
    cascadedProjects = result.changes
  }

  const client = getClient(id) as any
  return { ...client, cascaded_projects: cascadedProjects }
}

export function deleteClient(id: number) {
  db.prepare('DELETE FROM clients WHERE id = ?').run(id)
}

// ============ Project Queries ============

const PROJECT_SELECT = `
  SELECT p.*, c.name AS client_name, c.company AS client_company,
    (SELECT COALESCE(SUM(te.duration_minutes), 0) / 60.0 FROM time_entries te
       WHERE te.project_id = p.id AND te.end_time IS NOT NULL) AS total_hours,
    (SELECT COALESCE(SUM(te.duration_minutes), 0) / 60.0 FROM time_entries te
       WHERE te.project_id = p.id AND ${UNBILLED_SQL}) AS unbilled_hours,
    (SELECT COUNT(*) FROM time_entries te
       WHERE te.project_id = p.id AND ${UNBILLED_SQL}) AS unbilled_entries,
    (SELECT MIN(te.start_time) FROM time_entries te
       WHERE te.project_id = p.id AND ${UNBILLED_SQL}) AS oldest_unbilled,
    (SELECT COALESCE(SUM(it.total), 0) FROM invoice_items it
       WHERE it.project_id = p.id) AS invoiced_amount,
    (SELECT COALESCE(SUM(it.total), 0) FROM invoice_items it JOIN invoices i ON i.id = it.invoice_id
       WHERE it.project_id = p.id AND i.status = 'paid') AS paid_amount,
    (SELECT MAX(te.start_time) FROM time_entries te WHERE te.project_id = p.id) AS last_activity
  FROM projects p
  LEFT JOIN clients c ON p.client_id = c.id
`

function withProjectMoney(p: any) {
  if (!p) return p
  const unbilledAmount = (Number(p.unbilled_hours) || 0) * (Number(p.rate) || 0)
  return {
    ...p,
    unbilled_amount: Math.round(unbilledAmount * 100) / 100,
    // Kept for older callers: what has actually been put on invoices
    billed_total: p.invoiced_amount,
  }
}

export function listProjects(clientId?: number) {
  const rows = clientId
    ? db.prepare(`${PROJECT_SELECT} WHERE p.client_id = ? ORDER BY p.created_at DESC`).all(clientId)
    : db.prepare(`${PROJECT_SELECT} ORDER BY p.created_at DESC`).all()
  return rows.map(withProjectMoney)
}

export function getProject(id: number) {
  return withProjectMoney(db.prepare(`${PROJECT_SELECT} WHERE p.id = ?`).get(id))
}

export function createProject(data: any) {
  const stmt = db.prepare(`
    INSERT INTO projects (client_id, name, description, rate, status, color)
    VALUES (@client_id, @name, @description, @rate, @status, @color)
  `)
  const result = stmt.run({
    client_id: data.client_id,
    name: data.name,
    description: data.description || '',
    rate: Number(data.rate) || 0,
    status: data.status || 'active',
    color: data.color || '#F5A623',
  })
  return getProject(result.lastInsertRowid as number)
}

const PROJECT_COLUMNS = ['client_id', 'name', 'description', 'rate', 'status', 'color']

export function updateProject(id: number, data: any) {
  const fields = Object.keys(data).filter(k => PROJECT_COLUMNS.includes(k))
  if (fields.length) {
    const sets = fields.map(f => `${f} = @${f}`).join(', ')
    db.prepare(`UPDATE projects SET ${sets} WHERE id = @id`).run({ ...data, id })
  }
  return getProject(id)
}

export function deleteProject(id: number) {
  db.prepare('DELETE FROM projects WHERE id = ?').run(id)
}

// ============ Time Entry Queries ============

const ENTRY_SELECT = `
  SELECT te.*, p.name AS project_name, p.color AS project_color, p.rate, p.client_id,
         p.status AS project_status, c.name AS client_name,
         i.invoice_number, i.status AS invoice_stored_status, i.due_date AS invoice_due_date
  FROM time_entries te
  LEFT JOIN projects p ON te.project_id = p.id
  LEFT JOIN clients c ON p.client_id = c.id
  LEFT JOIN invoices i ON i.id = te.invoice_id
`

/**
 * Where an entry's money is:
 *   unbilled | nonbillable | draft | sent | overdue | paid
 *   invoiced = flagged by an older version with no invoice we could match
 */
function withBillingState(e: any, today = localToday()) {
  if (!e) return e
  let state: string
  if (e.invoice_id && e.invoice_number) state = effectiveInvoiceStatus(e.invoice_stored_status, e.invoice_due_date, today)
  else if (e.is_invoiced) state = 'invoiced'
  else if (!e.is_billable) state = 'nonbillable'
  else state = 'unbilled'
  return { ...e, billing_state: state, invoice_status: e.invoice_id ? state : null }
}

export function listTimeEntries(projectId?: number) {
  const today = localToday()
  const rows = projectId
    ? db.prepare(`${ENTRY_SELECT} WHERE te.project_id = ? ORDER BY te.start_time DESC`).all(projectId)
    : db.prepare(`${ENTRY_SELECT} ORDER BY te.start_time DESC`).all()
  return rows.map(r => withBillingState(r, today))
}

export function getTimeEntry(id: number) {
  return withBillingState(db.prepare(`${ENTRY_SELECT} WHERE te.id = ?`).get(id))
}

export function createTimeEntry(data: any) {
  const stmt = db.prepare(`
    INSERT INTO time_entries (project_id, description, start_time, end_time, duration_minutes, is_billable, is_invoiced)
    VALUES (@project_id, @description, @start_time, @end_time, @duration_minutes, @is_billable, 0)
  `)
  const result = stmt.run({
    project_id: data.project_id,
    description: data.description || '',
    start_time: data.start_time,
    end_time: data.end_time || null,
    duration_minutes: data.duration_minutes || 0,
    is_billable: data.is_billable ?? 1,
  })
  return getTimeEntry(result.lastInsertRowid as number)
}

// Billing links are managed by invoices, never written from the entry editor
const ENTRY_COLUMNS = ['project_id', 'description', 'start_time', 'end_time', 'duration_minutes', 'is_billable']

export function updateTimeEntry(id: number, data: any) {
  const fields = Object.keys(data).filter(k => ENTRY_COLUMNS.includes(k))
  if (fields.length) {
    const sets = fields.map(f => `${f} = @${f}`).join(', ')
    db.prepare(`UPDATE time_entries SET ${sets} WHERE id = @id`).run({ ...data, id })
  }
  return getTimeEntry(id)
}

export function deleteTimeEntry(id: number) {
  db.prepare('DELETE FROM time_entries WHERE id = ?').run(id)
}

/** Mark finished, not-yet-invoiced entries billable or not ("Don't bill"). */
export function setEntriesBillable(ids: number[], billable: boolean) {
  const list = (ids || []).map(Number).filter(Boolean)
  if (!list.length) return 0
  const stmt = db.prepare(`
    UPDATE time_entries SET is_billable = ?
    WHERE id = ? AND end_time IS NOT NULL AND invoice_id IS NULL AND is_invoiced = 0
  `)
  const tx = db.transaction(() => list.reduce((n, id) => n + stmt.run(billable ? 1 : 0, id).changes, 0))
  return tx()
}

/**
 * Entries that can go on an invoice for this client: everything unbilled,
 * plus (when editing) whatever is already on that invoice.
 */
export function listInvoiceableEntries(clientId: number, invoiceId?: number | null) {
  const today = localToday()
  const rows = db.prepare(`
    ${ENTRY_SELECT}
    WHERE p.client_id = @clientId AND te.end_time IS NOT NULL
      AND ((${UNBILLED_SQL}) OR (@invoiceId IS NOT NULL AND te.invoice_id = @invoiceId))
    ORDER BY te.start_time
  `).all({ clientId, invoiceId: invoiceId ?? null })
  return rows.map(r => withBillingState(r, today))
}

export function startTimer(projectId: number, description: string = '') {
  const now = new Date().toISOString()
  const stmt = db.prepare(`
    INSERT INTO time_entries (project_id, description, start_time, active_since, is_billable)
    VALUES (?, ?, ?, ?, 1)
  `)
  const result = stmt.run(projectId, description, now, now)
  return getTimeEntry(result.lastInsertRowid as number)
}

export function stopTimer(id: number) {
  const entry = db.prepare('SELECT * FROM time_entries WHERE id = ?').get(id) as any
  if (!entry) return null

  const now = new Date()
  const durationMinutes = getAccumulatedDurationMinutes(entry, now)

  // A timer that ran under a minute is almost always a mis-click or a quick
  // project switch. Rounding would turn it into billable time, so drop it.
  if (durationMinutes < 1) {
    const snapshot = getTimeEntry(id)
    db.prepare('DELETE FROM time_entries WHERE id = ?').run(id)
    return { ...snapshot, end_time: now.toISOString(), duration_minutes: 0, discarded: true }
  }

  // Get rounding preference
  const rounding = db.prepare("SELECT value FROM settings WHERE key = 'time_rounding'").get() as any
  const roundTo = rounding?.value || 'none'
  const roundedDuration = roundDuration(durationMinutes, roundTo)

  db.prepare(`
    UPDATE time_entries
    SET end_time = ?, duration_minutes = ?, paused_at = NULL, active_since = NULL
    WHERE id = ?
  `).run(now.toISOString(), roundedDuration, id)

  return getTimeEntry(id)
}

export function pauseTimer(id: number) {
  const entry = db.prepare('SELECT * FROM time_entries WHERE id = ?').get(id) as any
  if (!entry || entry.end_time || entry.paused_at) return entry ? getTimeEntry(id) : null

  const now = new Date()
  const durationMinutes = getAccumulatedDurationMinutes(entry, now)
  db.prepare(`
    UPDATE time_entries
    SET duration_minutes = ?, paused_at = ?, active_since = NULL
    WHERE id = ?
  `).run(durationMinutes, now.toISOString(), id)

  return getTimeEntry(id)
}

export function resumeTimer(id: number) {
  const entry = db.prepare('SELECT * FROM time_entries WHERE id = ?').get(id) as any
  if (!entry || entry.end_time || !entry.paused_at) return entry ? getTimeEntry(id) : null

  const now = new Date().toISOString()
  db.prepare(`
    UPDATE time_entries
    SET paused_at = NULL, active_since = ?
    WHERE id = ?
  `).run(now, id)

  return getTimeEntry(id)
}

export function getActiveTimer() {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NULL AND te.paused_at IS NULL
    ORDER BY te.start_time DESC
    LIMIT 1
  `).get() || null
}

export function getPausedTimer() {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NULL AND te.paused_at IS NOT NULL
    ORDER BY te.paused_at DESC
    LIMIT 1
  `).get() || null
}

function getAccumulatedDurationMinutes(entry: any, now: Date) {
  const accumulated = Number(entry.duration_minutes) || 0
  if (entry.paused_at) return accumulated

  const activeSince = new Date(entry.active_since || entry.start_time).getTime()
  const runningMinutes = Math.max(0, now.getTime() - activeSince) / 60000
  return accumulated + runningMinutes
}

function roundDuration(minutes: number, roundTo: string): number {
  if (roundTo === 'none') return Math.round(minutes * 100) / 100
  const increment = parseInt(roundTo)
  return Math.ceil(minutes / increment) * increment
}

// ============ Invoice Queries ============

const INVOICE_SELECT = `
  SELECT i.*, c.name AS client_name, c.company AS client_company,
         c.email AS client_email, c.address AS client_address,
         p.name AS project_name,
         (SELECT COUNT(*) FROM time_entries te WHERE te.invoice_id = i.id) AS entry_count,
         (SELECT COALESCE(SUM(te.duration_minutes), 0) / 60.0 FROM time_entries te WHERE te.invoice_id = i.id) AS entry_hours,
         (SELECT COUNT(DISTINCT it.project_id) FROM invoice_items it WHERE it.invoice_id = i.id AND it.project_id IS NOT NULL) AS project_count
  FROM invoices i
  LEFT JOIN clients c ON i.client_id = c.id
  LEFT JOIN projects p ON i.project_id = p.id
`

/** Replace the stored status with what the user should see (overdue is derived). */
function withInvoiceStatus(inv: any, today = localToday()) {
  if (!inv) return inv
  const status = effectiveInvoiceStatus(inv.status, inv.due_date, today)
  const due = inv.due_date ? String(inv.due_date).slice(0, 10) : null
  const daysPastDue = due
    ? Math.round((Date.parse(`${today}T00:00:00`) - Date.parse(`${due}T00:00:00`)) / 86_400_000)
    : 0
  return { ...inv, stored_status: inv.status, status, days_past_due: daysPastDue }
}

export function listInvoices(status?: string) {
  const today = localToday()
  const rows = (db.prepare(`${INVOICE_SELECT} ORDER BY i.issue_date DESC, i.id DESC`).all() as any[])
    .map(r => withInvoiceStatus(r, today))
  return status ? rows.filter(r => r.status === status) : rows
}

export function getInvoice(id: number) {
  const invoice = withInvoiceStatus(db.prepare(`${INVOICE_SELECT} WHERE i.id = ?`).get(id))
  if (invoice) {
    invoice.items = db.prepare('SELECT * FROM invoice_items WHERE invoice_id = ? ORDER BY id').all(id)
    invoice.entries = db.prepare(`
      SELECT te.*, p.name AS project_name, p.color AS project_color, p.rate
      FROM time_entries te LEFT JOIN projects p ON p.id = te.project_id
      WHERE te.invoice_id = ?
      ORDER BY te.start_time
    `).all(id)
  }
  return invoice
}

const INVOICE_COLUMNS = [
  'project_id', 'client_id', 'issue_date', 'due_date', 'status', 'subtotal', 'tax_rate', 'total',
  'notes', 'pdf_path', 'tax_year', 'payment_date', 'payment_method', 'currency',
  'gst_hst_applicable', 'gst_hst_number', 'gst_hst_rate', 'gst_hst_amount', 'sent_at', 'line_style',
]

function insertInvoiceItems(invoiceId: number, items: any[]) {
  const stmt = db.prepare(`
    INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total, project_id, source_key, custom_description)
    VALUES (@invoice_id, @description, @quantity, @unit_price, @total, @project_id, @source_key, @custom_description)
  `)
  for (const it of items || []) {
    stmt.run({
      invoice_id: invoiceId,
      description: String(it.description ?? ''),
      quantity: Number(it.quantity) || 0,
      unit_price: Number(it.unit_price) || 0,
      total: Number(it.total) || 0,
      project_id: it.project_id ? Number(it.project_id) : null,
      source_key: it.source_key || null,
      custom_description: it.custom_description ? 1 : 0,
    })
  }
}

/**
 * Make `ids` exactly the entries on this invoice. Entries that dropped off go
 * back to unbilled. Only unbilled entries (or ones already here) can be added;
 * anything else throws so the whole save rolls back instead of double billing.
 * Returns how many projects the linked time spans and, if one, which.
 */
function setInvoiceEntries(invoiceId: number, ids: number[]): { projectId: number | null; projectCount: number } {
  const list = Array.from(new Set((ids || []).map(Number).filter(Boolean)))
  const current = (db.prepare('SELECT id FROM time_entries WHERE invoice_id = ?').all(invoiceId) as any[]).map(r => r.id)
  const keep = new Set(list)
  const release = db.prepare('UPDATE time_entries SET invoice_id = NULL, is_invoiced = 0 WHERE id = ?')
  for (const id of current) if (!keep.has(id)) release.run(id)

  const link = db.prepare(`
    UPDATE time_entries SET invoice_id = @invoiceId, is_invoiced = 1
    WHERE id = @id AND end_time IS NOT NULL
      AND (invoice_id = @invoiceId OR (invoice_id IS NULL AND is_invoiced = 0))
  `)
  for (const id of list) {
    if (link.run({ invoiceId, id }).changes === 0) {
      throw new Error('Some of the selected time is already on another invoice or was deleted. Reopen the invoice and try again.')
    }
  }

  const projects = db.prepare('SELECT DISTINCT project_id FROM time_entries WHERE invoice_id = ?').all(invoiceId) as any[]
  return { projectId: projects.length === 1 ? projects[0].project_id : null, projectCount: projects.length }
}

/** Point the invoice at its project when all linked time shares one; leave it alone when nothing is linked. */
function syncInvoiceProject(invoiceId: number, linked: { projectId: number | null; projectCount: number }) {
  if (linked.projectCount === 0) return
  db.prepare('UPDATE invoices SET project_id = ? WHERE id = ?').run(linked.projectId, invoiceId)
}

function nextInvoiceNumber(): { number: string; next: number } {
  const settings = getSettings()
  const prefix = settings.invoice_prefix ?? 'INV-'
  let n = Number(settings.invoice_next_number) || 1001
  const taken = db.prepare('SELECT 1 FROM invoices WHERE invoice_number = ?')
  while (taken.get(`${prefix}${n}`)) n++
  return { number: `${prefix}${n}`, next: n + 1 }
}

export function createInvoice(data: any) {
  const { items, entry_ids, project_ids: _legacyProjectIds, ...inv } = data

  const tx = db.transaction(() => {
    const { number, next } = nextInvoiceNumber()
    const status = inv.status === 'sent' || inv.status === 'paid' ? inv.status : 'draft'
    const today = localToday()

    const result = db.prepare(`
      INSERT INTO invoices (
        project_id, client_id, invoice_number, issue_date, due_date, status,
        subtotal, tax_rate, total, notes, tax_year, currency,
        gst_hst_applicable, gst_hst_number, gst_hst_rate, gst_hst_amount,
        sent_at, line_style, payment_date, payment_method
      ) VALUES (
        @project_id, @client_id, @invoice_number, @issue_date, @due_date, @status,
        @subtotal, @tax_rate, @total, @notes, @tax_year, @currency,
        @gst_hst_applicable, @gst_hst_number, @gst_hst_rate, @gst_hst_amount,
        @sent_at, @line_style, @payment_date, @payment_method
      )
    `).run({
      project_id: inv.project_id ?? null,
      client_id: inv.client_id,
      invoice_number: number,
      issue_date: inv.issue_date,
      due_date: inv.due_date,
      status,
      subtotal: inv.subtotal || 0,
      tax_rate: inv.tax_rate || 0,
      total: inv.total || 0,
      notes: inv.notes || '',
      tax_year: inv.tax_year || yearOf(inv.issue_date),
      currency: inv.currency || 'CAD',
      gst_hst_applicable: inv.gst_hst_applicable ? 1 : 0,
      gst_hst_number: inv.gst_hst_number || null,
      gst_hst_rate: inv.gst_hst_rate || 0,
      gst_hst_amount: inv.gst_hst_amount || 0,
      sent_at: status === 'draft' ? null : today,
      line_style: inv.line_style || null,
      payment_date: status === 'paid' ? (inv.payment_date || today) : null,
      payment_method: status === 'paid' ? (inv.payment_method || null) : null,
    })
    const invoiceId = Number(result.lastInsertRowid)

    insertInvoiceItems(invoiceId, items || [])

    if (Array.isArray(entry_ids) && entry_ids.length) {
      syncInvoiceProject(invoiceId, setInvoiceEntries(invoiceId, entry_ids))
    }

    db.prepare("INSERT OR REPLACE INTO settings (key, value) VALUES ('invoice_next_number', ?)").run(String(next))
    return invoiceId
  })

  return getInvoice(tx())
}

export function updateInvoice(id: number, data: any) {
  const { items, entry_ids, ...inv } = data

  const tx = db.transaction(() => {
    const existing = db.prepare('SELECT * FROM invoices WHERE id = ?').get(id) as any
    if (!existing) return

    if (inv.issue_date && inv.tax_year === undefined) inv.tax_year = yearOf(inv.issue_date)
    if (inv.status === 'overdue') inv.status = 'sent' // derived from the due date now
    if (inv.status === 'paid' && inv.payment_date === undefined && !existing.payment_date) {
      inv.payment_date = localToday()
    }
    if (inv.status === 'sent' && !existing.sent_at && inv.sent_at === undefined) inv.sent_at = localToday()

    const fields = Object.keys(inv).filter(k => INVOICE_COLUMNS.includes(k))
    if (fields.length > 0) {
      const sets = fields.map(f => `${f} = @${f}`).join(', ')
      const params: any = { id }
      for (const f of fields) params[f] = inv[f] === undefined ? null : inv[f]
      db.prepare(`UPDATE invoices SET ${sets} WHERE id = @id`).run(params)
    }

    if (Array.isArray(items)) {
      db.prepare('DELETE FROM invoice_items WHERE invoice_id = ?').run(id)
      insertInvoiceItems(id, items)
    }

    if (Array.isArray(entry_ids)) {
      syncInvoiceProject(id, setInvoiceEntries(id, entry_ids))
    }
  })
  tx()

  return getInvoice(id)
}

/** Delete an invoice. Its time goes back to unbilled so it can be billed again. */
export function deleteInvoice(id: number) {
  const tx = db.transaction(() => {
    const r = db.prepare(`
      SELECT COUNT(*) AS n, COALESCE(SUM(duration_minutes), 0) AS mins
      FROM time_entries WHERE invoice_id = ?
    `).get(id) as { n: number; mins: number }
    db.prepare('UPDATE time_entries SET invoice_id = NULL, is_invoiced = 0 WHERE invoice_id = ?').run(id)
    db.prepare('DELETE FROM invoices WHERE id = ?').run(id)
    return { released_entries: r.n, released_hours: r.mins / 60 }
  })
  return tx()
}

export function markInvoicesSent(ids: number[]) {
  const today = localToday()
  const stmt = db.prepare(`
    UPDATE invoices SET status = 'sent', sent_at = COALESCE(sent_at, ?)
    WHERE id = ? AND status = 'draft'
  `)
  const tx = db.transaction(() => (ids || []).reduce((n, id) => n + stmt.run(today, Number(id)).changes, 0))
  return tx()
}

export function markInvoicesPaid(ids: number[], paymentDate?: string, paymentMethod?: string | null) {
  const date = paymentDate || localToday()
  const stmt = db.prepare(`
    UPDATE invoices
    SET status = 'paid', payment_date = @date, payment_method = @method,
        sent_at = COALESCE(sent_at, issue_date)
    WHERE id = @id AND status != 'paid'
  `)
  const tx = db.transaction(() =>
    (ids || []).reduce((n, id) => n + stmt.run({ id: Number(id), date, method: paymentMethod || null }).changes, 0))
  return tx()
}

/** Undo a payment that was recorded by mistake. */
export function markInvoiceUnpaid(id: number) {
  db.prepare(`
    UPDATE invoices SET status = 'sent', payment_date = NULL, payment_method = NULL,
      sent_at = COALESCE(sent_at, issue_date)
    WHERE id = ?
  `).run(id)
  return getInvoice(id)
}

// ============ Settings ============

export function getSettings(): any {
  const rows = db.prepare('SELECT key, value FROM settings').all() as any[]
  const settings: any = {}
  for (const row of rows) {
    if (['default_rate', 'invoice_next_number'].includes(row.key)) {
      settings[row.key] = parseFloat(row.value) || 0
    } else {
      settings[row.key] = row.value
    }
  }
  return settings
}

export function updateSettings(data: any) {
  const stmt = db.prepare('INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)')
  for (const [key, value] of Object.entries(data)) {
    stmt.run(key, String(value))
  }
  return getSettings()
}

// ============ Dashboard ============

export function getDashboardStats() {
  const now = new Date()
  // Weeks start on Monday, matching the Time page
  const startOfWeek = new Date(now)
  startOfWeek.setHours(0, 0, 0, 0)
  startOfWeek.setDate(startOfWeek.getDate() - ((startOfWeek.getDay() + 6) % 7))
  const startOfMonth = new Date(now.getFullYear(), now.getMonth(), 1)
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate())

  const hoursSince = db.prepare(`
    SELECT COALESCE(SUM(duration_minutes) / 60.0, 0) AS hours
    FROM time_entries WHERE end_time IS NOT NULL AND start_time >= ?
  `)

  const unbilled = db.prepare(`
    SELECT COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
           COALESCE(SUM(te.duration_minutes * p.rate / 60.0), 0) AS amount
    FROM time_entries te JOIN projects p ON p.id = te.project_id
    WHERE ${UNBILLED_SQL}
  `).get() as any

  const today = localToday()
  const invoices = db.prepare(`
    SELECT
      COALESCE(SUM(CASE WHEN status IN ('sent', 'overdue') THEN total END), 0) AS outstanding,
      COALESCE(SUM(CASE WHEN status IN ('sent', 'overdue') AND due_date < @today THEN total END), 0) AS overdue,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN total END), 0) AS paid,
      COALESCE(SUM(CASE WHEN status = 'paid' AND substr(COALESCE(payment_date, issue_date), 1, 4) = @year THEN total END), 0) AS paid_ytd
    FROM invoices
  `).get({ today, year: today.slice(0, 4) }) as any

  return {
    hours_today: (hoursSince.get(startOfToday.toISOString()) as any).hours,
    hours_this_week: (hoursSince.get(startOfWeek.toISOString()) as any).hours,
    hours_this_month: (hoursSince.get(startOfMonth.toISOString()) as any).hours,
    unbilled_hours: unbilled.hours,
    unbilled_amount: unbilled.amount,
    outstanding_total: invoices.outstanding,
    overdue_total: invoices.overdue,
    paid_total: invoices.paid,
    paid_ytd: invoices.paid_ytd,
  }
}

export function getRecentEntries(limit = 10) {
  const today = localToday()
  return (db.prepare(`${ENTRY_SELECT} WHERE te.end_time IS NOT NULL ORDER BY te.start_time DESC LIMIT ?`).all(limit) as any[])
    .map(r => withBillingState(r, today))
}

// ============ Reports ============
// Report ranges are local calendar dates, end date included.

export function hoursByProject(startDate: string, endDate: string) {
  const [from, to] = localRangeToUtc(startDate, endDate)
  return db.prepare(`
    SELECT p.id, p.name, p.color, c.name AS client_name,
      COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 1 THEN te.duration_minutes * p.rate / 60.0 END), 0) AS value
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON c.id = p.client_id
    WHERE te.end_time IS NOT NULL AND te.start_time >= ? AND te.start_time < ?
    GROUP BY p.id
    ORDER BY hours DESC
  `).all(from, to)
}

export function hoursByClient(startDate: string, endDate: string) {
  const [from, to] = localRangeToUtc(startDate, endDate)
  return db.prepare(`
    SELECT c.id, c.name,
      COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 1 THEN te.duration_minutes * p.rate / 60.0 END), 0) AS value
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NOT NULL AND te.start_time >= ? AND te.start_time < ?
    GROUP BY c.id
    ORDER BY hours DESC
  `).all(from, to)
}

/** Invoiced by issue month and paid by payment month. Drafts are left out. */
export function earningsByMonth(startDate: string, endDate: string) {
  return db.prepare(`
    SELECT month, COALESCE(SUM(paid), 0) AS paid, COALESCE(SUM(invoiced), 0) AS invoiced
    FROM (
      SELECT substr(issue_date, 1, 7) AS month, 0 AS paid, total AS invoiced
      FROM invoices WHERE status != 'draft' AND issue_date >= @s AND issue_date <= @e
      UNION ALL
      SELECT substr(COALESCE(payment_date, issue_date), 1, 7) AS month, total AS paid, 0 AS invoiced
      FROM invoices WHERE status = 'paid'
        AND COALESCE(payment_date, issue_date) >= @s AND COALESCE(payment_date, issue_date) <= @e
    )
    GROUP BY month
    ORDER BY month
  `).all({ s: startDate, e: endDate })
}

export function getUnbilledEntries(projectId: number) {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE te.project_id = ? AND ${UNBILLED_SQL}
    ORDER BY te.start_time
  `).all(projectId)
}

export function getTodayHours(): number {
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const result = db.prepare(`
    SELECT COALESCE(SUM(duration_minutes) / 60.0, 0) as hours
    FROM time_entries
    WHERE end_time IS NOT NULL AND start_time >= ?
  `).get(today.toISOString()) as any
  return result.hours
}

export function getUnbilledTotals(): { hours: number; amount: number } {
  return db.prepare(`
    SELECT COALESCE(SUM(te.duration_minutes) / 60.0, 0) AS hours,
           COALESCE(SUM(te.duration_minutes * p.rate / 60.0), 0) AS amount
    FROM time_entries te JOIN projects p ON p.id = te.project_id
    WHERE ${UNBILLED_SQL}
  `).get() as any
}

export function getRecentProjects(limit: number = 5): any[] {
  return db.prepare(`
    SELECT DISTINCT p.id, p.name, p.client_id, p.color, p.status, p.rate,
           c.name as client_name,
           MAX(te.start_time) as last_used
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE p.status = 'active'
    GROUP BY p.id
    ORDER BY last_used DESC
    LIMIT ?
  `).all(limit)
}

export function getUnbilledEntriesByClient(clientId: number) {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE p.client_id = ? AND ${UNBILLED_SQL}
    ORDER BY te.start_time
  `).all(clientId)
}

export function getUnbilledEntriesForProjects(projectIds: number[]) {
  if (projectIds.length === 0) return []
  const placeholders = projectIds.map(() => '?').join(',')
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE te.project_id IN (${placeholders}) AND ${UNBILLED_SQL}
    ORDER BY p.name, te.start_time
  `).all(...projectIds)
}

// ============ Billing overview ============

const STALE_UNBILLED_DAYS = 30
const STALE_DRAFT_DAYS = 3

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** "Jun 12", or "Jun 12, 2025" outside the current year. */
function friendlyDay(ymd: string, today = localToday()): string {
  const [y, m, d] = String(ymd).slice(0, 10).split('-').map(Number)
  if (!y || !m || !d) return String(ymd)
  const base = `${MONTHS[m - 1]} ${d}`
  return String(y) === today.slice(0, 4) ? base : `${base}, ${y}`
}

function daysBetween(fromDay: string, toDay: string) {
  return Math.round((Date.parse(`${toDay}T00:00:00`) - Date.parse(`${fromDay}T00:00:00`)) / 86_400_000)
}

const NAME_NOISE = new Set([
  'the', 'inc', 'incorporated', 'ltd', 'limited', 'llc', 'llp', 'co', 'corp', 'corporation',
  'company', 'society', 'association', 'group', 'foundation', 'and',
])

function normalizeName(name: string): string {
  return String(name || '')
    .toLowerCase()
    .replace(/&/g, ' and ')
    .replace(/[^a-z0-9 ]+/g, ' ')
    .split(/\s+/)
    .filter(w => w && !NAME_NOISE.has(w))
    .join(' ')
}

function getDismissed(): Set<string> {
  try {
    const row = db.prepare("SELECT value FROM settings WHERE key = 'dismissed_attention'").get() as any
    return new Set(JSON.parse(row?.value || '[]'))
  } catch {
    return new Set()
  }
}

export function dismissAttention(key: string) {
  const set = getDismissed()
  set.add(key)
  db.prepare("INSERT OR REPLACE INTO settings (key, value) VALUES ('dismissed_attention', ?)")
    .run(JSON.stringify(Array.from(set)))
  return true
}

/** Pairs of clients that look like the same organization. */
function findDuplicateClients(dismissed: Set<string>) {
  const clients = db.prepare('SELECT id, name, company FROM clients').all() as any[]
  const pairs: Array<{ a: any; b: any; key: string }> = []
  for (let i = 0; i < clients.length; i++) {
    for (let j = i + 1; j < clients.length; j++) {
      const a = clients[i]
      const b = clients[j]
      const na = normalizeName(a.name)
      const nb = normalizeName(b.name)
      const ca = normalizeName(a.company)
      const cb = normalizeName(b.company)
      const same = (na && na === nb) || (ca && ca === nb) || (cb && cb === na) || (ca && cb && ca === cb)
      if (!same) continue
      const key = `dup:${Math.min(a.id, b.id)}-${Math.max(a.id, b.id)}`
      if (!dismissed.has(key)) pairs.push({ a, b, key })
    }
  }
  return pairs
}

/**
 * Everything the Billing page and Dashboard need: money in each stage,
 * unbilled work grouped by client, and a short list of things to act on.
 */
export function getBillingOverview() {
  const today = localToday()
  const year = today.slice(0, 4)

  const unbilledProjects = db.prepare(`
    SELECT p.id AS project_id, p.name AS project_name, p.color AS project_color,
           p.status AS project_status, p.rate, c.id AS client_id, c.name AS client_name,
           COUNT(te.id) AS entry_count,
           SUM(te.duration_minutes) / 60.0 AS hours,
           SUM(te.duration_minutes * p.rate / 60.0) AS amount,
           MIN(te.start_time) AS oldest, MAX(te.start_time) AS newest
    FROM time_entries te
    JOIN projects p ON p.id = te.project_id
    JOIN clients c ON c.id = p.client_id
    WHERE ${UNBILLED_SQL}
    GROUP BY p.id
    ORDER BY c.name COLLATE NOCASE, oldest
  `).all() as any[]

  // Group ready-to-bill work by client
  const byClient = new Map<number, any>()
  for (const row of unbilledProjects) {
    row.oldest_day = localDate(new Date(row.oldest))
    row.age_days = daysBetween(row.oldest_day, today)
    let group = byClient.get(row.client_id)
    if (!group) {
      group = { client_id: row.client_id, client_name: row.client_name, amount: 0, hours: 0, entry_count: 0, oldest_day: row.oldest_day, projects: [] }
      byClient.set(row.client_id, group)
    }
    group.amount += row.amount
    group.hours += row.hours
    group.entry_count += row.entry_count
    if (row.oldest_day < group.oldest_day) group.oldest_day = row.oldest_day
    group.projects.push(row)
  }
  const readyToBill = Array.from(byClient.values()).sort((a, b) => b.amount - a.amount)

  const invoices = listInvoices() as any[]
  const sum = (list: any[]) => list.reduce((s, i) => s + (Number(i.total) || 0), 0)
  const drafts = invoices.filter(i => i.status === 'draft')
  const awaiting = invoices.filter(i => i.status === 'sent')
  const overdue = invoices.filter(i => i.status === 'overdue')
  const paidThisYear = invoices.filter(i => i.status === 'paid' && String(i.payment_date || i.issue_date).startsWith(year))

  const pipeline = {
    unbilled_amount: readyToBill.reduce((s, g) => s + g.amount, 0),
    unbilled_hours: readyToBill.reduce((s, g) => s + g.hours, 0),
    unbilled_entries: readyToBill.reduce((s, g) => s + g.entry_count, 0),
    unbilled_projects: unbilledProjects.length,
    draft_amount: sum(drafts),
    draft_count: drafts.length,
    awaiting_amount: sum(awaiting),
    awaiting_count: awaiting.length,
    overdue_amount: sum(overdue),
    overdue_count: overdue.length,
    paid_ytd_amount: sum(paidThisYear),
    paid_ytd_count: paidThisYear.length,
  }

  // ---- Needs attention ----
  const attention: any[] = []
  const dismissed = getDismissed()

  for (const inv of overdue) {
    attention.push({
      key: `overdue:${inv.id}`,
      kind: 'overdue',
      tone: 'danger',
      title: `${inv.invoice_number} is overdue`,
      detail: `${inv.client_name || 'Client'} · was due ${friendlyDay(inv.due_date, today)} · ${inv.days_past_due} day${inv.days_past_due === 1 ? '' : 's'} late`,
      amount: inv.total,
      invoice_id: inv.id,
    })
  }

  for (const row of unbilledProjects) {
    const closed = row.project_status === 'complete' || row.project_status === 'archived'
    const stale = row.age_days >= STALE_UNBILLED_DAYS
    if (!closed && !stale) continue
    attention.push({
      key: `unbilled:${row.project_id}`,
      kind: 'unbilled',
      tone: 'warning',
      title: row.project_name,
      detail: `${row.client_name} · unbilled since ${friendlyDay(row.oldest_day, today)}${closed ? ` · project is marked ${row.project_status}` : ''}`,
      amount: row.amount,
      project_id: row.project_id,
      client_id: row.client_id,
      closed,
      oldest_day: row.oldest_day,
    })
  }

  for (const inv of drafts) {
    const createdMs = sqliteTimeMs(inv.created_at)
    const created = createdMs ? localDate(new Date(createdMs)) : String(inv.issue_date).slice(0, 10)
    const age = daysBetween(created, today)
    if (age < STALE_DRAFT_DAYS) continue
    attention.push({
      key: `draft:${inv.id}`,
      kind: 'draft',
      tone: 'neutral',
      title: `${inv.invoice_number} is still a draft`,
      detail: `${inv.client_name || 'Client'} · created ${age} days ago · not marked as sent`,
      amount: inv.total,
      invoice_id: inv.id,
    })
  }

  const accidental = db.prepare(`
    SELECT te.id, te.start_time, te.end_time, te.duration_minutes, p.name AS project_name, p.rate,
           (julianday(te.end_time) - julianday(te.start_time)) * 86400.0 AS wall_seconds
    FROM time_entries te JOIN projects p ON p.id = te.project_id
    WHERE ${UNBILLED_SQL}
      AND (julianday(te.end_time) - julianday(te.start_time)) * 86400.0 < 60
      AND te.duration_minutes <= 30
  `).all() as any[]
  for (const e of accidental) {
    const secs = Math.max(1, Math.round(e.wall_seconds))
    attention.push({
      key: `timer:${e.id}`,
      kind: 'accidental_timer',
      tone: 'neutral',
      title: e.project_name,
      detail: `Timer ran ${secs} second${secs === 1 ? '' : 's'} on ${friendlyDay(localDate(new Date(e.start_time)), today)} and was rounded up to ${Math.round(e.duration_minutes)}m`,
      amount: (e.duration_minutes / 60) * e.rate,
      entry_id: e.id,
    })
  }

  for (const pair of findDuplicateClients(dismissed)) {
    attention.push({
      key: pair.key,
      kind: 'duplicate_clients',
      tone: 'neutral',
      title: `${pair.a.name} and ${pair.b.name}`,
      detail: 'These look like the same client',
      client_ids: [pair.a.id, pair.b.id],
      client_names: [pair.a.name, pair.b.name],
    })
  }

  return { today, pipeline, ready_to_bill: readyToBill, attention: attention.filter(a => !dismissed.has(a.key)), invoices }
}

// ============ Tax Overview ============

export interface TaxOverviewBucket {
  category: string
  total: number
  count: number
}

export interface MonthlyIncomeRow {
  month: string // 'YYYY-MM'
  paid: number
  invoiced: number
}

export function getTaxOverview(taxYear: number) {
  // Income aggregates — uses tax_year column (Phase 1 backfilled this)
  const income = db.prepare(`
    SELECT
      COALESCE(SUM(total), 0) as total_invoiced,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN total ELSE 0 END), 0) as total_paid,
      COALESCE(SUM(CASE WHEN status IN ('sent', 'overdue') THEN total ELSE 0 END), 0) as total_outstanding,
      COALESCE(SUM(CASE WHEN status = 'paid' AND gst_hst_applicable = 1 THEN gst_hst_amount ELSE 0 END), 0) as gst_collected_paid,
      COALESCE(SUM(CASE WHEN gst_hst_applicable = 1 THEN gst_hst_amount ELSE 0 END), 0) as gst_collected_total,
      COUNT(*) as invoice_count,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN 1 ELSE 0 END), 0) as paid_count
    FROM invoices
    WHERE tax_year = ?
  `).get(taxYear) as any

  // Expenses by category for the same year
  const expensesByCategory = db.prepare(`
    SELECT category, COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM expenses
    WHERE tax_year = ?
    GROUP BY category
    ORDER BY total DESC
  `).all(taxYear) as TaxOverviewBucket[]

  const totalExpenses = expensesByCategory.reduce((s, r) => s + r.total, 0)

  // Monthly income: paid invoices keyed by payment_date (fall back to issue_date if missing).
  // We also track invoiced amount by issue month for context.
  const monthly = db.prepare(`
    SELECT
      strftime('%Y-%m', COALESCE(payment_date, issue_date)) as month,
      COALESCE(SUM(CASE WHEN status = 'paid' THEN total ELSE 0 END), 0) as paid,
      COALESCE(SUM(total), 0) as invoiced
    FROM invoices
    WHERE tax_year = ?
    GROUP BY month
    ORDER BY month
  `).all(taxYear) as MonthlyIncomeRow[]

  return {
    tax_year: taxYear,
    total_invoiced: income.total_invoiced,
    total_paid: income.total_paid,
    total_outstanding: income.total_outstanding,
    gst_collected_paid: income.gst_collected_paid,
    gst_collected_total: income.gst_collected_total,
    invoice_count: income.invoice_count,
    paid_count: income.paid_count,
    expenses_by_category: expensesByCategory,
    total_expenses: totalExpenses,
    monthly_income: monthly,
  }
}

export function listInvoicesByYear(taxYear: number) {
  return db.prepare(`
    SELECT i.*, c.name as client_name, c.company as client_company
    FROM invoices i
    LEFT JOIN clients c ON i.client_id = c.id
    WHERE i.tax_year = ?
    ORDER BY i.issue_date
  `).all(taxYear)
}

// ============ Tax Settings ============

export function getTaxSettings() {
  return db.prepare('SELECT * FROM tax_settings WHERE id = 1').get() as any
}

export function saveTaxSettings(data: any) {
  const existing = getTaxSettings()
  const merged = { ...existing, ...data, id: 1, updated_at: new Date().toISOString() }
  // Restrict to known columns
  const fields = [
    'business_name', 'business_address', 'gst_hst_number', 'gst_hst_registered',
    'province', 'fiscal_year_start', 'default_tax_rate', 'income_tax_bracket',
    'currency', 'updated_at',
  ] as const

  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  const payload: any = { id: 1 }
  for (const f of fields) payload[f] = merged[f]

  db.prepare(`UPDATE tax_settings SET ${sets} WHERE id = @id`).run(payload)
  return getTaxSettings()
}

// ============ Expenses ============

export function listExpenses(taxYear?: number) {
  if (taxYear) {
    return db.prepare(`
      SELECT * FROM expenses WHERE tax_year = ? ORDER BY date DESC, id DESC
    `).all(taxYear)
  }
  return db.prepare('SELECT * FROM expenses ORDER BY date DESC, id DESC').all()
}

export function getExpense(id: number) {
  return db.prepare('SELECT * FROM expenses WHERE id = ?').get(id)
}

export function createExpense(data: any) {
  const date = data.date || localToday()
  const taxYear = data.tax_year || yearOf(date)
  const stmt = db.prepare(`
    INSERT INTO expenses (date, category, description, amount, tax_year, receipt_note, receipt_id)
    VALUES (@date, @category, @description, @amount, @tax_year, @receipt_note, @receipt_id)
  `)
  const result = stmt.run({
    date,
    category: data.category || 'other',
    description: data.description || '',
    amount: data.amount || 0,
    tax_year: taxYear,
    receipt_note: data.receipt_note || '',
    receipt_id: data.receipt_id || null,
  })
  return getExpense(result.lastInsertRowid as number)
}

export function updateExpense(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  if (fields.length === 0) return getExpense(id)
  // Re-derive tax_year if date changed
  if (data.date && !data.tax_year) {
    data.tax_year = yearOf(data.date)
    fields.push('tax_year')
  }
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE expenses SET ${sets} WHERE id = @id`).run({ ...data, id })
  return getExpense(id)
}

export function deleteExpense(id: number) {
  db.prepare('DELETE FROM expenses WHERE id = ?').run(id)
}

// ============ Commission Tracking ============

/**
 * Authoritative commission calculation. Runs on every create/update so the
 * stored number always matches the rules.
 *
 *  Solar:   kW × $50
 *  Roofing: ≤ $20,000 → $250 ; ≥ $30,000 → $500 ;
 *           $20,001–$29,999 → Needs Review (commission 0) unless a manual
 *           override is provided.
 *
 * Returns the rule-based `calculated` amount and a `needs_review` flag. A manual
 * override (when present and ≥ 0) takes precedence in display/totals and clears
 * the review flag.
 */
export function computeCommission(data: any): { calculated: number; needs_review: number } {
  const overrideRaw = data.manual_override
  const hasOverride = overrideRaw !== null && overrideRaw !== undefined && overrideRaw !== '' &&
    !isNaN(Number(overrideRaw)) && Number(overrideRaw) >= 0

  if (data.job_type === 'roofing') {
    const amt = Number(data.contract_amount) || 0
    if (amt <= 20000) return { calculated: 250, needs_review: 0 }
    if (amt >= 30000) return { calculated: 500, needs_review: 0 }
    // $20,001–$29,999 gap
    return { calculated: 0, needs_review: hasOverride ? 0 : 1 }
  }

  // Solar (default)
  const kw = Number(data.system_size_kw) || 0
  const amount = Math.max(0, Math.round(kw * 50 * 100) / 100)
  return { calculated: amount, needs_review: 0 }
}

export function listCommissions() {
  return db.prepare('SELECT * FROM commissions ORDER BY appointment_date DESC, id DESC').all()
}

export function getCommission(id: number) {
  return db.prepare('SELECT * FROM commissions WHERE id = ?').get(id)
}

export function createCommission(data: any) {
  const { calculated, needs_review } = computeCommission(data)
  const override = data.manual_override === '' || data.manual_override === null || data.manual_override === undefined
    ? null
    : Math.max(0, Number(data.manual_override))

  const stmt = db.prepare(`
    INSERT INTO commissions (
      client_name, job_type, appointment_date, closer_name, status, payment_status,
      system_size_kw, contract_amount, calculated_commission, manual_override, needs_review, notes
    ) VALUES (
      @client_name, @job_type, @appointment_date, @closer_name, @status, @payment_status,
      @system_size_kw, @contract_amount, @calculated_commission, @manual_override, @needs_review, @notes
    )
  `)
  const result = stmt.run({
    client_name: data.client_name,
    job_type: data.job_type || 'solar',
    appointment_date: data.appointment_date || null,
    closer_name: data.closer_name || '',
    status: data.status || 'appointment_set',
    payment_status: data.payment_status || 'unpaid',
    system_size_kw: data.system_size_kw != null && data.system_size_kw !== '' ? Number(data.system_size_kw) : null,
    contract_amount: data.contract_amount != null && data.contract_amount !== '' ? Number(data.contract_amount) : null,
    calculated_commission: calculated,
    manual_override: override,
    needs_review,
    notes: data.notes || '',
  })
  return getCommission(result.lastInsertRowid as number)
}

export function updateCommission(id: number, data: any) {
  const existing = getCommission(id) as any
  if (!existing) return null

  // Merge so the recalculation sees the full picture even on partial updates
  const merged = { ...existing, ...data }
  const { calculated, needs_review } = computeCommission(merged)
  const override = merged.manual_override === '' || merged.manual_override === null || merged.manual_override === undefined
    ? null
    : Math.max(0, Number(merged.manual_override))

  db.prepare(`
    UPDATE commissions SET
      client_name = @client_name,
      job_type = @job_type,
      appointment_date = @appointment_date,
      closer_name = @closer_name,
      status = @status,
      payment_status = @payment_status,
      system_size_kw = @system_size_kw,
      contract_amount = @contract_amount,
      calculated_commission = @calculated_commission,
      manual_override = @manual_override,
      needs_review = @needs_review,
      notes = @notes,
      updated_at = datetime('now')
    WHERE id = @id
  `).run({
    id,
    client_name: merged.client_name,
    job_type: merged.job_type,
    appointment_date: merged.appointment_date || null,
    closer_name: merged.closer_name || '',
    status: merged.status,
    payment_status: merged.payment_status,
    system_size_kw: merged.system_size_kw != null && merged.system_size_kw !== '' ? Number(merged.system_size_kw) : null,
    contract_amount: merged.contract_amount != null && merged.contract_amount !== '' ? Number(merged.contract_amount) : null,
    calculated_commission: calculated,
    manual_override: override,
    needs_review,
    notes: merged.notes || '',
  })
  return getCommission(id)
}

export function deleteCommission(id: number) {
  db.prepare('DELETE FROM commissions WHERE id = ?').run(id)
}

/** Effective payout for totals: override wins; review gap → 0. */
function commissionEffective(c: any): number {
  if (c.manual_override != null) return c.manual_override
  if (c.needs_review) return 0
  return c.calculated_commission || 0
}

const COMMISSION_PATCH_COLS = ['status', 'payment_status', 'invoice_status', 'invoiced_at', 'paid_at', 'invoice_id']

/**
 * Lightweight status patch for quick / bulk actions — updates only allowlisted
 * lifecycle columns and does NOT recompute the commission amount (kW/contract
 * are unchanged). Full edits still go through updateCommission.
 */
export function patchCommission(id: number, patch: any) {
  const fields = Object.keys(patch).filter(k => COMMISSION_PATCH_COLS.includes(k))
  if (fields.length === 0) return getCommission(id)
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE commissions SET ${sets}, updated_at = datetime('now') WHERE id = @id`).run({ ...patch, id })
  return getCommission(id)
}

export function bulkPatchCommissions(ids: number[], patch: any) {
  const tx = db.transaction((list: number[]) => {
    for (const id of list) patchCommission(id, patch)
  })
  tx(ids)
  return listCommissions()
}

// ============ Commission Invoices ============

export function listCommissionInvoices() {
  return db.prepare('SELECT * FROM commission_invoices ORDER BY created_at DESC, id DESC').all()
}

export function getCommissionInvoice(id: number) {
  const inv = db.prepare('SELECT * FROM commission_invoices WHERE id = ?').get(id) as any
  if (inv) {
    inv.jobs = db.prepare('SELECT * FROM commissions WHERE invoice_id = ? ORDER BY job_type, appointment_date').all(id)
  }
  return inv
}

/**
 * Create a commission invoice from a set of job ids. Stamps the jobs as
 * "invoiced" and links them to the new invoice.
 */
export function createCommissionInvoice(data: { jobIds: number[]; category?: string; date_from?: string; date_to?: string; notes?: string }) {
  const jobs = (data.jobIds || []).map(id => getCommission(id)).filter(Boolean) as any[]
  if (jobs.length === 0) throw new Error('No jobs selected for the invoice')

  const total = jobs.reduce((s, c) => s + commissionEffective(c), 0)
  const types = new Set(jobs.map(j => j.job_type))
  const category = data.category && data.category !== 'both'
    ? data.category
    : (types.size > 1 ? 'mixed' : (types.has('solar') ? 'solar' : 'roofing'))

  const tx = db.transaction(() => {
    const result = db.prepare(`
      INSERT INTO commission_invoices (invoice_number, category, date_from, date_to, status, job_count, total, notes)
      VALUES ('PENDING', @category, @date_from, @date_to, 'draft', @job_count, @total, @notes)
    `).run({
      category,
      date_from: data.date_from || null,
      date_to: data.date_to || null,
      job_count: jobs.length,
      total,
      notes: data.notes || '',
    })
    const id = Number(result.lastInsertRowid)
    const number = `COMM-${String(1000 + id).padStart(4, '0')}`
    db.prepare('UPDATE commission_invoices SET invoice_number = ? WHERE id = ?').run(number, id)

    const now = localToday()
    const upd = db.prepare(`
      UPDATE commissions SET invoice_id = ?, invoice_status = 'invoiced', invoiced_at = ?, updated_at = datetime('now')
      WHERE id = ?
    `)
    for (const j of jobs) upd.run(id, now, j.id)
    return id
  })
  const newId = tx()
  return getCommissionInvoice(newId)
}

export function updateCommissionInvoiceStatus(id: number, status: string) {
  if (status === 'paid') {
    const now = localToday()
    const tx = db.transaction(() => {
      db.prepare("UPDATE commission_invoices SET status = 'paid', paid_at = ?, updated_at = datetime('now') WHERE id = ?").run(now, id)
      // Cascade: every job on this invoice is now paid out
      db.prepare(`
        UPDATE commissions SET payment_status = 'paid', status = 'paid', invoice_status = 'paid', paid_at = ?, updated_at = datetime('now')
        WHERE invoice_id = ?
      `).run(now, id)
    })
    tx()
  } else {
    db.prepare("UPDATE commission_invoices SET status = ?, updated_at = datetime('now') WHERE id = ?").run(status, id)
  }
  return getCommissionInvoice(id)
}

/** Delete an invoice and release its (unpaid) jobs back to "not invoiced". */
export function deleteCommissionInvoice(id: number) {
  const tx = db.transaction(() => {
    db.prepare(`
      UPDATE commissions SET invoice_id = NULL, invoice_status = 'not_invoiced', invoiced_at = NULL, updated_at = datetime('now')
      WHERE invoice_id = ? AND payment_status != 'paid'
    `).run(id)
    // Paid jobs keep their paid state but lose the (now-deleted) invoice link
    db.prepare('UPDATE commissions SET invoice_id = NULL WHERE invoice_id = ?').run(id)
    db.prepare('DELETE FROM commission_invoices WHERE id = ?').run(id)
  })
  tx()
}

export { db }
