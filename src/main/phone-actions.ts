import * as db from './database'
import type { TimerManager } from './timer-manager'
import { EXPENSE_CATEGORIES, paymentMethodNames } from './phone-snapshot'

// Changes made on the phone, applied to Billable (actions schema version 1).
// Everything here arrives over the network, so each payload is checked field
// by field before it touches the database. A change that can't be applied is
// rejected with a short reason the phone shows.

export interface IncomingAction {
  id: string
  kind: string
  payload: unknown
  occurredAt: string
  createdAt?: string
}

export interface ActionResult {
  status: 'applied' | 'rejected'
  summary: string
  message?: string
  createdBillableId?: string | null
}

class Reject extends Error {}
function reject(message: string): never { throw new Reject(message) }

// ---------- Field checks ----------

type Obj = Record<string, unknown>

function obj(value: unknown, allowed: string[]): Obj {
  if (!value || typeof value !== 'object' || Array.isArray(value)) reject("Billable couldn't read this change.")
  const o = value as Obj
  for (const key of Object.keys(o)) if (!allowed.includes(key)) reject("Billable couldn't read this change.")
  return o
}

const isId = (v: unknown): v is string => typeof v === 'string' && /^\d{1,12}$/.test(v)
const isUuid = (v: unknown): v is string => typeof v === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v)

function day(v: unknown): string {
  if (typeof v !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(v) || !Number.isFinite(Date.parse(`${v}T00:00:00`))) {
    reject("Billable couldn't read the date.")
  }
  return v as string
}

function optionalNote(v: unknown): string | undefined {
  if (v === undefined || v === null) return undefined
  if (typeof v !== 'string' || v.length > 500) reject('That note is too long.')
  return (v as string).trim()
}

function num(v: unknown, min: number, max: number, what: string): number {
  if (typeof v !== 'number' || !Number.isFinite(v) || v < min || v > max) reject(`Billable couldn't read the ${what}.`)
  return v as number
}

// ---------- Lookups ----------

function project(ref: unknown): any {
  const r = obj(ref, ['billableId', 'actionId'])
  let projectId: number | null = null
  if (isId(r.billableId) && r.actionId === undefined) {
    projectId = Number(r.billableId)
  } else if (isUuid(r.actionId) && r.billableId === undefined) {
    // A job created on the phone: find what Billable made for it
    const row = db.getDatabase()
      .prepare('SELECT status, created_billable_id FROM phone_actions WHERE id = ?')
      .get(String(r.actionId).toLowerCase()) as any
    if (!row) reject("The new job hasn't reached Billable yet.")
    if (row.status !== 'applied' || !row.created_billable_id) reject("The new job wasn't created, so this couldn't be added.")
    projectId = Number(row.created_billable_id)
  } else {
    reject("Billable couldn't tell which job this is for.")
  }
  const p = db.getProject(projectId!)
  if (!p) reject('That job no longer exists in Billable.')
  return p
}

/** The last moment the Mac changed this timer: its start, resume or pause. */
function lastTouched(entry: any): number {
  const start = new Date(entry.active_since || entry.start_time).getTime()
  const paused = entry.paused_at ? new Date(entry.paused_at).getTime() : 0
  return Math.max(start || 0, paused || 0)
}

const fmtMinutes = (m: number) => {
  const h = Math.floor(m / 60)
  const mm = Math.round(m % 60)
  return h ? (mm ? `${h}h ${mm}m` : `${h}h`) : `${mm}m`
}

const fmtMoney = (n: number) => `$${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`

// ---------- Applying ----------

/**
 * Apply one action. Timer changes happen at the moment they were tapped
 * (occurredAt), so a timer started on the phone at 9:00 counts from 9:00
 * even if the Mac was asleep until noon.
 */
export function applyAction(action: IncomingAction, timer: TimerManager): ActionResult {
  const now = Date.now()
  let at = new Date(Date.parse(action.occurredAt))
  if (!Number.isFinite(at.getTime())) at = new Date(now)
  // A phone clock that runs ahead can't put work in the future
  if (at.getTime() > now) at = new Date(now)

  try {
    return run(action, at, timer)
  } catch (err) {
    if (err instanceof Reject) return { status: 'rejected', summary: describeKind(action.kind), message: err.message }
    console.error('[phone] action failed', action.kind, err)
    return { status: 'rejected', summary: describeKind(action.kind), message: 'Billable hit a problem applying this change.' }
  }
}

