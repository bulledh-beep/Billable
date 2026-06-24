import Database from 'better-sqlite3'
import { getProfileDbPath, getActiveProfileId } from './profiles'

let db: Database.Database

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

export function listClients() {
  return db.prepare('SELECT * FROM clients ORDER BY name').all()
}

export function getClient(id: number) {
  return db.prepare('SELECT * FROM clients WHERE id = ?').get(id)
}

export function createClient(data: any) {
  const stmt = db.prepare(`
    INSERT INTO clients (name, company, email, address, default_rate, currency)
    VALUES (@name, @company, @email, @address, @default_rate, @currency)
  `)
  const result = stmt.run(data)
  return db.prepare('SELECT * FROM clients WHERE id = ?').get(result.lastInsertRowid)
}

export function updateClient(id: number, data: any) {
  const existing = db.prepare('SELECT * FROM clients WHERE id = ?').get(id) as any
  if (!existing) return null

  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at' && k !== 'cascaded_projects')
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

  const client = db.prepare('SELECT * FROM clients WHERE id = ?').get(id) as any
  return { ...client, cascaded_projects: cascadedProjects }
}

export function deleteClient(id: number) {
  db.prepare('DELETE FROM clients WHERE id = ?').run(id)
}

// ============ Project Queries ============

export function listProjects(clientId?: number) {
  const query = `
    SELECT p.*, c.name as client_name, c.company as client_company,
      COALESCE(SUM(CASE WHEN te.end_time IS NOT NULL THEN te.duration_minutes ELSE 0 END) / 60.0, 0) as total_hours,
      COALESCE(SUM(CASE WHEN te.is_invoiced = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes * p.rate / 60.0 ELSE 0 END), 0) as billed_total,
      COALESCE(SUM(CASE WHEN te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes / 60.0 ELSE 0 END), 0) as unbilled_hours
    FROM projects p
    LEFT JOIN clients c ON p.client_id = c.id
    LEFT JOIN time_entries te ON te.project_id = p.id
    ${clientId ? 'WHERE p.client_id = ?' : ''}
    GROUP BY p.id
    ORDER BY p.created_at DESC
  `
  return clientId ? db.prepare(query).all(clientId) : db.prepare(query).all()
}

export function getProject(id: number) {
  const query = `
    SELECT p.*, c.name as client_name, c.company as client_company,
      COALESCE(SUM(CASE WHEN te.end_time IS NOT NULL THEN te.duration_minutes ELSE 0 END) / 60.0, 0) as total_hours,
      COALESCE(SUM(CASE WHEN te.is_invoiced = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes * p.rate / 60.0 ELSE 0 END), 0) as billed_total,
      COALESCE(SUM(CASE WHEN te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes / 60.0 ELSE 0 END), 0) as unbilled_hours
    FROM projects p
    LEFT JOIN clients c ON p.client_id = c.id
    LEFT JOIN time_entries te ON te.project_id = p.id
    WHERE p.id = ?
    GROUP BY p.id
  `
  return db.prepare(query).get(id)
}

export function createProject(data: any) {
  const stmt = db.prepare(`
    INSERT INTO projects (client_id, name, description, rate, status, color)
    VALUES (@client_id, @name, @description, @rate, @status, @color)
  `)
  const result = stmt.run(data)
  return getProject(result.lastInsertRowid as number)
}

export function updateProject(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE projects SET ${sets} WHERE id = @id`).run({ ...data, id })
  return getProject(id)
}

export function deleteProject(id: number) {
  db.prepare('DELETE FROM projects WHERE id = ?').run(id)
}

// ============ Time Entry Queries ============

export function listTimeEntries(projectId?: number) {
  const query = `
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    ${projectId ? 'WHERE te.project_id = ?' : ''}
    ORDER BY te.start_time DESC
  `
  return projectId ? db.prepare(query).all(projectId) : db.prepare(query).all()
}

export function getTimeEntry(id: number) {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE te.id = ?
  `).get(id)
}

