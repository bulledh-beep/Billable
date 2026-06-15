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

  // ----- Phase 2: Family Finance Expansion -----
  db.exec(`
    CREATE TABLE IF NOT EXISTS workspaces (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      name TEXT NOT NULL,
      created_at TEXT DEFAULT (datetime('now'))
    );

    CREATE TABLE IF NOT EXISTS users (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      name TEXT NOT NULL,
      email TEXT DEFAULT '',
      role TEXT DEFAULT 'member',
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS bills (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      user_id INTEGER,
      vendor TEXT NOT NULL,
      amount REAL DEFAULT 0,
      currency TEXT DEFAULT 'CAD',
      due_date TEXT,
      status TEXT DEFAULT 'upcoming' CHECK(status IN ('upcoming', 'due_soon', 'overdue', 'paid', 'autopay_scheduled', 'needs_review')),
      category TEXT DEFAULT 'other',
      recurring INTEGER DEFAULT 0,
      frequency TEXT DEFAULT 'one_time' CHECK(frequency IN ('one_time', 'weekly', 'monthly', 'quarterly', 'yearly')),
      autopay INTEGER DEFAULT 0,
      notes TEXT DEFAULT '',
      source TEXT DEFAULT 'manual',
      source_email_id TEXT,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS subscriptions (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      user_id INTEGER,
      name TEXT NOT NULL,
      vendor TEXT NOT NULL,
      amount REAL DEFAULT 0,
      currency TEXT DEFAULT 'CAD',
      billing_cycle TEXT DEFAULT 'monthly',
      next_billing_date TEXT,
      category TEXT DEFAULT 'software',
      payment_method TEXT DEFAULT '',
      status TEXT DEFAULT 'active' CHECK(status IN ('active', 'paused', 'cancelled')),
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS payments (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      user_id INTEGER,
      bill_id INTEGER,
      invoice_id INTEGER,
      amount REAL DEFAULT 0,
      currency TEXT DEFAULT 'CAD',
      payment_date TEXT NOT NULL,
      payment_method TEXT DEFAULT '',
      reference_number TEXT DEFAULT '',
      notes TEXT DEFAULT '',
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL,
      FOREIGN KEY (bill_id) REFERENCES bills(id) ON DELETE SET NULL,
      FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS email_imports (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      user_id INTEGER,
      provider TEXT DEFAULT 'gmail',
      source_email_id TEXT UNIQUE,
      sender TEXT NOT NULL,
      subject TEXT NOT NULL,
      received_at TEXT NOT NULL,
      body_preview TEXT DEFAULT '',
      attachment_names TEXT DEFAULT '',
      status TEXT DEFAULT 'detected' CHECK(status IN ('detected', 'reviewed', 'imported', 'ignored', 'duplicate')),
      confidence_score REAL DEFAULT 1.0,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS bill_import_candidates (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      user_id INTEGER,
      email_import_id INTEGER,
      extracted_vendor TEXT,
      extracted_amount REAL,
      extracted_currency TEXT DEFAULT 'CAD',
      extracted_due_date TEXT,
      extracted_invoice_date TEXT,
      extracted_payment_date TEXT,
      extracted_status TEXT DEFAULT 'needs_review',
      extracted_category TEXT,
      extracted_invoice_number TEXT,
      extracted_frequency TEXT DEFAULT 'one_time',
      extracted_record_type TEXT CHECK(extracted_record_type IN ('bill', 'expense', 'payment', 'subscription', 'receipt')),
      confidence_score REAL DEFAULT 0,
      duplicate_of_id INTEGER,
      raw_extraction_json TEXT,
      review_status TEXT DEFAULT 'needs_review' CHECK(review_status IN ('needs_review', 'approved', 'ignored', 'duplicate')),
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL,
      FOREIGN KEY (email_import_id) REFERENCES email_imports(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS automation_rules (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      user_id INTEGER,
      rule_name TEXT NOT NULL,
      sender_contains TEXT DEFAULT '',
      subject_contains TEXT DEFAULT '',
      vendor TEXT DEFAULT '',
      category TEXT DEFAULT '',
      record_type TEXT DEFAULT 'bill',
      recurring_frequency TEXT DEFAULT 'monthly',
      auto_approve INTEGER DEFAULT 0,
      is_active INTEGER DEFAULT 1,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
    );

    CREATE TABLE IF NOT EXISTS budget_categories (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      name TEXT NOT NULL,
      color TEXT DEFAULT '#3B82F6',
      created_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      UNIQUE (workspace_id, name)
    );

    CREATE TABLE IF NOT EXISTS monthly_budgets (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      workspace_id INTEGER,
      category_id INTEGER NOT NULL,
      month TEXT NOT NULL, -- 'YYYY-MM'
      amount_limit REAL NOT NULL,
      created_at TEXT DEFAULT (datetime('now')),
      updated_at TEXT DEFAULT (datetime('now')),
      FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE SET NULL,
      FOREIGN KEY (category_id) REFERENCES budget_categories(id) ON DELETE CASCADE,
      UNIQUE (workspace_id, category_id, month)
    );
  `)

  // Add new columns to expenses
  addColumnIfMissing('expenses', 'workspace_id', 'INTEGER')
  addColumnIfMissing('expenses', 'user_id', 'INTEGER')
  addColumnIfMissing('expenses', 'vendor', "TEXT DEFAULT ''")
  addColumnIfMissing('expenses', 'currency', "TEXT DEFAULT 'CAD'")
  addColumnIfMissing('expenses', 'is_deductible', 'INTEGER DEFAULT 1')
  addColumnIfMissing('expenses', 'is_reimbursable', 'INTEGER DEFAULT 0')
  addColumnIfMissing('expenses', 'client_id', 'INTEGER')
  addColumnIfMissing('expenses', 'project_id', 'INTEGER')
  addColumnIfMissing('expenses', 'receipt_url', 'TEXT')
  addColumnIfMissing('expenses', 'source', "TEXT DEFAULT 'manual'")
  addColumnIfMissing('expenses', 'source_email_id', 'TEXT')
  addColumnIfMissing('expenses', 'updated_at', "TEXT DEFAULT (datetime('now'))")

  // ----- Income ledger: payments table holds money-in (e-Transfers, deposits,
  // other income) as plain transactions — NOT fake invoices. -----
  addColumnIfMissing('payments', 'vendor', "TEXT DEFAULT ''")
  addColumnIfMissing('payments', 'category', "TEXT DEFAULT 'income'")
  addColumnIfMissing('payments', 'source', "TEXT DEFAULT 'manual'")
  addColumnIfMissing('payments', 'source_email_id', 'TEXT')

  // ----- Phase 1: trustworthy email scoring -----
  // Relevance ("is this financial?") is separate from confidence ("are the
  // extracted fields right?"). Auto-approval requires both to be high.
  addColumnIfMissing('bill_import_candidates', 'relevance_score', 'REAL DEFAULT 0')
  addColumnIfMissing('bill_import_candidates', 'detected_reason', "TEXT DEFAULT ''")
  addColumnIfMissing('bill_import_candidates', 'low_confidence_reason', "TEXT DEFAULT ''")
  addColumnIfMissing('bill_import_candidates', 'suggested_action', "TEXT DEFAULT ''")
  addColumnIfMissing('bill_import_candidates', 'extracted_account_last4', 'TEXT')
  addColumnIfMissing('bill_import_candidates', 'extracted_payment_method', 'TEXT')
  addColumnIfMissing('bill_import_candidates', 'auto_approved', 'INTEGER DEFAULT 0')

  // Seed default workspace and user
  db.prepare(`
    INSERT OR IGNORE INTO workspaces (id, name) VALUES (1, 'Default Workspace')
  `).run()

  db.prepare(`
    INSERT OR IGNORE INTO users (id, workspace_id, name, email, role)
    VALUES (1, 1, 'Default User', '', 'owner')
  `).run()
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
    bill_tracking_enabled: '1',
    // Email auto-approval: only fires when relevance AND confidence both clear
    // this threshold, all key fields are present, and there's no duplicate.
    bill_auto_approve_enabled: '1',
    bill_auto_approve_threshold: '85',
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
      COALESCE(SUM(CASE WHEN te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes / 60.0 ELSE 0 END), 0) as unbilled_hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes ELSE 0 END) / 60.0, 0) as total_billable_hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 0 AND te.end_time IS NOT NULL THEN te.duration_minutes ELSE 0 END) / 60.0, 0) as total_non_billable_hours
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
      COALESCE(SUM(CASE WHEN te.is_invoiced = 0 AND te.is_billable = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes / 60.0 ELSE 0 END), 0) as unbilled_hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 1 AND te.end_time IS NOT NULL THEN te.duration_minutes ELSE 0 END) / 60.0, 0) as total_billable_hours,
      COALESCE(SUM(CASE WHEN te.is_billable = 0 AND te.end_time IS NOT NULL THEN te.duration_minutes ELSE 0 END) / 60.0, 0) as total_non_billable_hours
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

