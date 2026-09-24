import * as db from './database'
import type { TimerManager } from './timer-manager'

// The copy of Billable that Content HQ shows on the phone (snapshot schema
// version 1). Content HQ never does billing math, so every figure here comes
// from the same queries Billable's own pages use. Client emails, addresses,
// invoice lines and payment details stay on the Mac.

export const EXPENSE_CATEGORIES: Array<{ id: string; label: string }> = [
  { id: 'equipment', label: 'Equipment' },
  { id: 'software', label: 'Software' },
  { id: 'home_office', label: 'Home office' },
  { id: 'phone_internet', label: 'Phone and internet' },
  { id: 'travel', label: 'Travel' },
  { id: 'meals', label: 'Meals' },
  { id: 'professional_development', label: 'Professional development' },
  { id: 'other', label: 'Other' },
]

const RECENT_DAYS = 60
const MAX_ENTRIES = 5000
const MAX_INVOICES = 2000
const MAX_EXPENSES = 5000
const MAX_CLIENTS = 1000
const MAX_PROJECTS = 2000
const MAX_NOTE = 500
const MAX_MONEY = 10_000_000

const pad2 = (n: number) => String(n).padStart(2, '0')

/** ISO 8601 in the Mac's timezone, with its offset: 2026-09-24T18:00:00-06:00 */
export function isoLocal(d: Date): string {
  const off = -d.getTimezoneOffset()
  const sign = off >= 0 ? '+' : '-'
  const abs = Math.abs(off)
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}` +
    `T${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}` +
    `${sign}${pad2(Math.floor(abs / 60))}:${pad2(abs % 60)}`
}

/** Stored times come as ISO strings, SQLite UTC "YYYY-MM-DD HH:MM:SS", or plain dates. */
function parseStored(value: unknown): Date | null {
  if (!value) return null
  const s = String(value)
  let ms: number
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) ms = Date.parse(`${s}T00:00:00`) // a local day
  else if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}/.test(s)) ms = Date.parse(`${s.replace(' ', 'T')}Z`)
  else ms = Date.parse(s)
  return Number.isFinite(ms) ? new Date(ms) : null
}

const timeOrNull = (value: unknown) => {
  const d = parseStored(value)
  return d ? isoLocal(d) : null
}

const dayOrNull = (value: unknown) => {
  const s = String(value || '').slice(0, 10)
  return /^\d{4}-\d{2}-\d{2}$/.test(s) ? s : null
}

const money = (n: unknown) => {
  const v = Math.round((Number(n) || 0) * 100) / 100
  return Math.min(MAX_MONEY, Math.max(0, v))
}

const hours = (n: unknown) => Math.max(0, Math.round((Number(n) || 0) * 100) / 100)
const minutes = (n: unknown) => Math.min(1440, Math.max(0, Math.round((Number(n) || 0) * 100) / 100))
/** Text that can be empty goes as null, as Content HQ's contract asks. */
const note = (s: unknown) => String(s ?? '').trim().slice(0, MAX_NOTE) || null
const id = (n: unknown) => String(n)
/** Content HQ requires names to be non-empty and caps their length; one odd row must not stop every sync. */
const label = (s: unknown, max: number, fallback: string) => (String(s ?? '').trim() || fallback).slice(0, max)

const PROJECT_STATUSES = new Set(['active', 'paused', 'complete', 'archived'])
const INVOICE_STATUSES = new Set(['draft', 'sent', 'overdue', 'paid'])

export function paymentMethodNames(): string[] {
  const settings = db.getSettings() || {}
  let names: string[] = []
  try {
    names = (JSON.parse(settings.payment_methods || '[]') as Array<{ name?: string }>)
      .map(m => String(m?.name || '').trim().slice(0, 60))
      .filter(Boolean)
  } catch { /* fall through to the default */ }
  if (!names.length) names = [String(settings.default_payment_method || 'e-Transfer').slice(0, 60)]
  return Array.from(new Set(names)).slice(0, 20)
}

export function snapshotCurrency(): string {
  const settings = db.getSettings() || {}
  const code = String(settings.default_currency || '').toUpperCase()
  return /^[A-Z]{3}$/.test(code) ? code : 'CAD'
}

function timerSnapshot(timer: TimerManager) {
  const running = timer.getActive()
  const paused = timer.getPaused()
  const e = running || paused
  if (!e) return null
  return {
    entryId: id(e.id),
    projectBillableId: id(e.project_id),
    note: note(e.description),
    startedAt: timeOrNull(e.start_time)!,
    state: running ? 'running' : 'paused',
    accumulatedMinutes: Math.max(0, Math.round((Number(e.duration_minutes) || 0) * 100) / 100),
    activeSince: running ? timeOrNull(e.active_since || e.start_time) : null,
  }
}

export function buildSnapshot(timer: TimerManager) {
  const now = new Date()
  const today = db.localToday()
  const month = today.slice(0, 7)
  const stats = db.getDashboardStats()
  const overview = db.getBillingOverview()
  const p = overview.pipeline
  const allInvoices = overview.invoices as any[]

  const paidThisMonth = allInvoices
    .filter(i => i.status === 'paid' && String(i.payment_date || i.issue_date).startsWith(month))
    .reduce((s, i) => s + (Number(i.total) || 0), 0)

  const clients = (db.listClients() as any[]).slice(0, MAX_CLIENTS).map(c => ({
    billableId: id(c.id),
    name: label(c.name, 200, 'Unnamed client'),
    company: String(c.company ?? '').trim().slice(0, 200) || null,
    rate: money(c.default_rate),
    unbilledAmount: money(c.unbilled_amount),
    outstandingAmount: money(c.outstanding_amount),
    overdueAmount: money(c.overdue_amount),
  }))

  const projects = (db.listProjects() as any[]).slice(0, MAX_PROJECTS).map(pr => ({
    billableId: id(pr.id),
    clientBillableId: id(pr.client_id),
    name: label(pr.name, 200, 'Untitled job'),
    status: PROJECT_STATUSES.has(pr.status) ? pr.status : 'active',
    rate: money(pr.rate),
    color: /^#[0-9a-fA-F]{6}$/.test(pr.color || '') ? pr.color : '#F5A623',
    trackedHours: hours(pr.total_hours),
    unbilledHours: hours(pr.unbilled_hours),
    unbilledAmount: money(pr.unbilled_amount),
    lastActivityAt: timeOrNull(pr.last_activity),
  }))

  // Every unbilled entry, plus everything from the last 60 days
  const since = new Date(now)
  since.setHours(0, 0, 0, 0)
  since.setDate(since.getDate() - RECENT_DAYS)
  const entries = (db.listTimeEntries() as any[])
    .filter(e => e.end_time)
    .filter(e => e.billing_state === 'unbilled' || (parseStored(e.start_time)?.getTime() ?? 0) >= since.getTime())
    .slice(0, MAX_ENTRIES)
    .map(e => {
      const start = parseStored(e.start_time)
      return {
        billableId: id(e.id),
        projectBillableId: id(e.project_id),
        date: start ? db.localDate(start) : today,
        startedAt: start ? isoLocal(start) : isoLocal(now),
        endedAt: timeOrNull(e.end_time),
        minutes: minutes(e.duration_minutes),
        note: note(e.description),
        billable: !!e.is_billable,
        invoiced: !!(e.invoice_id || e.is_invoiced),
      }
    })

  const invoiceProjects = new Map<number, Set<string>>()
  for (const row of db.getDatabase().prepare(
    'SELECT DISTINCT invoice_id, project_id FROM invoice_items WHERE project_id IS NOT NULL',
  ).all() as any[]) {
    if (!invoiceProjects.has(row.invoice_id)) invoiceProjects.set(row.invoice_id, new Set())
    invoiceProjects.get(row.invoice_id)!.add(id(row.project_id))
  }
  const invoices = allInvoices.slice(0, MAX_INVOICES).map(inv => {
    const projectIds = new Set(invoiceProjects.get(inv.id) || [])
    if (inv.project_id) projectIds.add(id(inv.project_id))
    return {
      billableId: id(inv.id),
      number: label(inv.invoice_number, 60, `Invoice ${inv.id}`),
      clientBillableId: id(inv.client_id),
      projectBillableIds: Array.from(projectIds).slice(0, 200),
      status: INVOICE_STATUSES.has(inv.status) ? inv.status : 'draft',
      issueDate: dayOrNull(inv.issue_date) || today,
      dueDate: dayOrNull(inv.due_date) || dayOrNull(inv.issue_date) || today,
      sentAt: timeOrNull(inv.sent_at),
      paidDate: inv.status === 'paid' ? dayOrNull(inv.payment_date) : null,
      total: money(inv.total),
      hours: hours(inv.entry_hours),
    }
  })

  const year = now.getFullYear()
  const expenses = [...(db.listExpenses(year) as any[]), ...(db.listExpenses(year - 1) as any[])]
    .slice(0, MAX_EXPENSES)
    .map(x => ({
      billableId: id(x.id),
      date: dayOrNull(x.date) || today,
      category: EXPENSE_CATEGORIES.some(c => c.id === x.category) ? x.category : 'other',
      description: note(x.description),
      amount: money(x.amount),
    }))

  return {
    generatedAt: isoLocal(now),
    currency: snapshotCurrency(),
    timer: timerSnapshot(timer),
    totals: {
      unbilledAmount: money(p.unbilled_amount),
      unbilledHours: hours(p.unbilled_hours),
      awaitingAmount: money(p.awaiting_amount),
      awaitingCount: p.awaiting_count,
      overdueAmount: money(p.overdue_amount),
      overdueCount: p.overdue_count,
      paidThisMonth: money(paidThisMonth),
      paidThisYear: money(p.paid_ytd_amount),
      hoursToday: hours(stats.hours_today),
      hoursThisWeek: hours(stats.hours_this_week),
    },
    clients,
    projects,
    entries,
    invoices,
    expenses,
    expenseCategories: EXPENSE_CATEGORIES,
    paymentMethods: paymentMethodNames(),
  }
}

export type Snapshot = ReturnType<typeof buildSnapshot>