function describeKind(kind: string): string {
  switch (kind) {
    case 'timer.start': return 'Start a timer'
    case 'timer.pause': return 'Pause the timer'
    case 'timer.resume': return 'Resume the timer'
    case 'timer.stop': return 'Stop the timer'
    case 'time.add': return 'Log time'
    case 'project.create': return 'Create a job'
    case 'project.setStatus': return 'Change a job'
    case 'expense.add': return 'Add an expense'
    case 'invoice.markSent': return 'Mark an invoice sent'
    case 'invoice.markPaid': return 'Record a payment'
    default: return 'Unknown change'
  }
}

function run(action: IncomingAction, at: Date, timer: TimerManager): ActionResult {
  const payload = action.payload ?? {}

  switch (action.kind) {
    case 'timer.start': {
      const p = obj(payload, ['project', 'note'])
      const job = project(p.project)
      const text = optionalNote(p.note) || ''
      const running = timer.getActive()
      const paused = timer.getPaused()
      const current = running || paused
      if (current && lastTouched(current) > at.getTime()) {
        reject('A timer was started on your Mac after this, so Billable kept that one.')
      }
      const entry = timer.start(job.id, text, at)
      return { status: 'applied', summary: `Started a timer on ${job.name}`, createdBillableId: entry ? String(entry.id) : null }
    }

    case 'timer.pause': {
      obj(payload, [])
      const running = timer.getActive()
      if (!running) reject('No timer was running on your Mac.')
      if (lastTouched(running) > at.getTime()) reject('The timer on your Mac started after this.')
      timer.pause(at)
      return { status: 'applied', summary: `Paused the timer on ${running.project_name || 'a job'}` }
    }

    case 'timer.resume': {
      obj(payload, [])
      const paused = timer.getPaused()
      if (!paused) reject('No timer was paused on your Mac.')
      if (lastTouched(paused) > at.getTime()) reject('The timer on your Mac was paused after this.')
      timer.resume(at)
      return { status: 'applied', summary: `Resumed the timer on ${paused.project_name || 'a job'}` }
    }

    case 'timer.stop': {
      const p = obj(payload, ['note'])
      const text = optionalNote(p.note)
      const current = timer.getActive() || timer.getPaused()
      if (!current) reject('No timer was running on your Mac.')
      if (lastTouched(current) > at.getTime()) reject('The timer on your Mac changed after this.')
      if (text !== undefined) db.updateTimeEntry(current.id, { description: text })
      const entry = timer.stop(at) as any
      if (entry?.discarded) {
        return { status: 'applied', summary: `Stopped the timer on ${current.project_name || 'a job'}`, message: 'It ran under a minute, so it wasn’t kept.' }
      }
      return {
        status: 'applied',
        summary: `Stopped the timer on ${current.project_name || 'a job'} at ${fmtMinutes(Number(entry?.duration_minutes) || 0)}`,
        createdBillableId: entry ? String(entry.id) : null,
      }
    }

    case 'time.add': {
      const p = obj(payload, ['project', 'date', 'startTime', 'minutes', 'note', 'billable'])
      const job = project(p.project)
      const date = day(p.date)
      let startTime = '09:00'
      if (p.startTime !== undefined && p.startTime !== null) {
        if (typeof p.startTime !== 'string' || !/^([01]\d|2[0-3]):[0-5]\d$/.test(p.startTime)) reject("Billable couldn't read the start time.")
        startTime = p.startTime as string
      }
      const mins = num(p.minutes, 1, 1440, 'duration')
      if (!Number.isInteger(mins)) reject("Billable couldn't read the duration.")
      if (typeof p.billable !== 'boolean') reject("Billable couldn't tell if this time is billable.")
      const start = new Date(`${date}T${startTime}:00`)
      const end = new Date(start.getTime() + mins * 60_000)
      const entry = db.createTimeEntry({
        project_id: job.id,
        description: optionalNote(p.note) || '',
        start_time: start.toISOString(),
        end_time: end.toISOString(),
        duration_minutes: mins,
        is_billable: p.billable ? 1 : 0,
      }) as any
      return { status: 'applied', summary: `Logged ${fmtMinutes(mins)} on ${job.name}`, createdBillableId: String(entry.id) }
    }

    case 'project.create': {
      const p = obj(payload, ['clientBillableId', 'name', 'rate'])
      if (!isId(p.clientBillableId)) reject("Billable couldn't tell which client this is for.")
      const client = db.getClient(Number(p.clientBillableId)) as any
      if (!client) reject('That client no longer exists in Billable.')
      if (typeof p.name !== 'string' || !p.name.trim() || p.name.trim().length > 120) reject('The job needs a name up to 120 characters.')
      const settings = db.getSettings() || {}
      const rate = p.rate === undefined || p.rate === null
        ? (Number(client.default_rate) || Number(settings.default_rate) || 0)
        : num(p.rate, 0, 100000, 'rate')
      const created = db.createProject({ client_id: client.id, name: (p.name as string).trim(), rate, status: 'active' }) as any
      return { status: 'applied', summary: `Created the job ${created.name} for ${client.name}`, createdBillableId: String(created.id) }
    }

    case 'project.setStatus': {
      const p = obj(payload, ['project', 'status'])
      const job = project(p.project)
      if (!['active', 'paused', 'complete', 'archived'].includes(p.status as string)) reject("Billable couldn't read the job status.")
      db.updateProject(job.id, { status: p.status })
      const label: Record<string, string> = { active: 'active', paused: 'paused', complete: 'complete', archived: 'archived' }
      return { status: 'applied', summary: `Marked ${job.name} ${label[p.status as string]}` }
    }

    case 'expense.add': {
      const p = obj(payload, ['date', 'category', 'amount', 'description'])
      const date = day(p.date)
      const category = EXPENSE_CATEGORIES.find(c => c.id === p.category)
      if (!category) reject("Billable doesn't have that expense category.")
      const amount = Math.round(num(p.amount, 0.01, 100000, 'amount') * 100) / 100
      const created = db.createExpense({ date, category: category!.id, amount, description: optionalNote(p.description) || '' }) as any
      return { status: 'applied', summary: `Added a ${fmtMoney(amount)} ${category!.label.toLowerCase()} expense`, createdBillableId: String(created.id) }
    }

    case 'invoice.markSent': {
      const p = obj(payload, ['invoiceBillableId', 'sentDate'])
      if (!isId(p.invoiceBillableId)) reject("Billable couldn't tell which invoice this is.")
      const sentDate = day(p.sentDate)
      const inv = db.getInvoice(Number(p.invoiceBillableId)) as any
      if (!inv) reject('That invoice no longer exists in Billable.')
      if (inv.stored_status !== 'draft') {
        return { status: 'applied', summary: `${inv.invoice_number} was already sent` }
      }
      db.markInvoicesSent([inv.id], sentDate)
      return { status: 'applied', summary: `Marked ${inv.invoice_number} sent` }
    }

    case 'invoice.markPaid': {
      const p = obj(payload, ['invoiceBillableIds', 'paidDate', 'method'])
      const ids = p.invoiceBillableIds
      if (!Array.isArray(ids) || ids.length < 1 || ids.length > 50 || !ids.every(isId)) reject("Billable couldn't tell which invoices were paid.")
      const paidDate = day(p.paidDate)
      if (typeof p.method !== 'string' || !paymentMethodNames().includes(p.method)) reject("Billable doesn't have that payment method.")
      const invoices = (ids as string[]).map(v => db.getInvoice(Number(v)) as any)
      if (invoices.some(i => !i)) reject('One of those invoices no longer exists in Billable.')
      const open = invoices.filter(i => i.stored_status !== 'paid')
      if (!open.length) return { status: 'applied', summary: 'Those invoices were already paid' }
      db.markInvoicesPaid(open.map(i => i.id), paidDate, p.method as string)
      const total = open.reduce((s, i) => s + (Number(i.total) || 0), 0)
      const names = open.length === 1 ? open[0].invoice_number : `${open.length} invoices`
      return { status: 'applied', summary: `Recorded a ${fmtMoney(total)} payment for ${names}` }
    }

    default:
      return reject('This version of Billable doesn’t know that change. Update Billable on your Mac.')
  }
}
