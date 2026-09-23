export interface Client {
  id: number
  name: string
  company: string
  email: string
  address: string
  default_rate: number
  currency: string
  created_at: string
  // Money totals (computed)
  project_count?: number
  active_project_count?: number
  unbilled_amount?: number
  unbilled_hours?: number
  oldest_unbilled?: string | null
  outstanding_amount?: number
  overdue_amount?: number
  paid_amount?: number
  invoice_count?: number
  last_activity?: string | null
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
  /** Same as invoiced_amount (kept for older code). */
  billed_total?: number
  unbilled_hours?: number
  unbilled_entries?: number
  /** Unbilled hours at the project's current rate. */
  unbilled_amount?: number
  oldest_unbilled?: string | null
  /** Sum of this project's invoice lines, before tax. */
  invoiced_amount?: number
  /** Invoice lines on paid invoices, before tax. */
  paid_amount?: number
  last_activity?: string | null
}

export interface TimeEntry {
  id: number
  project_id: number
  description: string
  start_time: string
  end_time: string | null
  duration_minutes: number
  paused_at?: string | null
  active_since?: string | null
  is_billable: number
  is_invoiced: number
  invoice_id?: number | null
  created_at: string
  // Joined fields
  project_name?: string
  project_color?: string
  project_status?: Project['status']
  client_id?: number
  client_name?: string
  rate?: number
  invoice_number?: string | null
  invoice_status?: InvoiceStatus | null
  billing_state?: BillingState
  /** Set on a stop result when a sub-minute timer was thrown away. */
  discarded?: boolean
}

/** Where a time entry's money is. `invoiced` = flagged by an older version with no invoice link. */
export type BillingState = 'unbilled' | 'nonbillable' | 'draft' | 'sent' | 'overdue' | 'paid' | 'invoiced'

/** Invoice status as shown. Overdue is derived from the due date. */
export type InvoiceStatus = 'draft' | 'sent' | 'overdue' | 'paid'

export interface TimerState {
  active: TimeEntry | null
  paused: TimeEntry | null
}

export interface Invoice {
  id: number
  project_id: number | null
  client_id: number
  invoice_number: string
  issue_date: string
  due_date: string
  status: InvoiceStatus
  stored_status?: 'draft' | 'sent' | 'paid' | 'overdue'
  days_past_due?: number
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
  sent_at?: string | null
  line_style?: LineStyle | null
  // Joined fields
  client_name?: string
  client_company?: string
  client_email?: string
  client_address?: string
  project_name?: string
  entry_count?: number
  entry_hours?: number
  project_count?: number
  items?: InvoiceItem[]
  entries?: TimeEntry[]
}

/** How the invoice builder turns time into lines. */
export type LineStyle = 'entry' | 'project' | 'day' | 'manual'

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
}

export interface InvoiceItem {
  id: number
  invoice_id: number
  description: string
  quantity: number
  unit_price: number
  total: number
  project_id?: number | null
  /** entry:ID, project:ID or day:YYYY-MM-DD:PROJECT for lines built from time. Null for manual lines. */
  source_key?: string | null
  custom_description?: number
}

// ===== Commission tracking (appointment-setting commissions) =====
export type CommissionJobType = 'solar' | 'roofing'

export type CommissionStatus =
  | 'appointment_set'
  | 'appointment_attended'
  | 'closed_waiting'
  | 'paid'
  | 'lost'
  | 'cancelled'
  | 'needs_review'

export type CommissionPaymentStatus = 'unpaid' | 'pending' | 'paid'

export interface Commission {
  id: number
  client_name: string
  job_type: CommissionJobType
  appointment_date: string | null
  closer_name: string
  status: CommissionStatus
  payment_status: CommissionPaymentStatus
  system_size_kw: number | null
  contract_amount: number | null
  /** Rule-based commission (0 when in the roofing review gap). */
  calculated_commission: number
  /** When set, overrides the calculated commission. */
  manual_override: number | null
  /** 1 when roofing is $20,001–$29,999 and no override is set. */
  needs_review: number
  notes: string
  // Invoice tracking
  invoice_id?: number | null
  invoice_status?: CommissionInvoiceStage
  invoiced_at?: string | null
  paid_at?: string | null
  created_at: string
  updated_at: string
}