export function createTimeEntry(data: any) {
  const stmt = db.prepare(`
    INSERT INTO time_entries (project_id, description, start_time, end_time, duration_minutes, is_billable, is_invoiced)
    VALUES (@project_id, @description, @start_time, @end_time, @duration_minutes, @is_billable, @is_invoiced)
  `)
  const result = stmt.run({
    project_id: data.project_id,
    description: data.description || '',
    start_time: data.start_time,
    end_time: data.end_time || null,
    duration_minutes: data.duration_minutes || 0,
    is_billable: data.is_billable ?? 1,
    is_invoiced: data.is_invoiced ?? 0,
  })
  return getTimeEntry(result.lastInsertRowid as number)
}

export function updateTimeEntry(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  if (sets) {
    db.prepare(`UPDATE time_entries SET ${sets} WHERE id = @id`).run({ ...data, id })
  }
  return getTimeEntry(id)
}

export function deleteTimeEntry(id: number) {
  db.prepare('DELETE FROM time_entries WHERE id = ?').run(id)
}

export function startTimer(projectId: number, description: string = '') {
  const now = new Date().toISOString()
  const stmt = db.prepare(`
    INSERT INTO time_entries (project_id, description, start_time, is_billable)
    VALUES (?, ?, ?, 1)
  `)
  const result = stmt.run(projectId, description, now)
  return getTimeEntry(result.lastInsertRowid as number)
}

export function stopTimer(id: number) {
  const entry = db.prepare('SELECT * FROM time_entries WHERE id = ?').get(id) as any
  if (!entry) return null

  const now = new Date()
  const start = new Date(entry.start_time)
  const durationMinutes = (now.getTime() - start.getTime()) / 60000

  // Get rounding preference
  const rounding = db.prepare("SELECT value FROM settings WHERE key = 'time_rounding'").get() as any
  const roundTo = rounding?.value || 'none'
  const roundedDuration = roundDuration(durationMinutes, roundTo)

  db.prepare(`
    UPDATE time_entries SET end_time = ?, duration_minutes = ? WHERE id = ?
  `).run(now.toISOString(), roundedDuration, id)

  return getTimeEntry(id)
}