export function startTimer(projectId: number, description: string = '', isBillable: number = 1) {
  const now = new Date().toISOString()
  const stmt = db.prepare(`
    INSERT INTO time_entries (project_id, description, start_time, is_billable)
    VALUES (?, ?, ?, ?)
  `)
  const result = stmt.run(projectId, description, now, isBillable)
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
  const todayStr = now.toISOString().slice(0, 10)
  
  // Start of week
  const startOfWeek = new Date(now)
  startOfWeek.setDate(now.getDate() - now.getDay())
  startOfWeek.setHours(0, 0, 0, 0)

  // End of week
  const endOfWeek = new Date(startOfWeek)
  endOfWeek.setDate(startOfWeek.getDate() + 7)
  const endOfWeekStr = endOfWeek.toISOString().slice(0, 10)

  // Start of month
  const startOfMonth = new Date(now.getFullYear(), now.getMonth(), 1)
  const startOfMonthStr = startOfMonth.toISOString().slice(0, 10)

  // End of month
  const endOfMonth = new Date(now.getFullYear(), now.getMonth() + 1, 0)
  const endOfMonthStr = endOfMonth.toISOString().slice(0, 10)

  // 30 days from now
  const next30Days = new Date(now)
  next30Days.setDate(now.getDate() + 30)
  const next30DaysStr = next30Days.toISOString().slice(0, 10)

  // 1. Time tracking stats
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

  // 2. Invoice Income stats
  const outstandingInvoices = db.prepare(`
    SELECT COALESCE(SUM(total), 0) as total
    FROM invoices WHERE status IN ('sent', 'overdue')
  `).get() as any

  const paidInvoicesThisMonth = db.prepare(`
    SELECT COALESCE(SUM(total), 0) as total,
           COALESCE(SUM(CASE WHEN gst_hst_applicable = 1 THEN gst_hst_amount ELSE 0 END), 0) as gst
    FROM invoices 
    WHERE status = 'paid' AND COALESCE(payment_date, issue_date) >= ? AND COALESCE(payment_date, issue_date) <= ?
  `).get(startOfMonthStr, endOfMonthStr) as any

  // 3. Expense stats
  const expensesThisMonth = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total
    FROM expenses
    WHERE date >= ? AND date <= ?
  `).get(startOfMonthStr, endOfMonthStr) as any

  const deductibleExpensesThisMonth = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total
    FROM expenses
    WHERE date >= ? AND date <= ? AND is_deductible = 1
  `).get(startOfMonthStr, endOfMonthStr) as any

  const expensesByCategory = db.prepare(`
    SELECT category, COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM expenses
    WHERE date >= ? AND date <= ?
    GROUP BY category
  `).all(startOfMonthStr, endOfMonthStr) as any[]

  // 4. Bills stats
  const billsDueThisWeek = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM bills
    WHERE status != 'paid' AND due_date >= ? AND due_date <= ?
  `).get(todayStr, endOfWeekStr) as any

  const billsDueThisMonth = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM bills
    WHERE status != 'paid' AND due_date >= ? AND due_date <= ?
  `).get(startOfMonthStr, endOfMonthStr) as any

  const billsDueInNext30Days = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total
    FROM bills
    WHERE status != 'paid' AND due_date >= ? AND due_date <= ?
  `).get(todayStr, next30DaysStr) as any

  const billsOverdue = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM bills
    WHERE status != 'paid' AND due_date < ?
  `).get(todayStr) as any

  const billsPaidThisMonth = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM bills
    WHERE status = 'paid' AND updated_at >= ? AND updated_at <= ?
  `).get(startOfMonthStr, endOfMonthStr) as any

  // 5. Subscription stats
  const activeSubscriptionsTotal = db.prepare(`
    SELECT COALESCE(SUM(amount), 0) as total
    FROM subscriptions
    WHERE status = 'active'
  `).get() as any

  // 6. Tax settings / set-aside
  const taxSettings = getTaxSettings()
  const taxRate = (taxSettings?.income_tax_bracket || 25) / 100.0

  const taxableIncomeThisMonth = Math.max(0, (paidInvoicesThisMonth.total - paidInvoicesThisMonth.gst) - deductibleExpensesThisMonth.total)
  const estimatedTaxSetAside = taxableIncomeThisMonth * taxRate

  // 7. Safe to Spend
  const safeToSpend = paidInvoicesThisMonth.total - expensesThisMonth.total - billsDueInNext30Days.total - estimatedTaxSetAside

  // 8. Upcoming payment reminders (next 5 upcoming unpaid bills)
  const upcomingReminders = db.prepare(`
    SELECT id, vendor, amount, currency, due_date, status, category
    FROM bills
    WHERE status != 'paid' AND due_date >= ?
    ORDER BY due_date ASC
    LIMIT 5
  `).all(todayStr)

  return {
    hours_this_week: hoursThisWeek.hours,
    hours_this_month: hoursThisMonth.hours,
    unbilled_hours: unbilled.hours,
    outstanding_invoices: outstandingInvoices.total,
    paid_income_this_month: paidInvoicesThisMonth.total,
    expenses_this_month: expensesThisMonth.total,
    expenses_by_category: expensesByCategory,
    bills_due_this_week_total: billsDueThisWeek.total,
    bills_due_this_week_count: billsDueThisWeek.count,
    bills_due_this_month_total: billsDueThisMonth.total,
    bills_due_this_month_count: billsDueThisMonth.count,
    bills_due_in_next_30_days: billsDueInNext30Days.total,
    bills_overdue_total: billsOverdue.total,
    bills_overdue_count: billsOverdue.count,
    bills_paid_this_month_total: billsPaidThisMonth.total,
    bills_paid_this_month_count: billsPaidThisMonth.count,
    active_subscriptions_total: activeSubscriptionsTotal.total,
    estimated_tax_set_aside: estimatedTaxSetAside,
    tax_bracket_rate: taxRate * 100,
    safe_to_spend: safeToSpend,
    upcoming_reminders: upcomingReminders,
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

  // Expenses by category for the same year (only deductible ones count for taxes)
  const expensesByCategory = db.prepare(`
    SELECT category, COALESCE(SUM(amount), 0) as total, COUNT(*) as count
    FROM expenses
    WHERE tax_year = ? AND is_deductible = 1
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
    INSERT INTO expenses (
      date, category, description, amount, tax_year, receipt_note, receipt_id,
      workspace_id, user_id, vendor, currency, is_deductible, is_reimbursable,
      client_id, project_id, receipt_url, source, source_email_id
    )
    VALUES (
      @date, @category, @description, @amount, @tax_year, @receipt_note, @receipt_id,
      @workspace_id, @user_id, @vendor, @currency, @is_deductible, @is_reimbursable,
      @client_id, @project_id, @receipt_url, @source, @source_email_id
    )
  `)
  const result = stmt.run({
    date,
    category: data.category || 'other',
    description: data.description || '',
    amount: data.amount || 0,
    tax_year: taxYear,
    receipt_note: data.receipt_note || '',
    receipt_id: data.receipt_id || null,
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    vendor: data.vendor || '',
    currency: data.currency || 'CAD',
    is_deductible: data.is_deductible ?? 1,
    is_reimbursable: data.is_reimbursable ?? 0,
    client_id: data.client_id || null,
    project_id: data.project_id || null,
    receipt_url: data.receipt_url || null,
    source: data.source || 'manual',
    source_email_id: data.source_email_id || null,
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

// ============ Bills ============
export function listBills() {
  return db.prepare('SELECT * FROM bills ORDER BY due_date ASC, id DESC').all()
}

export function getBill(id: number) {
  return db.prepare('SELECT * FROM bills WHERE id = ?').get(id)
}

export function createBill(data: any) {
  const stmt = db.prepare(`
    INSERT INTO bills (workspace_id, user_id, vendor, amount, currency, due_date, status, category, recurring, frequency, autopay, notes, source, source_email_id)
    VALUES (@workspace_id, @user_id, @vendor, @amount, @currency, @due_date, @status, @category, @recurring, @frequency, @autopay, @notes, @source, @source_email_id)
  `)
  const result = stmt.run({
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    vendor: data.vendor,
    amount: data.amount ?? 0,
    currency: data.currency ?? 'CAD',
    due_date: data.due_date || null,
    status: data.status ?? 'upcoming',
    category: data.category ?? 'other',
    recurring: data.recurring ? 1 : 0,
    frequency: data.frequency ?? 'one_time',
    autopay: data.autopay ? 1 : 0,
    notes: data.notes ?? '',
    source: data.source ?? 'manual',
    source_email_id: data.source_email_id || null,
  })
  return getBill(result.lastInsertRowid as number)
}

export function updateBill(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  if (fields.length === 0) return getBill(id)
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE bills SET ${sets}, updated_at = datetime('now') WHERE id = @id`).run({ ...data, id })
  return getBill(id)
}