export type CommissionInvoiceStage = 'not_invoiced' | 'invoiced' | 'paid'
export type CommissionInvoiceCategory = 'solar' | 'roofing' | 'mixed'
export type CommissionInvoiceStatus = 'draft' | 'sent' | 'paid' | 'cancelled'

export interface CommissionInvoice {
  id: number
  invoice_number: string
  category: CommissionInvoiceCategory
  date_from: string | null
  date_to: string | null
  status: CommissionInvoiceStatus
  job_count: number
  total: number
  notes: string
  created_at: string
  updated_at: string
  paid_at: string | null
  jobs?: Commission[]
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
  hours_today: number
  hours_this_week: number
  hours_this_month: number
  unbilled_hours: number
  unbilled_amount: number
  outstanding_total: number
  overdue_total: number
  paid_total: number
  paid_ytd: number
}

export interface BillingPipeline {
  unbilled_amount: number
  unbilled_hours: number
  unbilled_entries: number
  unbilled_projects: number
  draft_amount: number
  draft_count: number
  awaiting_amount: number
  awaiting_count: number
  overdue_amount: number
  overdue_count: number
  paid_ytd_amount: number
  paid_ytd_count: number
}

export interface UnbilledProject {
  project_id: number
  project_name: string
  project_color: string
  project_status: Project['status']
  rate: number
  client_id: number
  client_name: string
  entry_count: number
  hours: number
  amount: number
  oldest: string
  newest: string
  oldest_day: string
  age_days: number
}

export interface UnbilledClient {
  client_id: number
  client_name: string
  amount: number
  hours: number
  entry_count: number
  oldest_day: string
  projects: UnbilledProject[]
}

export type AttentionKind = 'overdue' | 'unbilled' | 'draft' | 'accidental_timer' | 'duplicate_clients'

export interface AttentionItem {
  key: string
  kind: AttentionKind
  tone: 'danger' | 'warning' | 'neutral'
  title: string
  detail: string
  amount?: number
  invoice_id?: number
  project_id?: number
  client_id?: number
  entry_id?: number
  closed?: boolean
  oldest_day?: string
  client_ids?: number[]
  client_names?: string[]
}

export interface BillingOverview {
  today: string
  pipeline: BillingPipeline
  ready_to_bill: UnbilledClient[]
  attention: AttentionItem[]
  invoices: Invoice[]
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
  'time:start': (projectId: number, description?: string) => TimeEntry
  'time:stop': () => TimeEntry | null
  'time:pause': () => TimeEntry | null
  'time:resume': () => TimeEntry | null
  'time:active': () => TimeEntry | null
  'time:state': () => TimerState
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
  // Commissions
  'commissions:list': () => Commission[]
  'commissions:get': (id: number) => Commission | null
  'commissions:create': (data: Partial<Commission>) => Commission
  'commissions:update': (id: number, data: Partial<Commission>) => Commission
  'commissions:patch': (id: number, patch: Partial<Commission>) => Commission
  'commissions:bulk-patch': (ids: number[], patch: Partial<Commission>) => Commission[]
  'commissions:delete': (id: number) => void
  // Commission invoices
  'commission-invoices:list': () => CommissionInvoice[]
  'commission-invoices:get': (id: number) => CommissionInvoice | null
  'commission-invoices:create': (data: any) => CommissionInvoice
  'commission-invoices:update-status': (id: number, status: string) => CommissionInvoice
  'commission-invoices:delete': (id: number) => void
  'commission-invoices:export-pdf': (id: number) => string | null
  // Expenses
  'expense:list': (taxYear?: number) => Expense[]
  'expense:get': (id: number) => Expense | null
  'expense:create': (data: Partial<Expense>) => Expense
  'expense:update': (id: number, data: Partial<Expense>) => Expense
  'expense:delete': (id: number) => void
}
