export interface Client {
  id: number
  name: string
  company: string
  email: string
  address: string
  default_rate: number
  currency: string
  created_at: string
}

export interface Project {
  id: number
  client_id: number
  name: string
  description: string
  rate: number
  status: 'active' | 'paused' | 'complete' | 'archived'
  color: string
  created_at: string
  // Joined fields
  client_name?: string
  client_company?: string
  total_hours?: number
  billed_total?: number
  unbilled_hours?: number
  total_billable_hours?: number
  total_non_billable_hours?: number
}

export interface TimeEntry {
  id: number
  project_id: number
  description: string
  start_time: string
  end_time: string | null
  duration_minutes: number
  is_billable: number
  is_invoiced: number
  created_at: string
  // Joined fields
  project_name?: string
  project_color?: string
  client_name?: string
  rate?: number
}

export interface Invoice {
  id: number
  project_id: number | null
  client_id: number
  invoice_number: string
  issue_date: string
  due_date: string
  status: 'draft' | 'sent' | 'paid' | 'overdue'
  subtotal: number
  tax_rate: number
  total: number
  notes: string
  pdf_path: string | null
  created_at: string
  // Tax / payment fields (added Phase 1)
  tax_year?: number | null
  payment_date?: string | null
  payment_method?: string | null
  currency?: string
  gst_hst_applicable?: number
  gst_hst_number?: string | null
  gst_hst_rate?: number
  gst_hst_amount?: number
  // Joined fields
  client_name?: string
  client_company?: string
  client_email?: string
  client_address?: string
  project_name?: string
  items?: InvoiceItem[]
}

export type CanadianProvince =
  | 'AB' | 'BC' | 'MB' | 'NB' | 'NL' | 'NS' | 'NT' | 'NU' | 'ON' | 'PE' | 'QC' | 'SK' | 'YT'

export interface TaxSettings {
  id: number
  business_name: string
  business_address: string
  gst_hst_number: string
  gst_hst_registered: number
  province: CanadianProvince | ''
  fiscal_year_start: string // 'MM-DD'
  default_tax_rate: number
  income_tax_bracket: number
  currency: 'CAD' | 'USD'
  updated_at: string
}

export type ExpenseCategory =
  | 'equipment'
  | 'software'
  | 'home_office'
  | 'phone_internet'
  | 'travel'
  | 'meals'
  | 'professional_development'
  | 'other'

export interface Expense {
  id: number
  date: string
  category: ExpenseCategory
  description: string
  amount: number
  tax_year: number
  receipt_note: string
  receipt_id: number | null
  created_at: string
  workspace_id?: number | null
  user_id?: number | null
  vendor?: string
  currency?: string
  is_deductible?: number
  is_reimbursable?: number
  client_id?: number | null
  project_id?: number | null
  receipt_url?: string | null
  source?: string
  source_email_id?: string | null
  updated_at?: string
}

export interface Workspace {
  id: number
  name: string
  created_at: string
}

export interface User {
  id: number
  workspace_id: number | null
  name: string
  email: string
  role: 'owner' | 'member'
  created_at: string
}

export type BillStatus =
  | 'upcoming'
  | 'due_soon'
  | 'overdue'
  | 'paid'
  | 'autopay_scheduled'
  | 'needs_review'

export type BillFrequency = 'one_time' | 'weekly' | 'monthly' | 'quarterly' | 'yearly'

export interface Bill {
  id: number
  workspace_id: number | null
  user_id: number | null
  vendor: string
  amount: number
  currency: string
  due_date: string | null
  status: BillStatus
  category: string
  recurring: number
  frequency: BillFrequency
  autopay: number
  notes: string
  source: string
  source_email_id: string | null
  created_at: string
  updated_at: string
}

export interface Subscription {
  id: number
  workspace_id: number | null
  user_id: number | null
  name: string
  vendor: string
  amount: number
  currency: string
  billing_cycle: string
  next_billing_date: string | null
  category: string
  payment_method: string
  status: 'active' | 'paused' | 'cancelled'
  created_at: string
  updated_at: string
}

export interface Payment {
  id: number
  workspace_id: number | null
  user_id: number | null
  bill_id: number | null
  invoice_id: number | null
  amount: number
  currency: string
  payment_date: string
  payment_method: string
  reference_number: string
  notes: string
  created_at: string
  updated_at: string
}

export type EmailImportStatus = 'detected' | 'reviewed' | 'imported' | 'ignored' | 'duplicate'

export interface EmailImport {
  id: number
  workspace_id: number | null
  user_id: number | null
  provider: 'gmail'
  source_email_id: string
  sender: string
  subject: string
  received_at: string
  body_preview: string
  attachment_names: string
  status: EmailImportStatus
  confidence_score: number
  created_at: string
  updated_at: string
}