export function deleteBill(id: number) {
  db.prepare('DELETE FROM bills WHERE id = ?').run(id)
}

// ============ Subscriptions ============
export function listSubscriptions() {
  return db.prepare('SELECT * FROM subscriptions ORDER BY name ASC').all()
}

export function getSubscription(id: number) {
  return db.prepare('SELECT * FROM subscriptions WHERE id = ?').get(id)
}

export function createSubscription(data: any) {
  const stmt = db.prepare(`
    INSERT INTO subscriptions (workspace_id, user_id, name, vendor, amount, currency, billing_cycle, next_billing_date, category, payment_method, status)
    VALUES (@workspace_id, @user_id, @name, @vendor, @amount, @currency, @billing_cycle, @next_billing_date, @category, @payment_method, @status)
  `)
  const result = stmt.run({
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    name: data.name,
    vendor: data.vendor,
    amount: data.amount ?? 0,
    currency: data.currency ?? 'CAD',
    billing_cycle: data.billing_cycle ?? 'monthly',
    next_billing_date: data.next_billing_date || null,
    category: data.category ?? 'software',
    payment_method: data.payment_method ?? '',
    status: data.status ?? 'active',
  })
  return getSubscription(result.lastInsertRowid as number)
}

export function updateSubscription(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  if (fields.length === 0) return getSubscription(id)
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE subscriptions SET ${sets}, updated_at = datetime('now') WHERE id = @id`).run({ ...data, id })
  return getSubscription(id)
}

export function deleteSubscription(id: number) {
  db.prepare('DELETE FROM subscriptions WHERE id = ?').run(id)
}

// ============ Payments ============
export function listPayments() {
  return db.prepare('SELECT * FROM payments ORDER BY payment_date DESC, id DESC').all()
}

/**
 * Record money received (income) as a plain transaction in the income ledger.
 * No invoice matching, no phantom clients/invoices — an e-Transfer from family
 * is just income, not invoiced business revenue. (Real invoice income is tracked
 * separately via the invoices table.)
 */
export function createPayment(data: any) {
  const stmt = db.prepare(`
    INSERT INTO payments (workspace_id, user_id, bill_id, invoice_id, vendor, amount, currency, payment_date, payment_method, category, reference_number, notes, source, source_email_id)
    VALUES (@workspace_id, @user_id, @bill_id, @invoice_id, @vendor, @amount, @currency, @payment_date, @payment_method, @category, @reference_number, @notes, @source, @source_email_id)
  `)
  const result = stmt.run({
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    bill_id: data.bill_id || null,
    invoice_id: data.invoice_id || null,
    vendor: data.vendor ?? '',
    amount: data.amount ?? 0,
    currency: data.currency ?? 'CAD',
    payment_date: data.payment_date || new Date().toISOString().slice(0, 10),
    payment_method: data.payment_method ?? '',
    category: data.category ?? 'income',
    reference_number: data.reference_number ?? '',
    notes: data.notes ?? '',
    source: data.source ?? 'manual',
    source_email_id: data.source_email_id || null,
  })
  return db.prepare('SELECT * FROM payments WHERE id = ?').get(result.lastInsertRowid)
}

export function deletePayment(id: number) {
  db.prepare('DELETE FROM payments WHERE id = ?').run(id)
}

// ============ Email Imports & Candidates ============
export function listEmailImports() {
  return db.prepare('SELECT * FROM email_imports ORDER BY received_at DESC').all()
}

export function getEmailImport(id: number) {
  return db.prepare('SELECT * FROM email_imports WHERE id = ?').get(id)
}

export function createEmailImport(data: any) {
  const stmt = db.prepare(`
    INSERT INTO email_imports (workspace_id, user_id, provider, source_email_id, sender, subject, received_at, body_preview, attachment_names, status, confidence_score)
    VALUES (@workspace_id, @user_id, @provider, @source_email_id, @sender, @subject, @received_at, @body_preview, @attachment_names, @status, @confidence_score)
  `)
  const result = stmt.run({
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    provider: data.provider ?? 'gmail',
    source_email_id: data.source_email_id,
    sender: data.sender,
    subject: data.subject,
    received_at: data.received_at,
    body_preview: data.body_preview ?? '',
    attachment_names: data.attachment_names ?? '',
    status: data.status ?? 'detected',
    confidence_score: data.confidence_score ?? 1.0,
  })
  return getEmailImport(result.lastInsertRowid as number)
}

export function updateEmailImportStatus(id: number, status: string) {
  db.prepare(`UPDATE email_imports SET status = ?, updated_at = datetime('now') WHERE id = ?`).run(status, id)
  return getEmailImport(id)
}

export function listCandidates(reviewStatus?: string) {
  if (reviewStatus) {
    return db.prepare('SELECT * FROM bill_import_candidates WHERE review_status = ? ORDER BY created_at DESC').all(reviewStatus)
  }
  return db.prepare('SELECT * FROM bill_import_candidates ORDER BY created_at DESC').all()
}

export function getCandidate(id: number) {
  return db.prepare('SELECT * FROM bill_import_candidates WHERE id = ?').get(id)
}

export function createCandidate(data: any) {
  const stmt = db.prepare(`
    INSERT INTO bill_import_candidates (workspace_id, user_id, email_import_id, extracted_vendor, extracted_amount, extracted_currency, extracted_due_date, extracted_invoice_date, extracted_payment_date, extracted_status, extracted_category, extracted_invoice_number, extracted_frequency, extracted_record_type, confidence_score, relevance_score, detected_reason, low_confidence_reason, suggested_action, extracted_account_last4, extracted_payment_method, auto_approved, duplicate_of_id, raw_extraction_json, review_status)
    VALUES (@workspace_id, @user_id, @email_import_id, @extracted_vendor, @extracted_amount, @extracted_currency, @extracted_due_date, @extracted_invoice_date, @extracted_payment_date, @extracted_status, @extracted_category, @extracted_invoice_number, @extracted_frequency, @extracted_record_type, @confidence_score, @relevance_score, @detected_reason, @low_confidence_reason, @suggested_action, @extracted_account_last4, @extracted_payment_method, @auto_approved, @duplicate_of_id, @raw_extraction_json, @review_status)
  `)
  const result = stmt.run({
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    email_import_id: data.email_import_id || null,
    extracted_vendor: data.extracted_vendor || null,
    extracted_amount: data.extracted_amount ?? null,
    extracted_currency: data.extracted_currency ?? 'CAD',
    extracted_due_date: data.extracted_due_date || null,
    extracted_invoice_date: data.extracted_invoice_date || null,
    extracted_payment_date: data.extracted_payment_date || null,
    extracted_status: data.extracted_status ?? 'needs_review',
    extracted_category: data.extracted_category || null,
    extracted_invoice_number: data.extracted_invoice_number || null,
    extracted_frequency: data.extracted_frequency ?? 'one_time',
    extracted_record_type: data.extracted_record_type ?? 'bill',
    confidence_score: data.confidence_score ?? 0,
    relevance_score: data.relevance_score ?? 0,
    detected_reason: data.detected_reason ?? '',
    low_confidence_reason: data.low_confidence_reason ?? '',
    suggested_action: data.suggested_action ?? '',
    extracted_account_last4: data.extracted_account_last4 || null,
    extracted_payment_method: data.extracted_payment_method || null,
    auto_approved: data.auto_approved ? 1 : 0,
    duplicate_of_id: data.duplicate_of_id || null,
    raw_extraction_json: data.raw_extraction_json || null,
    review_status: data.review_status ?? 'needs_review',
  })
  return getCandidate(result.lastInsertRowid as number)
}

export function updateCandidate(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  if (fields.length === 0) return getCandidate(id)
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE bill_import_candidates SET ${sets}, updated_at = datetime('now') WHERE id = ?`).run({ ...data, id })
  return getCandidate(id)
}

