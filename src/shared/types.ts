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
  // Joined fields
  client_name?: string
  client_company?: string
  client_email?: string
  client_address?: string
  project_name?: string
  items?: InvoiceItem[]
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
}

export interface DashboardStats {
  hours_this_week: number
  hours_this_month: number
  unbilled_hours: number
  outstanding_total: number
  paid_total: number
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
  'time:stop': (id: number) => TimeEntry
  'time:active': () => TimeEntry | null
  // Invoices
  'invoices:list': (status?: string) => Invoice[]
  'invoices:get': (id: number) => Invoice | null
  'invoices:create': (data: any) => Invoice
  'invoices:update': (id: number, data: Partial<Invoice>) => Invoice
  'invoices:delete': (id: number) => void
  'invoices:export-pdf': (id: number) => string
  // Dashboard
  'dashboard:stats': () => DashboardStats
  'dashboard:recent': () => TimeEntry[]
  // Settings
  'settings:get': () => Settings
  'settings:update': (data: Partial<Settings>) => Settings
  'settings:export-db': () => string
  'settings:import-db': (path: string) => void
  // Reports
  'reports:hours-by-project': (startDate: string, endDate: string) => any[]
  'reports:hours-by-client': (startDate: string, endDate: string) => any[]
  'reports:earnings-by-month': (startDate: string, endDate: string) => any[]
  'reports:export-csv': (data: any[], filename: string) => string
  // Dialog
  'dialog:open-file': (options: any) => string | null
  'dialog:save-file': (options: any) => string | null
}