export type CandidateRecordType = 'bill' | 'expense' | 'payment' | 'subscription' | 'receipt'
export type CandidateReviewStatus = 'needs_review' | 'approved' | 'ignored' | 'duplicate'

export interface BillImportCandidate {
  id: number
  workspace_id: number | null
  user_id: number | null
  email_import_id: number | null
  extracted_vendor: string | null
  extracted_amount: number | null
  extracted_currency: string
  extracted_due_date: string | null
  extracted_invoice_date: string | null
  extracted_payment_date: string | null
  extracted_status: string
  extracted_category: string | null
  extracted_invoice_number: string | null
  extracted_frequency: string
  extracted_record_type: CandidateRecordType
  confidence_score: number
  duplicate_of_id: number | null
  raw_extraction_json: string | null
  review_status: CandidateReviewStatus
  created_at: string
  updated_at: string
}

export interface AutomationRule {
  id: number
  workspace_id: number | null
  user_id: number | null
  rule_name: string
  sender_contains: string
  subject_contains: string
  vendor: string
  category: string
  record_type: string
  recurring_frequency: string
  auto_approve: number
  is_active: number
  created_at: string
  updated_at: string
}

export interface BudgetCategory {
  id: number
  workspace_id: number | null
  name: string
  color: string
  created_at: string
}

export interface MonthlyBudget {
  id: number
  workspace_id: number | null
  category_id: number
  month: string // 'YYYY-MM'
  amount_limit: number
  created_at: string
  updated_at: string
  // Joined fields
  category_name?: string
  category_color?: string
}

export interface InvoiceItem {
  id: number
  invoice_id: number
  description: string
  quantity: number
  unit_price: number
  total: number
}

export interface PaymentMethod {
  name: string
  email: string
}

export interface Settings {
  business_name: string
  business_email: string
  business_address: string
  business_logo: string
  tax_id: string
  default_currency: string
  default_rate: number
  invoice_prefix: string
  invoice_next_number: number
  payment_methods: string // JSON string of PaymentMethod[]
  default_payment_method: string // name of the default method
  time_rounding: 'none' | '6' | '15' | '30'
  theme: 'dark' | 'light'
  google_client_id?: string
  google_client_secret?: string
}

export interface Profile {
  id: string
  name: string
  color: string
  /** Base64-encoded PNG (no `data:` prefix). Empty/undefined means no photo set. */
  avatar?: string
  created_at: string
}

export interface UpdateStatus {
  current_version: string
  latest_version: string | null
  update_available: boolean
  release_url: string | null
  release_notes: string | null
  download_url: string | null
  download_size_bytes: number | null
  published_at: string | null
  last_checked_at: string
}

export interface UpdateProgress {
  received: number
  total: number
  percent: number
}

export interface ProfileListResponse {
  profiles: Profile[]
  active: Profile | null
}

export interface DashboardStats {
  hours_this_week: number
  hours_this_month: number
  unbilled_hours: number
  outstanding_invoices: number
  paid_income_this_month: number
  expenses_this_month: number
  expenses_by_category: Array<{ category: string; total: number; count: number }>
  bills_due_this_week_total: number
  bills_due_this_week_count: number
  bills_due_this_month_total: number
  bills_due_this_month_count: number
  bills_due_in_next_30_days: number
  bills_overdue_total: number
  bills_overdue_count: number
  bills_paid_this_month_total: number
  bills_paid_this_month_count: number
  active_subscriptions_total: number
  estimated_tax_set_aside: number
  tax_bracket_rate: number
  safe_to_spend: number
  upcoming_reminders: any[]
}

export interface TaxOverview {
  tax_year: number
  total_invoiced: number
  total_paid: number
  total_outstanding: number
  gst_collected_paid: number
  gst_collected_total: number
  invoice_count: number
  paid_count: number
  expenses_by_category: Array<{ category: ExpenseCategory | string; total: number; count: number }>
  total_expenses: number
  monthly_income: Array<{ month: string; paid: number; invoiced: number }>
}