export function deleteCandidate(id: number) {
  db.prepare('DELETE FROM bill_import_candidates WHERE id = ?').run(id)
}

// ============ Automation Rules ============
export function listAutomationRules() {
  return db.prepare('SELECT * FROM automation_rules ORDER BY id DESC').all()
}

export function getAutomationRule(id: number) {
  return db.prepare('SELECT * FROM automation_rules WHERE id = ?').get(id)
}

export function createAutomationRule(data: any) {
  const stmt = db.prepare(`
    INSERT INTO automation_rules (workspace_id, user_id, rule_name, sender_contains, subject_contains, vendor, category, record_type, recurring_frequency, auto_approve, is_active)
    VALUES (@workspace_id, @user_id, @rule_name, @sender_contains, @subject_contains, @vendor, @category, @record_type, @recurring_frequency, @auto_approve, @is_active)
  `)
  const result = stmt.run({
    workspace_id: data.workspace_id ?? 1,
    user_id: data.user_id ?? 1,
    rule_name: data.rule_name,
    sender_contains: data.sender_contains ?? '',
    subject_contains: data.subject_contains ?? '',
    vendor: data.vendor ?? '',
    category: data.category ?? '',
    record_type: data.record_type ?? 'bill',
    recurring_frequency: data.recurring_frequency ?? 'monthly',
    auto_approve: data.auto_approve ? 1 : 0,
    is_active: data.is_active ?? 1,
  })
  return getAutomationRule(result.lastInsertRowid as number)
}