export function getActiveTimer() {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NULL
    ORDER BY te.start_time DESC
    LIMIT 1
  `).get() || null
}

function roundDuration(minutes: number, roundTo: string): number {
  if (roundTo === 'none') return Math.round(minutes * 100) / 100
  const increment = parseInt(roundTo)
  return Math.ceil(minutes / increment) * increment
}

// ============ Invoice Queries ============

export function listInvoices(status?: string) {
  const query = `
    SELECT i.*, c.name as client_name, c.company as client_company,
           c.email as client_email, c.address as client_address,
           p.name as project_name
    FROM invoices i
    LEFT JOIN clients c ON i.client_id = c.id
    LEFT JOIN projects p ON i.project_id = p.id
    ${status ? "WHERE i.status = ?" : ''}
    ORDER BY i.created_at DESC
  `
  return status ? db.prepare(query).all(status) : db.prepare(query).all()
}

export function getInvoice(id: number) {
  const invoice = db.prepare(`
    SELECT i.*, c.name as client_name, c.company as client_company,
           c.email as client_email, c.address as client_address,
           p.name as project_name
    FROM invoices i
    LEFT JOIN clients c ON i.client_id = c.id
    LEFT JOIN projects p ON i.project_id = p.id
    WHERE i.id = ?
  `).get(id) as any

  if (invoice) {
    invoice.items = db.prepare('SELECT * FROM invoice_items WHERE invoice_id = ? ORDER BY id').all(id)
  }

  return invoice
}

export function createInvoice(data: any) {
  const { items, ...invoiceData } = data

  // Get next invoice number
  const settings = getSettings()
  const invoiceNumber = `${settings.invoice_prefix}${settings.invoice_next_number}`

  // Derive tax_year from issue_date if not provided
  const taxYear = invoiceData.tax_year ||
    (invoiceData.issue_date ? new Date(invoiceData.issue_date).getFullYear() : new Date().getFullYear())

  const stmt = db.prepare(`
    INSERT INTO invoices (
      project_id, client_id, invoice_number, issue_date, due_date, status,
      subtotal, tax_rate, total, notes,
      tax_year, currency,
      gst_hst_applicable, gst_hst_number, gst_hst_rate, gst_hst_amount
    )
    VALUES (
      @project_id, @client_id, @invoice_number, @issue_date, @due_date, @status,
      @subtotal, @tax_rate, @total, @notes,
      @tax_year, @currency,
      @gst_hst_applicable, @gst_hst_number, @gst_hst_rate, @gst_hst_amount
    )
  `)

  const result = stmt.run({
    project_id: invoiceData.project_id ?? null,
    client_id: invoiceData.client_id,
    invoice_number: invoiceNumber,
    issue_date: invoiceData.issue_date,
    due_date: invoiceData.due_date,
    status: invoiceData.status || 'draft',
    subtotal: invoiceData.subtotal || 0,
    tax_rate: invoiceData.tax_rate || 0,
    total: invoiceData.total || 0,
    notes: invoiceData.notes || '',
    tax_year: taxYear,
    currency: invoiceData.currency || 'CAD',
    gst_hst_applicable: invoiceData.gst_hst_applicable ? 1 : 0,
    gst_hst_number: invoiceData.gst_hst_number || null,
    gst_hst_rate: invoiceData.gst_hst_rate || 0,
    gst_hst_amount: invoiceData.gst_hst_amount || 0,
  })

  // Insert line items
  if (items && items.length > 0) {
    const itemStmt = db.prepare(`
      INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total)
      VALUES (?, ?, ?, ?, ?)
    `)
    for (const item of items) {
      itemStmt.run(result.lastInsertRowid, item.description, item.quantity, item.unit_price, item.total)
    }
  }

  // Mark time entries as invoiced for all relevant projects
  const projectIds: number[] = invoiceData.project_ids || (invoiceData.project_id ? [invoiceData.project_id] : [])
  if (projectIds.length > 0) {
    const placeholders = projectIds.map(() => '?').join(',')
    db.prepare(`
      UPDATE time_entries SET is_invoiced = 1
      WHERE project_id IN (${placeholders}) AND is_invoiced = 0 AND is_billable = 1 AND end_time IS NOT NULL
    `).run(...projectIds)
  }

  // Increment invoice number
  db.prepare("UPDATE settings SET value = ? WHERE key = 'invoice_next_number'")
    .run(String(settings.invoice_next_number + 1))

  return getInvoice(result.lastInsertRowid as number)
}

export function updateInvoice(id: number, data: any) {
  const { items, ...invoiceData } = data

  // Re-derive tax_year if issue_date changed and tax_year wasn't explicitly set
  if (invoiceData.issue_date && invoiceData.tax_year === undefined) {
    invoiceData.tax_year = new Date(invoiceData.issue_date).getFullYear()
  }
  // Auto-stamp payment_date when transitioning to paid (if not explicitly set)
  if (invoiceData.status === 'paid' && invoiceData.payment_date === undefined) {
    invoiceData.payment_date = new Date().toISOString().slice(0, 10)
  }

  const exclude = new Set([
    'id', 'created_at', 'client_name', 'client_company', 'client_email',
    'client_address', 'project_name', 'items',
  ])
  const fields = Object.keys(invoiceData).filter(k => !exclude.has(k))
  if (fields.length > 0) {
    const sets = fields.map(f => `${f} = @${f}`).join(', ')
    db.prepare(`UPDATE invoices SET ${sets} WHERE id = @id`).run({ ...invoiceData, id })
  }

  if (items) {
    db.prepare('DELETE FROM invoice_items WHERE invoice_id = ?').run(id)
    const itemStmt = db.prepare(`
      INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total)
      VALUES (?, ?, ?, ?, ?)
    `)
    for (const item of items) {
      itemStmt.run(id, item.description, item.quantity, item.unit_price, item.total)
    }
  }

  return getInvoice(id)
}

export function deleteInvoice(id: number) {
  db.prepare('DELETE FROM invoices WHERE id = ?').run(id)
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
  const startOfWeek = new Date(now)
  startOfWeek.setDate(now.getDate() - now.getDay())
  startOfWeek.setHours(0, 0, 0, 0)

  const startOfMonth = new Date(now.getFullYear(), now.getMonth(), 1)

  const hoursThisWeek = db.prepare(`
    SELECT COALESCE(SUM(duration_minutes) / 60.0, 0) as hours
    FROM time_entries
    WHERE end_time IS NOT NULL AND start_time >= ?
  `).get(startOfWeek.toISOString()) as any

  const hoursThisMonth = db.prepare(`
    SELECT COALESCE(SUM(duration_minutes) / 60.0, 0) as hours
    FROM time_entries
    WHERE end_time IS NOT NULL AND start_time >= ?
  `).get(startOfMonth.toISOString()) as any

  const unbilled = db.prepare(`
    SELECT COALESCE(SUM(te.duration_minutes / 60.0), 0) as hours
    FROM time_entries te
    WHERE te.end_time IS NOT NULL AND te.is_invoiced = 0 AND te.is_billable = 1
  `).get() as any

  const outstanding = db.prepare(`
    SELECT COALESCE(SUM(total), 0) as total
    FROM invoices WHERE status IN ('sent', 'overdue')
  `).get() as any

  const paid = db.prepare(`
    SELECT COALESCE(SUM(total), 0) as total
    FROM invoices WHERE status = 'paid'
  `).get() as any

  return {
    hours_this_week: hoursThisWeek.hours,
    hours_this_month: hoursThisMonth.hours,
    unbilled_hours: unbilled.hours,
    outstanding_total: outstanding.total,
    paid_total: paid.total,
  }
}

export function getRecentEntries() {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.color as project_color, p.rate,
           c.name as client_name
    FROM time_entries te
    LEFT JOIN projects p ON te.project_id = p.id
    LEFT JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NOT NULL
    ORDER BY te.start_time DESC
    LIMIT 10
  `).all()
}