export type IpcChannels = {
  // Clients
  'clients:list': () => Client[]
  'clients:get': (id: number) => Client | null
  'clients:create': (data: Omit<Client, 'id' | 'created_at'>) => Client
  'clients:update': (id: number, data: Partial<Client>) => Client
  'clients:delete': (id: number) => void
  // Projects
  'projects:list': (clientId?: number) => Project[]
  'projects:get': (id: number) => Project | null
  'projects:create': (data: Omit<Project, 'id' | 'created_at'>) => Project
  'projects:update': (id: number, data: Partial<Project>) => Project
  'projects:delete': (id: number) => void
  // Time entries
  'time:list': (projectId?: number) => TimeEntry[]
  'time:get': (id: number) => TimeEntry | null
  'time:create': (data: Partial<TimeEntry>) => TimeEntry
  'time:update': (id: number, data: Partial<TimeEntry>) => TimeEntry
  'time:delete': (id: number) => void
  'time:start': (projectId: number, description?: string, is_billable?: number) => TimeEntry
  'time:stop': (id: number) => TimeEntry | null
  'time:active': () => TimeEntry | null
  'time:unbilled': (projectId: number) => TimeEntry[]
  'time:unbilled-multi': (projectIds: number[]) => TimeEntry[]
  'time:unbilled-by-client': (clientId: number) => TimeEntry[]
  // Invoices
  'invoices:list': (status?: string) => Invoice[]
  'invoices:get': (id: number) => Invoice | null
  'invoices:create': (data: any) => Invoice
  'invoices:update': (id: number, data: Partial<Invoice>) => Invoice
  'invoices:delete': (id: number) => void
  'invoices:export-pdf': (id: number) => string | null
  // Dashboard
  'dashboard:stats': () => DashboardStats
  'dashboard:recent': () => TimeEntry[]
  // Settings
  'settings:get': () => Settings
  'settings:update': (data: Partial<Settings>) => Settings
  'settings:export-db': () => string | null
  'settings:import-db': () => boolean | null
  // Reports
  'reports:hours-by-project': (startDate: string, endDate: string) => any[]
  'reports:hours-by-client': (startDate: string, endDate: string) => any[]
  'reports:earnings-by-month': (startDate: string, endDate: string) => any[]
  'reports:export-csv': (data: any[], filename: string) => string | null
  // Dialog
  'dialog:open-file': (options: any) => string | null
  'dialog:save-file': (options: any) => string | null
  // Tax
  'tax:get-settings': () => TaxSettings
  'tax:save-settings': (data: Partial<TaxSettings>) => TaxSettings
  'tax:get-overview': (taxYear: number) => TaxOverview
  'tax:export-summary-pdf': (taxYear: number) => string | null
  'tax:export-invoices-csv': (taxYear: number) => string | null
  'tax:export-expenses-csv': (taxYear: number) => string | null
  // Expenses
  'expense:list': (taxYear?: number) => Expense[]
  'expense:get': (id: number) => Expense | null
  'expense:create': (data: Partial<Expense>) => Expense
  'expense:update': (id: number, data: Partial<Expense>) => Expense
  'expense:delete': (id: number) => void
  // Bills
  'bills:list': () => Bill[]
  'bills:get': (id: number) => Bill | null
  'bills:create': (data: any) => Bill
  'bills:update': (id: number, data: Partial<Bill>) => Bill
  'bills:delete': (id: number) => void
  // Subscriptions
  'subscriptions:list': () => Subscription[]
  'subscriptions:get': (id: number) => Subscription | null
  'subscriptions:create': (data: any) => Subscription
  'subscriptions:update': (id: number, data: Partial<Subscription>) => Subscription
  'subscriptions:delete': (id: number) => void
  // Payments
  'payments:list': () => Payment[]
  'payments:create': (data: any) => Payment
  'payments:delete': (id: number) => void
  // Email Imports
  'email-imports:list': () => EmailImport[]
  'email-imports:create': (data: any) => EmailImport
  'email-imports:update-status': (id: number, status: string) => EmailImport
  // Candidates
  'candidates:list': (reviewStatus?: string) => BillImportCandidate[]
  'candidates:get': (id: number) => BillImportCandidate | null
  'candidates:create': (data: any) => BillImportCandidate
  'candidates:update': (id: number, data: Partial<BillImportCandidate>) => BillImportCandidate
  'candidates:delete': (id: number) => void
  'candidates:parse-text': (text: string, subject: string, sender: string) => Promise<BillImportCandidate>
  // Automation Rules
  'automation-rules:list': () => AutomationRule[]
  'automation-rules:create': (data: any) => AutomationRule
  'automation-rules:update': (id: number, data: Partial<AutomationRule>) => AutomationRule
  'automation-rules:delete': (id: number) => void
  // Budgets
  'budgets:categories-list': () => BudgetCategory[]
  'budgets:category-create': (name: string, color?: string) => BudgetCategory | null
  'budgets:category-delete': (id: number) => void
  'budgets:monthly-list': (month: string) => MonthlyBudget[]
  'budgets:monthly-set': (categoryId: number, month: string, limit: number) => MonthlyBudget
  // Gmail OAuth
  'gmail:connect': () => Promise<string>
  'gmail:disconnect': () => void
  'gmail:status': () => { connected: boolean; email: string; clientId: string; clientSecret: string }
  'gmail:sync': (daysRange?: number) => Promise<{ fetched: number; skipped: number }>
}
