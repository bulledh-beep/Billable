import Database from 'better-sqlite3'
import path from 'path'
import { app } from 'electron'

let db: Database.Database

export function initDatabase(): Database.Database {
  const dbPath = path.join(app.getPath('userData'), 'billable.db')
  db = new Database(dbPath)

  db.pragma('journal_mode = WAL')
  db.pragma('foreign_keys = ON')

  createTables()
  seedDefaults()

  return db
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

  const stmt = db.prepare(`
    INSERT INTO invoices (project_id, client_id, invoice_number, issue_date, due_date, status, subtotal, tax_rate, total, notes)
    VALUES (@project_id, @client_id, @invoice_number, @issue_date, @due_date, @status, @subtotal, @tax_rate, @total, @notes)
  `)

  const result = stmt.run({
    ...invoiceData,
    invoice_number: invoiceNumber,
    status: invoiceData.status || 'draft',
    notes: invoiceData.notes || '',
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
  const fields = Object.keys(invoiceData).filter(k => k !== 'id' && k !== 'created_at' && k !== 'client_name' && k !== 'client_company' && k !== 'client_email' && k !== 'client_address' && k !== 'project_name')
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

export { db }
