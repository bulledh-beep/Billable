import { useState, useEffect, useMemo } from 'react'
import { useNavigate, useSearchParams, useParams } from 'react-router-dom'
import { Trash2, RotateCcw, Lock, Clock, AlertTriangle } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Segmented from '../components/Segmented'
import EmptyState from '../components/EmptyState'
import NumberField from '../components/NumberField'
import { formatMoney, formatDay, formatDurationShort, formatHoursShort, todayISO, addDays, toLocalISODate } from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import type { Client, Invoice, InvoiceItem, LineStyle, PaymentMethod, Project, TaxSettings, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

type GeneratedStyle = Exclude<LineStyle, 'manual'>

interface Line {
  key: string
  description: string
  quantity: number
  unit_price: number
  project_id: number | null
  source_key: string | null
  generated: boolean
  defaultDescription?: string
  defaultRate?: number
  custom?: boolean
  fixedTotal?: number
}

const lineTotal = (l: { quantity: number; unit_price: number; fixedTotal?: number }) =>
  l.fixedTotal ?? round2(l.quantity * l.unit_price)

interface ManualLine {
  key: string
  description: string
  quantity: number
  unit_price: number
  /** Saved amount from an existing invoice, kept until hours or rate change. */
  fixedTotal?: number
  project_id?: number | null
}

const STYLE_KEY = 'billable.lineStyle'
const round2 = (n: number) => Math.round(n * 100) / 100
const entryDay = (e: TimeEntry) => toLocalISODate(new Date(e.start_time))
const newKey = () => `m${Math.random().toString(36).slice(2, 9)}`

function readStyle(): GeneratedStyle {
  try {
    const v = localStorage.getItem(STYLE_KEY)
    if (v === 'entry' || v === 'project' || v === 'day') return v
  } catch { /* ignore */ }
  return 'entry'
}

/** Turn selected time into invoice lines. Overrides keep edited descriptions and rates. */
function buildLines(
  entries: TimeEntry[],
  style: GeneratedStyle,
  overrides: Record<string, { description?: string; unit_price?: number }>,
): Line[] {
  const multiProject = new Set(entries.map(e => e.project_id)).size > 1
  const sorted = [...entries].sort((a, b) => a.start_time.localeCompare(b.start_time))
  const groups = new Map<string, { entries: TimeEntry[]; description: string; project_id: number; rate: number }>()

  for (const e of sorted) {
    const day = entryDay(e)
    let key: string
    let description: string
    if (style === 'entry') {
      key = `entry:${e.id}`
      const what = e.description?.trim() || e.project_name || 'Work'
      description = `${formatDay(day)} · ${multiProject && e.description ? `${e.project_name}: ` : ''}${what}`
    } else if (style === 'project') {
      key = `project:${e.project_id}`
      description = e.project_name || 'Work'
    } else {
      key = `day:${day}:${e.project_id}`
      description = `${formatDay(day)} · ${e.project_name}`
    }
    const g = groups.get(key)
    if (g) g.entries.push(e)
    else groups.set(key, { entries: [e], description, project_id: e.project_id, rate: e.rate || 0 })
  }

  return Array.from(groups.entries()).map(([key, g]) => {
    let defaultDescription = g.description
    if (style === 'project') {
      const first = entryDay(g.entries[0])
      const last = entryDay(g.entries[g.entries.length - 1])
      defaultDescription = first === last ? `${g.description} · ${formatDay(first)}` : `${g.description} · ${formatDay(first)} – ${formatDay(last)}`
    } else if (style === 'day') {
      const notes = Array.from(new Set(g.entries.map(e => e.description?.trim()).filter(Boolean)))
      if (notes.length) defaultDescription = `${g.description}: ${notes.join('; ')}`
    }
    const minutes = g.entries.reduce((s, e) => s + e.duration_minutes, 0)
    const o = overrides[key] || {}
    return {
      key,
      source_key: key,
      generated: true,
      project_id: g.project_id,
      defaultDescription,
      description: o.description ?? defaultDescription,
      custom: o.description !== undefined && o.description !== defaultDescription,
      quantity: round2(minutes / 60),
      unit_price: o.unit_price ?? g.rate,
      defaultRate: g.rate,
    }
  })
}

export default function InvoiceCreate() {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const { id: editIdParam } = useParams()
  const editId = editIdParam ? parseInt(editIdParam) : null
  const isEdit = !!editId

  const [loaded, setLoaded] = useState(false)
  const [existing, setExisting] = useState<Invoice | null>(null)
  const [clients, setClients] = useState<Client[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [taxSettings, setTaxSettings] = useState<TaxSettings | null>(null)
  const [paymentMethods, setPaymentMethods] = useState<PaymentMethod[]>([])

  const [clientId, setClientId] = useState(0)
  const [issueDate, setIssueDate] = useState(todayISO())
  const [terms, setTerms] = useState('30')
  const [dueDate, setDueDate] = useState(addDays(todayISO(), 30))
  const [paymentMethod, setPaymentMethod] = useState('')
  const [notes, setNotes] = useState('')
  const [gstApplicable, setGstApplicable] = useState(false)
  const [gstRate, setGstRate] = useState(0)
  const [gstNumber, setGstNumber] = useState('')
  const [taxRate, setTaxRate] = useState(0)

  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [billThrough, setBillThrough] = useState('')
  const [style, setStyle] = useState<LineStyle>(readStyle())
  const [overrides, setOverrides] = useState<Record<string, { description?: string; unit_price?: number }>>({})
  const [manual, setManual] = useState<ManualLine[]>([])
  const [legacyLines, setLegacyLines] = useState<ManualLine[]>([])
  const [saving, setSaving] = useState(false)

  // ---- Load ----
  useEffect(() => { init() }, [editId])

  const init = async () => {
    const [c, p, s, tax] = await Promise.all([
      window.api.clients.list(),
      window.api.projects.list(),
      window.api.settings.get(),
      window.api.tax.getSettings(),
    ])
    setClients(c)
    setProjects(p)
    setTaxSettings(tax)
    let methods: PaymentMethod[] = []
    try { methods = JSON.parse(s.payment_methods || '[]') } catch { /* ignore */ }
    setPaymentMethods(methods)

    if (editId) {
      const inv: Invoice = await window.api.invoices.get(editId)
      if (!inv) { toast.error('Invoice not found'); navigate('/billing'); return }
      setExisting(inv)
      setClientId(inv.client_id)
      setIssueDate(String(inv.issue_date).slice(0, 10))
      setDueDate(String(inv.due_date).slice(0, 10))
      setTerms('custom')
      setNotes(inv.notes || '')
      setTaxRate(inv.tax_rate || 0)
      setGstApplicable(!!inv.gst_hst_applicable)
      setGstRate(inv.gst_hst_rate || 0)
      setGstNumber(inv.gst_hst_number || tax?.gst_hst_number || '')
      const match = methods.find(m => (inv.notes || '').includes(m.name))
      if (match) setPaymentMethod(match.name)

      const items = inv.items || []
      const linked = (inv.entries || []).map(e => e.id)
      const lineStyle: LineStyle = (inv.line_style as LineStyle) || 'manual'
      setStyle(lineStyle)
      setSelected(new Set(linked))
      if (lineStyle === 'manual') {
        const lines = items.map(it => ({ key: newKey(), description: it.description, quantity: it.quantity, unit_price: it.unit_price, fixedTotal: it.total, project_id: it.project_id ?? null }))
        setManual(lines)
        setLegacyLines(lines)
      } else {
        const ov: Record<string, { description?: string; unit_price?: number }> = {}
        for (const it of items as InvoiceItem[]) {
          if (!it.source_key) continue
          ov[it.source_key] = { unit_price: it.unit_price, ...(it.custom_description ? { description: it.description } : {}) }
        }
        setOverrides(ov)
        setManual(items.filter(it => !it.source_key).map(it => ({ key: newKey(), description: it.description, quantity: it.quantity, unit_price: it.unit_price, fixedTotal: it.total, project_id: it.project_id ?? null })))
      }
      await loadEntries(inv.client_id, editId, null)
    } else {
      // New invoice defaults
      if (tax?.gst_hst_registered) {
        setGstApplicable(true)
        setGstRate(tax.default_tax_rate || 0)
        setGstNumber(tax.gst_hst_number || '')
      }
      const def = methods.find(m => m.name === s.default_payment_method) || methods[0]
      if (def) { setPaymentMethod(def.name); setNotes(paymentNote(def)) }

      const presetClient = parseInt(searchParams.get('client_id') || '')
      const presetProject = parseInt(searchParams.get('project_id') || '')
      const presetProjects = (searchParams.get('project_ids') || '').split(',').map(Number).filter(Boolean)
      const projectFilter = presetProject ? [presetProject] : presetProjects.length ? presetProjects : null
      const cid = presetClient || (projectFilter ? p.find((x: Project) => x.id === projectFilter[0])?.client_id : 0) || 0
      if (cid) {
        setClientId(cid)
        await loadEntries(cid, null, projectFilter)
      }
    }
    setLoaded(true)
  }

  const loadEntries = async (cid: number, invoiceId: number | null, onlyProjects: number[] | null) => {
    const list: TimeEntry[] = await window.api.time.invoiceable(cid, invoiceId)
    setEntries(list)
    if (!invoiceId) {
      setSelected(new Set(list.filter(e => !onlyProjects || onlyProjects.includes(e.project_id)).map(e => e.id)))
    }
  }

  const paymentNote = (m: PaymentMethod) => (m.email ? `Payment by ${m.name} to ${m.email}` : `Payment by ${m.name}`)

  const changeClient = async (cid: number) => {
    setClientId(cid)
    setOverrides({})
    setBillThrough('')
    if (cid) await loadEntries(cid, editId, null)
    else { setEntries([]); setSelected(new Set()) }
  }

  const changeTerms = (t: string) => {
    setTerms(t)
    if (t !== 'custom') setDueDate(addDays(issueDate, parseInt(t)))
  }

  const changeIssueDate = (d: string) => {
    setIssueDate(d)
    if (terms !== 'custom') setDueDate(addDays(d, parseInt(terms)))
  }

  const changeStyle = (s: LineStyle) => {
    // Older invoices keep their hand-written lines until a line style is picked
    if (s === 'manual') setManual(legacyLines)
    else if (style === 'manual') setManual([])
    setStyle(s)
    setOverrides({})
    if (s !== 'manual') { try { localStorage.setItem(STYLE_KEY, s) } catch { /* ignore */ } }
  }

  const applyBillThrough = (d: string) => {
    setBillThrough(d)
    setSelected(new Set(entries.filter(e => !d || entryDay(e) <= d).map(e => e.id)))
  }

  // ---- Derived ----
  const selectedEntries = useMemo(() => entries.filter(e => selected.has(e.id)), [entries, selected])
  const selectedMinutes = selectedEntries.reduce((s, e) => s + e.duration_minutes, 0)

  const byProject = useMemo(() => {
    const map = new Map<number, { project_id: number; name: string; color: string; rate: number; entries: TimeEntry[] }>()
    for (const e of entries) {
      let g = map.get(e.project_id)
      if (!g) { g = { project_id: e.project_id, name: e.project_name || 'Project', color: e.project_color || '#888', rate: e.rate || 0, entries: [] }; map.set(e.project_id, g) }
      g.entries.push(e)
    }
    for (const g of map.values()) g.entries.sort((a, b) => a.start_time.localeCompare(b.start_time))
    return Array.from(map.values())
  }, [entries])

  const generated: Line[] = useMemo(
    () => (style === 'manual' ? [] : buildLines(selectedEntries, style, overrides)),
    [selectedEntries, style, overrides],
  )

  // Hand-added lines count toward the invoice's project when there is just one
  const selectedProjects = new Set(selectedEntries.map(e => e.project_id))
  const soleProject = selectedProjects.size === 1 ? Array.from(selectedProjects)[0] : (existing?.project_id ?? null)
  const manualLines: Line[] = manual.map(m => ({
    key: m.key, description: m.description, quantity: m.quantity, unit_price: m.unit_price,
    project_id: m.project_id !== undefined ? m.project_id : soleProject,
    source_key: null, generated: false, fixedTotal: m.fixedTotal,
  }))
  const allLines = [...generated, ...manualLines]
  const subtotal = round2(allLines.reduce((s, l) => s + lineTotal(l), 0))
  const gstAmount = gstApplicable ? round2(subtotal * (gstRate / 100)) : 0
  const otherTax = round2(subtotal * (taxRate / 100))
  const total = round2(subtotal + gstAmount + otherTax)
  const currency = taxSettings?.currency || 'CAD'
  const money = (n: number) => formatMoney(n, currency)

  const client = clients.find(c => c.id === clientId)
  // Invoices from before lines were tied to time can keep their hand-written lines
  const legacyEdit = isEdit && !!existing && !existing.line_style
  const sentAlready = existing && existing.status !== 'draft'

  // ---- Selection helpers ----
  const toggleEntry = (id: number) => setSelected(prev => {
    const next = new Set(prev)
    next.has(id) ? next.delete(id) : next.add(id)
    return next
  })
  const toggleProject = (ids: number[], on: boolean) => setSelected(prev => {
    const next = new Set(prev)
    ids.forEach(id => (on ? next.add(id) : next.delete(id)))
    return next
  })

  // ---- Line edits ----
  const editGenerated = (key: string, patch: { description?: string; unit_price?: number }) =>
    setOverrides(o => ({ ...o, [key]: { ...o[key], ...patch } }))
  const resetGenerated = (key: string) => setOverrides(o => { const n = { ...o }; delete n[key]; return n })
  const editManual = (key: string, patch: Partial<ManualLine>) =>
    setManual(list => list.map(m => {
      if (m.key !== key) return m
      // Changing hours or rate means the saved amount no longer applies
      const repriced = 'quantity' in patch || 'unit_price' in patch
      return { ...m, ...patch, ...(repriced ? { fixedTotal: undefined } : {}) }
    }))

  // ---- Save ----
  const save = async (markSent: boolean) => {
    if (!clientId) return toast.error('Choose a client')
    if (allLines.length === 0) return toast.error('Add at least one line')
    if (allLines.some(l => !String(l.description).trim())) return toast.error('Every line needs a description')
    setSaving(true)
    try {
      const items = allLines.map(l => ({
        description: l.description.trim(),
        quantity: l.quantity,
        unit_price: l.unit_price,
        total: lineTotal(l),
        project_id: l.project_id,
        source_key: l.source_key,
        custom_description: l.generated && l.custom ? 1 : 0,
      }))
      const payload: any = {
        client_id: clientId,
        issue_date: issueDate,
        due_date: dueDate,
        subtotal,
        tax_rate: taxRate,
        total,
        notes,
        items,
        entry_ids: Array.from(selected),
        line_style: style,
        currency,
        gst_hst_applicable: gstApplicable ? 1 : 0,
        gst_hst_number: gstApplicable ? gstNumber : null,
        gst_hst_rate: gstApplicable ? gstRate : 0,
        gst_hst_amount: gstAmount,
      }
      if (isEdit && editId) {
        if (markSent && existing?.status === 'draft') payload.status = 'sent'
        await window.api.invoices.update(editId, payload)
        toast.success(`${existing?.invoice_number} saved`)
        notifyBillingChanged()
        navigate(`/invoices/${editId}`)
      } else {
        const inv = await window.api.invoices.create({ ...payload, status: markSent ? 'sent' : 'draft' })
        toast.success(`${inv.invoice_number} created${markSent ? ' and marked as sent' : ''}`)
        notifyBillingChanged()
        navigate(`/invoices/${inv.id}`)
      }
    } catch (err: any) {
      toast.error(`Couldn't save: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  if (!loaded) return null

  const title = isEdit ? `Edit ${existing?.invoice_number || 'invoice'}` : 'New invoice'
  const latestDay = entries.length ? entryDay(entries[entries.length - 1]) : ''

  return (
    <div className="page !pb-28">
      <PageHeader
        crumbs={isEdit && existing ? [{ label: 'Billing', to: '/billing' }, { label: existing.invoice_number, to: `/invoices/${existing.id}` }] : [{ label: 'Billing', to: '/billing' }]}
        title={isEdit ? 'Edit' : 'New invoice'}
      />

      <div className="space-y-5">
        {sentAlready && (
          <div className="flex gap-2.5 p-3 rounded-lg bg-amber/10 text-sm text-fg-2">
            <AlertTriangle className="w-4 h-4 text-amber shrink-0 mt-px" />
            <p>This invoice has been {existing?.status === 'paid' ? 'paid' : 'sent'}. Saving changes what it says, so send the client a fresh copy if they need it.</p>
          </div>
        )}

        {/* Details */}
        <div className="card p-5">
          <div className="grid grid-cols-[minmax(0,1.6fr)_minmax(140px,1fr)_minmax(120px,0.9fr)_minmax(140px,1fr)] gap-4">
            <div>
              <label className="label">Client</label>
              <select className="input" value={clientId} onChange={e => changeClient(parseInt(e.target.value))} disabled={isEdit}>
                <option value={0}>Choose a client</option>
                {clients.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
              </select>
            </div>
            <div>
              <label className="label">Issue date</label>
              <input className="input" type="date" value={issueDate} onChange={e => changeIssueDate(e.target.value)} />
            </div>
            <div>
              <label className="label">Terms</label>
              <select className="input" value={terms} onChange={e => changeTerms(e.target.value)}>
                <option value="0">Due on receipt</option>
                <option value="15">Net 15</option>
                <option value="30">Net 30</option>
                <option value="45">Net 45</option>
                <option value="custom">Custom</option>
              </select>
            </div>
            <div>
              <label className="label">Due date</label>
              <input className="input" type="date" value={dueDate} onChange={e => { setDueDate(e.target.value); setTerms('custom') }} />
            </div>
          </div>
          {client && (
            <p className="text-xs text-fg-3 mt-3">
              Billing {client.name}{client.company ? `, ${client.company}` : ''}{client.email ? ` · ${client.email}` : ''}
              {!client.address && ' · no billing address on file'}
            </p>
          )}
        </div>

        {/* Time to bill */}
        {clientId > 0 && (
          <div className="card">
            <div className="card-header !h-auto py-3 flex-wrap">
              <div>
                <div className="card-title">Time to bill</div>
                <div className="text-xs text-fg-3 mt-0.5">
                  {entries.length === 0
                    ? `${client?.name || 'This client'} has no unbilled time`
                    : <>{selectedEntries.length} of {entries.length} entries · <span className="num">{formatDurationShort(selectedMinutes)}</span></>}
                </div>
              </div>
              {entries.length > 0 && (
                <div className="flex items-center gap-3">
                  <div className="flex items-center gap-2">
                    <label className="text-xs text-fg-3 whitespace-nowrap">Bill through</label>
                    <input className="input w-[150px] h-7 text-xs" type="date" value={billThrough} max={latestDay || undefined} onChange={e => applyBillThrough(e.target.value)} />
                    {billThrough && <button onClick={() => applyBillThrough('')} className="btn-ghost btn-sm">All</button>}
                  </div>
                  <Segmented
                    value={style}
                    onChange={changeStyle}
                    options={[
                      { value: 'entry', label: 'Line per entry' },
                      { value: 'project', label: 'Per project' },
                      { value: 'day', label: 'Per day' },
                      ...(legacyEdit ? [{ value: 'manual' as LineStyle, label: 'Keep current lines' }] : []),
                    ]}
                  />
                </div>
              )}
            </div>

            {legacyEdit && style === 'manual' && entries.length > 0 && (
              <p className="px-4 py-2.5 text-xs text-fg-2 bg-fg/[0.03] border-b border-line">
                This invoice's lines were written before invoices tracked time. Ticking time here links it to the invoice without changing the lines. Pick a line style above to rebuild the lines from the ticked time.
              </p>
            )}

            {entries.length === 0 ? (
              <EmptyState compact icon={Clock} title="Nothing unbilled" description="Add lines by hand below, or track time for this client first." />
            ) : (
              <div className="max-h-[420px] overflow-y-auto">
                {byProject.map(g => {
                  const ids = g.entries.map(e => e.id)
                  const on = ids.filter(id => selected.has(id)).length
                  const mins = g.entries.filter(e => selected.has(e.id)).reduce((s, e) => s + e.duration_minutes, 0)
                  return (
                    <div key={g.project_id} className="border-b border-line last:border-b-0">
                      <label className="flex items-center gap-3 px-4 h-10 bg-panel-2/50 cursor-pointer sticky top-0 z-[1] border-b border-line">
                        <input
                          type="checkbox"
                          checked={on === ids.length}
                          ref={el => { if (el) el.indeterminate = on > 0 && on < ids.length }}
                          onChange={() => toggleProject(ids, on !== ids.length)}
                        />
                        <span className="w-2 h-2 rounded-full" style={{ backgroundColor: g.color }} />
                        <span className="text-sm font-semibold text-fg flex-1 truncate">{g.name}</span>
                        <span className="text-xs text-fg-3 num">{money(g.rate)}/hr</span>
                        <span className="text-xs text-fg-2 num w-16 text-right">{formatDurationShort(mins)}</span>
                        <span className="text-sm num font-medium text-fg w-24 text-right">{money((mins / 60) * g.rate)}</span>
                      </label>
                      {g.entries.map(e => {
                        const day = entryDay(e)
                        const after = !!billThrough && day > billThrough
                        return (
                          <label key={e.id} className={`flex items-center gap-3 pl-9 pr-4 h-9 cursor-pointer hover:bg-fg/[0.02] ${after ? 'opacity-50' : ''}`}>
                            <input type="checkbox" checked={selected.has(e.id)} onChange={() => toggleEntry(e.id)} />
                            <span className="text-xs text-fg-3 num w-24 shrink-0">{formatDay(day)}</span>
                            <span className="text-sm text-fg-2 truncate flex-1">{e.description}</span>
                            {e.invoice_id === editId && editId && <span className="badge bg-fg/[0.06] text-fg-3">On this invoice</span>}
                            <span className="text-xs text-fg-2 num w-16 text-right">{formatDurationShort(e.duration_minutes)}</span>
                            <span className="text-sm num text-fg-2 w-24 text-right">{money((e.duration_minutes / 60) * (e.rate || 0))}</span>
                          </label>
                        )
                      })}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        )}

        {/* Lines */}
        {clientId > 0 && (
          <div className="card">
            <div className="card-header">
              <span className="card-title">Invoice lines</span>
              <button
                onClick={() => setManual(m => [...m, { key: newKey(), description: '', quantity: 1, unit_price: 0 }])}
                className="btn-ghost btn-sm"
              >
                Add line
              </button>
            </div>
            {allLines.length === 0 ? (
              <p className="px-4 py-8 text-center text-sm text-fg-3">Tick time above or add a line.</p>
            ) : (
              <table className="tbl">
                <thead>
                  <tr>
                    {/* Padding matches the text inside the editable fields below */}
                    <th className="!pl-[22px]">Description</th>
                    <th className="text-right w-24">Hours</th>
                    <th className="text-right w-32 !pr-[18px]">Rate</th>
                    <th className="text-right w-28">Amount</th>
                    <th className="w-10"></th>
                  </tr>
                </thead>
                <tbody>
                  {generated.map(l => (
                    <tr key={l.key}>
                      <td className="!py-1.5">
                        <input
                          className="input h-7 border-transparent hover:border-line bg-transparent focus:bg-field"
                          value={l.description}
                          onChange={e => editGenerated(l.key, { description: e.target.value })}
                        />
                      </td>
                      <td className="text-right num text-fg-2">
                        <span className="inline-flex items-center gap-1" title="Hours come from the ticked time">
                          <Lock className="w-3 h-3 text-fg-4" />{l.quantity.toFixed(2)}
                        </span>
                      </td>
                      <td className="!py-1.5">
                        <NumberField
                          className="h-7 text-right border-transparent hover:border-line bg-transparent focus:bg-field"
                          value={l.unit_price}
                          onChange={n => editGenerated(l.key, { unit_price: n })}
                        />
                      </td>
                      <td className="text-right num font-medium text-fg">{money(round2(l.quantity * l.unit_price))}</td>
                      <td>
                        {(l.custom || l.unit_price !== l.defaultRate) && (
                          <button onClick={() => resetGenerated(l.key)} className="btn-icon-sm" title="Reset to the tracked description and rate">
                            <RotateCcw />
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                  {manual.map(m => (
                    <tr key={m.key}>
                      <td className="!py-1.5">
                        <input
                          className="input h-7"
                          value={m.description}
                          placeholder="Description"
                          autoFocus={!m.description}
                          onChange={e => editManual(m.key, { description: e.target.value })}
                        />
                      </td>
                      <td className="!py-1.5">
                        <NumberField
                          className="h-7 text-right"
                          value={m.quantity}
                          onChange={n => editManual(m.key, { quantity: n })}
                        />
                      </td>
                      <td className="!py-1.5">
                        <NumberField
                          className="h-7 text-right"
                          value={m.unit_price}
                          onChange={n => editManual(m.key, { unit_price: n })}
                        />
                      </td>
                      <td className="text-right num font-medium text-fg">{money(m.fixedTotal ?? round2(m.quantity * m.unit_price))}</td>
                      <td>
                        <button onClick={() => setManual(list => list.filter(x => x.key !== m.key))} className="btn-icon-sm hover:text-red" title="Remove line">
                          <Trash2 />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}

        {/* Notes, tax, totals */}
        {clientId > 0 && (
          <div className="grid grid-cols-[minmax(0,1fr)_320px] gap-5 items-start">
            <div className="card p-5 space-y-4">
              <div className="grid grid-cols-2 gap-4">
                {paymentMethods.length > 0 && (
                  <div>
                    <label className="label">Payment method</label>
                    <select
                      className="input"
                      value={paymentMethod}
                      onChange={e => {
                        setPaymentMethod(e.target.value)
                        const m = paymentMethods.find(x => x.name === e.target.value)
                        if (m) setNotes(paymentNote(m))
                      }}
                    >
                      {paymentMethods.map(m => <option key={m.name} value={m.name}>{m.name}{m.email ? ` · ${m.email}` : ''}</option>)}
                    </select>
                  </div>
                )}
                <div>
                  <label className="label">Other tax (%)</label>
                  <input className="input num" type="number" step="0.01" value={taxRate || ''} placeholder="0" onChange={e => setTaxRate(parseFloat(e.target.value) || 0)} />
                </div>
              </div>
              <div className="rounded-lg border border-line p-3">
                <label className="flex items-center gap-2.5 cursor-pointer select-none">
                  <input type="checkbox" checked={gstApplicable} onChange={e => setGstApplicable(e.target.checked)} />
                  <span className="text-sm font-medium text-fg">Charge GST/HST</span>
                  <span className="text-xs text-fg-3">
                    {taxSettings?.gst_hst_registered ? `From your tax settings${taxSettings.province ? ` (${taxSettings.province})` : ''}` : "You're not registered in Tax settings"}
                  </span>
                </label>
                {gstApplicable && (
                  <div className="grid grid-cols-2 gap-3 mt-3">
                    <div>
                      <label className="label">Rate (%)</label>
                      <input className="input num" type="number" step="0.01" value={gstRate || ''} onChange={e => setGstRate(parseFloat(e.target.value) || 0)} />
                    </div>
                    <div>
                      <label className="label">GST/HST number</label>
                      <input className="input" value={gstNumber} onChange={e => setGstNumber(e.target.value)} placeholder="123456789 RT0001" />
                    </div>
                  </div>
                )}
              </div>
              <div>
                <label className="label">Notes and payment instructions</label>
                <textarea className="input" rows={3} value={notes} onChange={e => setNotes(e.target.value)} placeholder="Shown at the bottom of the invoice" />
              </div>
            </div>

            <div className="card p-5">
              <div className="space-y-2 text-sm">
                <div className="flex justify-between"><span className="text-fg-3">Subtotal</span><span className="num text-fg">{money(subtotal)}</span></div>
                {gstApplicable && gstRate > 0 && (
                  <div className="flex justify-between"><span className="text-fg-3">GST/HST ({gstRate}%)</span><span className="num text-fg">{money(gstAmount)}</span></div>
                )}
                {taxRate > 0 && (
                  <div className="flex justify-between"><span className="text-fg-3">Other tax ({taxRate}%)</span><span className="num text-fg">{money(otherTax)}</span></div>
                )}
                <div className="flex justify-between items-baseline pt-3 mt-1 border-t border-line">
                  <span className="font-semibold text-fg">Total</span>
                  <span className="num text-2xl font-semibold text-fg">{money(total)}</span>
                </div>
                {selectedMinutes > 0 && (
                  <p className="text-xs text-fg-3 pt-1">{formatHoursShort(selectedMinutes / 60)} of tracked time will be marked as billed on this invoice.</p>
                )}
              </div>
            </div>
          </div>
        )}
      </div>

      {/* Sticky actions */}
      <div className="fixed bottom-0 left-[220px] right-0 z-30 border-t border-line bg-bg">
        <div className="max-w-[1160px] mx-auto px-7 h-[52px] flex items-center gap-3">
          <div className="text-[13px] text-fg-3">
            {title} {clientId > 0 && <>· <span className="num font-semibold text-fg">{money(total)}</span></>}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <button onClick={() => navigate(isEdit ? `/invoices/${editId}` : -1 as any)} className="btn-secondary">Cancel</button>
            {(!isEdit || existing?.status === 'draft') && (
              <button onClick={() => save(false)} disabled={saving || !clientId} className="btn-secondary">
                {isEdit ? 'Save draft' : 'Save as draft'}
              </button>
            )}
            {(!isEdit || existing?.status === 'draft') ? (
              <button onClick={() => save(true)} disabled={saving || !clientId} className="btn-primary">
                Save and mark as sent
              </button>
            ) : (
              <button onClick={() => save(false)} disabled={saving} className="btn-primary">
                Save changes
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