export function updateAutomationRule(id: number, data: any) {
  const fields = Object.keys(data).filter(k => k !== 'id' && k !== 'created_at')
  if (fields.length === 0) return getAutomationRule(id)
  const sets = fields.map(f => `${f} = @${f}`).join(', ')
  db.prepare(`UPDATE automation_rules SET ${sets}, updated_at = datetime('now') WHERE id = @id`).run({ ...data, id })
  return getAutomationRule(id)
}

export function deleteAutomationRule(id: number) {
  db.prepare('DELETE FROM automation_rules WHERE id = ?').run(id)
}

// ============ Budgets ============
export function listBudgetCategories() {
  return db.prepare('SELECT * FROM budget_categories ORDER BY name ASC').all()
}

export function createBudgetCategory(name: string, color?: string) {
  const stmt = db.prepare(`
    INSERT OR IGNORE INTO budget_categories (workspace_id, name, color)
    VALUES (1, ?, ?)
  `)
  stmt.run(name, color || '#3B82F6')
  return db.prepare('SELECT * FROM budget_categories WHERE workspace_id = 1 AND name = ?').get(name)
}

export function deleteBudgetCategory(id: number) {
  db.prepare('DELETE FROM budget_categories WHERE id = ?').run(id)
}

export function listMonthlyBudgets(month: string) {
  // Return all budget categories with their limit for this month and their actual expenses for this month
  return db.prepare(`
    SELECT
      bc.id as category_id,
      bc.name as category_name,
      bc.color as category_color,
      COALESCE(mb.amount_limit, 0) as amount_limit,
      COALESCE(mb.id, 0) as budget_id,
      COALESCE((
        SELECT SUM(amount)
        FROM expenses
        WHERE category = bc.name AND strftime('%Y-%m', date) = ?
      ), 0) as current_spend
    FROM budget_categories bc
    LEFT JOIN monthly_budgets mb ON mb.category_id = bc.id AND mb.month = ?
    WHERE bc.workspace_id = 1
    ORDER BY bc.name ASC
  `).all(month, month)
}

export function setMonthlyBudget(categoryId: number, month: string, amountLimit: number) {
  db.prepare(`
    INSERT INTO monthly_budgets (workspace_id, category_id, month, amount_limit)
    VALUES (1, ?, ?, ?)
    ON CONFLICT(workspace_id, category_id, month) DO UPDATE SET amount_limit = excluded.amount_limit, updated_at = datetime('now')
  `).run(categoryId, month, amountLimit)

  return db.prepare('SELECT * FROM monthly_budgets WHERE workspace_id = 1 AND category_id = ? AND month = ?').get(categoryId, month)
}

export { db }