// ============ Reports ============

export function hoursByProject(startDate: string, endDate: string) {
  return db.prepare(`
    SELECT p.name, p.color, COALESCE(SUM(te.duration_minutes) / 60.0, 0) as hours
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE te.end_time IS NOT NULL AND te.start_time >= ? AND te.start_time <= ?
    GROUP BY p.id
    ORDER BY hours DESC
  `).all(startDate, endDate)
}

export function hoursByClient(startDate: string, endDate: string) {
  return db.prepare(`
    SELECT c.name, COALESCE(SUM(te.duration_minutes) / 60.0, 0) as hours
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    JOIN clients c ON p.client_id = c.id
    WHERE te.end_time IS NOT NULL AND te.start_time >= ? AND te.start_time <= ?
    GROUP BY c.id
    ORDER BY hours DESC
  `).all(startDate, endDate)
}

export function earningsByMonth(startDate: string, endDate: string) {
  return db.prepare(`
    SELECT strftime('%Y-%m', i.issue_date) as month, COALESCE(SUM(i.total), 0) as earnings
    FROM invoices i
    WHERE i.status IN ('paid', 'sent', 'overdue') AND i.issue_date >= ? AND i.issue_date <= ?
    GROUP BY month
    ORDER BY month
  `).all(startDate, endDate)
}

export function getUnbilledEntries(projectId: number) {
  return db.prepare(`
    SELECT te.*, p.name as project_name, p.rate
    FROM time_entries te
    JOIN projects p ON te.project_id = p.id
    WHERE te.project_id = ? AND te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL
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
    WHERE p.client_id = ? AND te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL
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
    WHERE te.project_id IN (${placeholders}) AND te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL
    ORDER BY p.name, te.start_time
  `).all(...projectIds)
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
  const date = data.date || new Date().toISOString().slice(0, 10)
  const taxYear = data.tax_year || new Date(date).getFullYear()
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
    data.tax_year = new Date(data.date).getFullYear()
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

    const now = new Date().toISOString().slice(0, 10)
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
    const now = new Date().toISOString().slice(0, 10)
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
