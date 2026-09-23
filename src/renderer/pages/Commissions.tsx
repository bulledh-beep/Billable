import { useState, useEffect, useMemo } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { Download, Pencil, Trash2, Sun, Home, ChevronDown, AlertTriangle, HandCoins, FileText, CheckCircle2, X, Eye } from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import PageHeader from '../components/PageHeader'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import Segmented from '../components/Segmented'
import SearchInput from '../components/SearchInput'
import Menu from '../components/Menu'
import { formatMoney, formatDay, todayISO } from '../utils/format'
import type {
  Commission, CommissionJobType, CommissionStatus, CommissionPaymentStatus,
  CommissionInvoice, CommissionInvoiceStatus,
} from '@shared/types'
import toast from 'react-hot-toast'

const STATUS_OPTIONS: { value: CommissionStatus; label: string }[] = [
  { value: 'appointment_set', label: 'Appointment set' },
  { value: 'appointment_attended', label: 'Attended' },
  { value: 'closed_waiting', label: 'Closed, unpaid' },
  { value: 'paid', label: 'Paid' },
  { value: 'lost', label: 'Lost' },
  { value: 'cancelled', label: 'Cancelled' },
  { value: 'needs_review', label: 'Needs review' },
]
const PAYMENT_OPTIONS: { value: CommissionPaymentStatus; label: string }[] = [
  { value: 'unpaid', label: 'Unpaid' },
  { value: 'pending', label: 'Pending' },
  { value: 'paid', label: 'Paid' },
]
const STATUS_STYLE: Record<CommissionStatus, string> = {
  appointment_set: 'bg-blue/12 text-blue',
  appointment_attended: 'bg-violet/12 text-violet',
  closed_waiting: 'bg-amber/12 text-amber',
  paid: 'bg-green/12 text-green',
  lost: 'bg-red/12 text-red',
  cancelled: 'bg-fg/[0.07] text-fg-3',
  needs_review: 'bg-accent/12 text-accent-text',
}
const INVOICE_STATUS_STYLE: Record<CommissionInvoiceStatus, string> = {
  draft: 'bg-fg/[0.07] text-fg-2',
  sent: 'bg-blue/12 text-blue',
  paid: 'bg-green/12 text-green',
  cancelled: 'bg-red/12 text-red',
}

const statusLabel = (s: CommissionStatus) => STATUS_OPTIONS.find(o => o.value === s)?.label || s
const paymentLabel = (p: CommissionPaymentStatus) => PAYMENT_OPTIONS.find(o => o.value === p)?.label || p

function effectiveCommission(c: Commission): number | null {
  if (c.manual_override != null) return c.manual_override
  if (c.needs_review) return null
  return c.calculated_commission
}
const effectiveOrZero = (c: Commission) => effectiveCommission(c) ?? 0

// ---- Stage predicates (mutually exclusive money buckets, no double counting) ----
const isPaid = (c: Commission) => c.payment_status === 'paid'
const isInvoiced = (c: Commission) => !isPaid(c) && c.invoice_status === 'invoiced'
const isOwed = (c: Commission) => !isPaid(c) && c.status === 'closed_waiting' && c.invoice_status !== 'invoiced'
const isActive = (c: Commission) => !isPaid(c) && ['appointment_set', 'appointment_attended', 'needs_review'].includes(c.status)
const isEarned = (c: Commission) => c.status === 'closed_waiting' || c.status === 'paid' || isPaid(c)

type Stage = 'active' | 'owed' | 'invoiced' | 'paid' | 'all'
const STAGE_TABS: { value: Stage; label: string }[] = [
  { value: 'active', label: 'Active' },
  { value: 'owed', label: 'Owed' },
  { value: 'invoiced', label: 'Invoiced' },
  { value: 'paid', label: 'Paid' },
  { value: 'all', label: 'All' },
]
const inStage = (c: Commission, stage: Stage) =>
  stage === 'all' ? true
    : stage === 'active' ? isActive(c)
      : stage === 'owed' ? isOwed(c)
        : stage === 'invoiced' ? isInvoiced(c)
          : isPaid(c)

type PayoutTone = 'potential' | 'owed' | 'invoiced' | 'paid' | 'review'
function payoutInfo(c: Commission): { label: string; value: number | null; tone: PayoutTone } {
  if (c.needs_review && c.manual_override == null) return { label: 'Needs review', value: null, tone: 'review' }
  const v = effectiveOrZero(c)
  if (isPaid(c)) return { label: 'Paid', value: v, tone: 'paid' }
  if (isInvoiced(c)) return { label: 'Invoiced', value: v, tone: 'invoiced' }
  if (isOwed(c)) return { label: 'Owed', value: v, tone: 'owed' }
  return { label: 'Potential', value: v, tone: 'potential' }
}
const TONE_TEXT: Record<PayoutTone, string> = {
  potential: 'text-fg-2', owed: 'text-amber', invoiced: 'text-blue',
  paid: 'text-green', review: 'text-amber',
}

function previewCommission(jobType: CommissionJobType, kw: string, contract: string, override: string): { value: number | null; review: boolean } {
  if (override !== '' && Number(override) >= 0) return { value: Number(override), review: false }
  if (jobType === 'roofing') {
    if (!contract) return { value: null, review: false }
    const amt = Number(contract) || 0
    if (amt <= 20000) return { value: 250, review: false }
    if (amt >= 30000) return { value: 500, review: false }
    return { value: null, review: true }
  }
  if (!kw) return { value: null, review: false }
  return { value: Math.round((Number(kw) || 0) * 50 * 100) / 100, review: false }
}

interface FormState {
  client_name: string; job_type: CommissionJobType; appointment_date: string; closer_name: string
  status: CommissionStatus; payment_status: CommissionPaymentStatus
  system_size_kw: string; contract_amount: string; manual_override: string; notes: string
}
const emptyForm = (): FormState => ({
  client_name: '', job_type: 'solar', appointment_date: todayISO(), closer_name: '',
  status: 'appointment_set', payment_status: 'unpaid',
  system_size_kw: '', contract_amount: '', manual_override: '', notes: '',
})

export default function Commissions() {
  const [commissions, setCommissions] = useState<Commission[]>([])
  const [invoices, setInvoices] = useState<CommissionInvoice[]>([])
  const [loading, setLoading] = useState(true)
  const [view, setView] = useState<'jobs' | 'invoices'>('jobs')
  const [stage, setStage] = useState<Stage>('owed')

  // Job add/edit
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<Commission | null>(null)
  const [deleteId, setDeleteId] = useState<number | null>(null)
  const [form, setForm] = useState<FormState>(emptyForm())
  const [showMore, setShowMore] = useState(false)
  const [saving, setSaving] = useState(false)

  // Selection / bulk
  const [selected, setSelected] = useState<Set<number>>(new Set())

  // Invoice generation
  const [showGen, setShowGen] = useState(false)
  const [genCategory, setGenCategory] = useState<'both' | CommissionJobType>('both')
  const [genFrom, setGenFrom] = useState('')
  const [genTo, setGenTo] = useState('')
  const [genIncludeInvoiced, setGenIncludeInvoiced] = useState(false)
  const [genExcluded, setGenExcluded] = useState<Set<number>>(new Set())
  const [genRestrict, setGenRestrict] = useState<number[] | null>(null) // when generating from a selection
  const [generating, setGenerating] = useState(false)

  // Invoice list
  const [viewInvoice, setViewInvoice] = useState<CommissionInvoice | null>(null)
  const [deleteInvoiceId, setDeleteInvoiceId] = useState<number | null>(null)

  // Filters
  const [search, setSearch] = useState('')
  const [jobFilter, setJobFilter] = useState<'all' | CommissionJobType>('all')
  const [dateFrom, setDateFrom] = useState('')
  const [dateTo, setDateTo] = useState('')

  useEffect(() => { load() }, [])
  const load = async () => {
    setLoading(true)
    try {
      const [c, inv] = await Promise.all([window.api.commissions.list(), window.api.commissionInvoices.list()])
      setCommissions(c); setInvoices(inv)
    } catch (err: any) { toast.error(`Failed to load: ${err.message || err}`) }
    finally { setLoading(false) }
  }

  // Base filter (search/type/date) — used by stats and the table (stage applied after)
  const baseFiltered = useMemo(() => commissions.filter(c => {
    if (jobFilter !== 'all' && c.job_type !== jobFilter) return false
    if (dateFrom && (c.appointment_date || '') < dateFrom) return false
    if (dateTo && (c.appointment_date || '') > dateTo) return false
    if (search) {
      const q = search.toLowerCase()
      if (!c.client_name.toLowerCase().includes(q) && !(c.closer_name || '').toLowerCase().includes(q)) return false
    }
    return true
  }), [commissions, jobFilter, dateFrom, dateTo, search])

  const visible = useMemo(() => baseFiltered.filter(c => inStage(c, stage)), [baseFiltered, stage])

  const stageCounts = useMemo(() => {
    const counts: Record<Stage, number> = { active: 0, owed: 0, invoiced: 0, paid: 0, all: baseFiltered.length }
    for (const c of baseFiltered) {
      if (isActive(c)) counts.active++
      if (isOwed(c)) counts.owed++
      if (isInvoiced(c)) counts.invoiced++
      if (isPaid(c)) counts.paid++
    }
    return counts
  }, [baseFiltered])

  const stats = useMemo(() => {
    const owed = baseFiltered.filter(isOwed).reduce((s, c) => s + effectiveOrZero(c), 0)
    const invoiced = baseFiltered.filter(isInvoiced).reduce((s, c) => s + effectiveOrZero(c), 0)
    const paid = baseFiltered.filter(isPaid).reduce((s, c) => s + effectiveOrZero(c), 0)
    const expected = baseFiltered.filter(isActive).reduce((s, c) => s + effectiveOrZero(c), 0)
    const closedDeals = baseFiltered.filter(isEarned).length
    const lostDeals = baseFiltered.filter(c => c.status === 'lost').length
    const decided = closedDeals + lostDeals
    const closeRate = decided > 0 ? (closedDeals / decided) * 100 : 0
    const avgPerClosed = closedDeals > 0 ? (owed + invoiced + paid) / closedDeals : 0
    return { owed, invoiced, paid, expected, closedDeals, lostDeals, closeRate, avgPerClosed }
  }, [baseFiltered])

  // ---- Quick & bulk status actions ----
  const today = todayISO()
  const refresh = () => load()
  const quickPatch = async (c: Commission, patch: Partial<Commission>) => {
    try { await window.api.commissions.patch(c.id, patch); refresh() }
    catch (e: any) { toast.error(`Failed: ${e.message || e}`) }
  }
  const markPaid = (c: Commission) => quickPatch(c, {
    payment_status: 'paid', status: 'paid', paid_at: today,
    invoice_status: c.invoice_status === 'invoiced' ? 'paid' : c.invoice_status,
  }).then(() => toast.success('Marked paid'))
  const markClosed = (c: Commission) => quickPatch(c, { status: 'closed_waiting' }).then(() => toast.success('Marked closed'))

  const bulkPatch = async (patch: Partial<Commission>, label: string) => {
    const ids = Array.from(selected)
    if (ids.length === 0) return
    try { await window.api.commissions.bulkPatch(ids, patch); setSelected(new Set()); toast.success(`${ids.length} ${label}`); refresh() }
    catch (e: any) { toast.error(`Failed: ${e.message || e}`) }
  }

  const toggleSelect = (id: number) => setSelected(prev => {
    const next = new Set(prev); next.has(id) ? next.delete(id) : next.add(id); return next
  })
  const allVisibleSelected = visible.length > 0 && visible.every(c => selected.has(c.id))
  const toggleSelectAll = () => setSelected(allVisibleSelected ? new Set() : new Set(visible.map(c => c.id)))

  // ---- Invoice generation ----
  const eligibleForInvoice = (c: Commission) =>
    (c.status === 'closed_waiting' || c.status === 'paid') &&
    !(c.needs_review && c.manual_override == null) &&
    (genIncludeInvoiced || c.invoice_status === 'not_invoiced' || c.invoice_status == null)

  const genEligible = useMemo(() => {
    let pool = commissions.filter(eligibleForInvoice)
    if (genRestrict) pool = pool.filter(c => genRestrict.includes(c.id))
    if (genCategory !== 'both') pool = pool.filter(c => c.job_type === genCategory)
    if (genFrom) pool = pool.filter(c => (c.appointment_date || '') >= genFrom)
    if (genTo) pool = pool.filter(c => (c.appointment_date || '') <= genTo)
    return pool
  }, [commissions, genCategory, genFrom, genTo, genIncludeInvoiced, genRestrict])

  const genSelected = useMemo(() => genEligible.filter(c => !genExcluded.has(c.id)), [genEligible, genExcluded])
  const genTotal = useMemo(() => genSelected.reduce((s, c) => s + effectiveOrZero(c), 0), [genSelected])

  const openGenerate = (restrictIds?: number[]) => {
    setGenRestrict(restrictIds || null)
    setGenExcluded(new Set())
    setGenCategory('both'); setGenFrom(''); setGenTo(''); setGenIncludeInvoiced(false)
    setShowGen(true)
  }
  const handleGenerate = async () => {
    if (genSelected.length === 0) return toast.error('No eligible jobs selected')
    setGenerating(true)
    try {
      await window.api.commissionInvoices.create({
        jobIds: genSelected.map(c => c.id), category: genCategory, date_from: genFrom || null, date_to: genTo || null,
      })
      toast.success('Commission invoice created')
      setShowGen(false); setSelected(new Set()); setView('invoices'); refresh()
    } catch (e: any) { toast.error(`Failed: ${e.message || e}`) }
    finally { setGenerating(false) }
  }

  // ---- Invoice list actions ----
  const markInvoicePaid = async (inv: CommissionInvoice) => {
    if (!confirm(`Mark ${inv.invoice_number} paid? All ${inv.job_count} jobs on it will be marked paid.`)) return
    try { await window.api.commissionInvoices.updateStatus(inv.id, 'paid'); toast.success('Invoice marked paid'); refresh() }
    catch (e: any) { toast.error(`Failed: ${e.message || e}`) }
  }
  const downloadInvoice = async (inv: CommissionInvoice) => {
    try { const p = await window.api.commissionInvoices.exportPDF(inv.id); if (p) toast.success('Invoice PDF saved') }
    catch (e: any) { toast.error(`Export failed: ${e.message || e}`) }
  }
  const handleDeleteInvoice = async () => {
    if (!deleteInvoiceId) return
    try { await window.api.commissionInvoices.delete(deleteInvoiceId); toast.success('Invoice deleted'); setDeleteInvoiceId(null); refresh() }
    catch (e: any) { toast.error(`Failed: ${e.message || e}`) }
  }
  const openInvoiceDetail = async (inv: CommissionInvoice) => {
    const full = await window.api.commissionInvoices.get(inv.id)
    setViewInvoice(full)
  }

  // ---- Add/Edit job form ----
  const openNew = () => { setEditing(null); setForm(emptyForm()); setShowMore(false); setShowForm(true) }
  const openEdit = (c: Commission) => {
    setEditing(c)
    setForm({
      client_name: c.client_name, job_type: c.job_type,
      appointment_date: (c.appointment_date || todayISO()).slice(0, 10),
      closer_name: c.closer_name, status: c.status, payment_status: c.payment_status,
      system_size_kw: c.system_size_kw != null ? String(c.system_size_kw) : '',
      contract_amount: c.contract_amount != null ? String(c.contract_amount) : '',
      manual_override: c.manual_override != null ? String(c.manual_override) : '', notes: c.notes,
    })
    setShowMore(c.manual_override != null || !!c.notes); setShowForm(true)
  }
  const update = <K extends keyof FormState>(key: K, value: FormState[K]) => setForm(f => ({ ...f, [key]: value }))
  const preview = previewCommission(form.job_type, form.system_size_kw, form.contract_amount, form.manual_override)
  const isClosedStatus = form.status === 'closed_waiting' || form.status === 'paid'
  const validationError = useMemo((): string | null => {
    if (!form.client_name.trim()) return 'Add the client name'
    if (!form.appointment_date) return 'Add the appointment date'
    if (form.manual_override !== '' && Number(form.manual_override) < 0) return "The override can't be negative"
    if (form.system_size_kw !== '' && Number(form.system_size_kw) < 0) return "kW can't be negative"
    if (form.contract_amount !== '' && Number(form.contract_amount) < 0) return "The contract amount can't be negative"
    if (isClosedStatus && form.job_type === 'solar' && (!form.system_size_kw || Number(form.system_size_kw) <= 0)) return 'Closed solar jobs need a system size'
    if (isClosedStatus && form.job_type === 'roofing' && (!form.contract_amount || Number(form.contract_amount) <= 0)) return 'Closed roofing jobs need a contract amount'
    return null
  }, [form, isClosedStatus])
  const previewBucket = useMemo(() => {
    if (preview.review) return { label: 'Needs review', value: null as number | null, tone: 'review' as PayoutTone, hint: 'Roofing jobs between $20,001 and $29,999 need a manual payout.' }
    const v = preview.value
    if (form.payment_status === 'paid') return { label: 'Paid payout', value: v, tone: 'paid' as PayoutTone, hint: 'This job has been paid.' }
    if (isClosedStatus) return { label: 'Owed payout', value: v, tone: 'owed' as PayoutTone, hint: 'This job is closed but not paid yet.' }
    return { label: 'Potential payout', value: v, tone: 'potential' as PayoutTone, hint: "Won't count as owed until the deal is marked closed." }
  }, [preview, form.payment_status, isClosedStatus])

  const handleSave = async () => {
    if (validationError) return toast.error(validationError)
    setSaving(true)
    try {
      const payload = {
        client_name: form.client_name.trim(), job_type: form.job_type, appointment_date: form.appointment_date,
        closer_name: form.closer_name.trim(), status: form.status, payment_status: form.payment_status,
        system_size_kw: form.job_type === 'solar' && form.system_size_kw !== '' ? Number(form.system_size_kw) : null,
        contract_amount: form.job_type === 'roofing' && form.contract_amount !== '' ? Number(form.contract_amount) : null,
        manual_override: form.manual_override !== '' ? Number(form.manual_override) : null, notes: form.notes,
      }
      if (editing) { await window.api.commissions.update(editing.id, payload); toast.success('Commission updated') }
      else { await window.api.commissions.create(payload); toast.success('Commission job added') }
      setShowForm(false); load()
    } catch (e: any) { toast.error(`Failed to save: ${e.message || e}`) }
    finally { setSaving(false) }
  }
  const handleDelete = async () => {
    if (!deleteId) return
    try { await window.api.commissions.delete(deleteId); toast.success('Commission deleted'); setDeleteId(null); load() }
    catch (e: any) { toast.error(`Failed: ${e.message || e}`) }
  }
  const handleExportCSV = async () => {
    if (visible.length === 0) return toast.error('No commissions to export')
    const rows = visible.map(c => {
      const p = payoutInfo(c)
      return {
        client: c.client_name, job_type: c.job_type, appointment_date: c.appointment_date || '',
        closer: c.closer_name, status: statusLabel(c.status), payment_status: paymentLabel(c.payment_status),
        invoice_status: c.invoice_status || 'not_invoiced',
        system_size_kw: c.system_size_kw ?? '', contract_amount: c.contract_amount ?? '',
        payout_state: p.label, commission: p.value == null ? 'Needs Review' : p.value.toFixed(2), notes: c.notes,
      }
    })
    const result = await window.api.reports.exportCSV(rows, 'commissions.csv')
    if (result) toast.success('Exported commissions.csv')
  }

  return (
    <div className="page">
      <PageHeader
        title="Commissions"
        actions={
          <>
            {view === 'jobs' && <button onClick={handleExportCSV} className="btn-secondary">CSV</button>}
            <button onClick={() => openGenerate()} className="btn-secondary">New invoice</button>
            <button onClick={openNew} className="btn-secondary">Add job</button>
          </>
        }
      />

      <div className="flex items-center gap-3 mb-5">
        <Segmented
          value={view}
          onChange={setView}
          options={[
            { value: 'jobs', label: 'Jobs', count: commissions.length },
            { value: 'invoices', label: 'Invoices', count: invoices.length },
          ]}
        />
        <span className="text-sm text-fg-3">Solar and roofing appointment payouts</span>
      </div>

      {view === 'jobs' ? (
        <>
          <MetricStrip className="mb-3">
            <Metric label="Owed" value={<Money amount={stats.owed} />} sub="Closed, not invoiced or paid" onClick={() => setStage('owed')} active={stage === 'owed'} />
            <Metric label="Invoiced" value={<Money amount={stats.invoiced} />} sub="On an invoice, unpaid" onClick={() => setStage('invoiced')} active={stage === 'invoiced'} />
            <Metric label="Paid" value={<Money amount={stats.paid} />} sub="Paid out" onClick={() => setStage('paid')} active={stage === 'paid'} />
            <Metric label="Expected" value={<Money amount={stats.expected} />} sub="If active jobs close" onClick={() => setStage('active')} active={stage === 'active'} />
          </MetricStrip>
          <div className="flex items-center gap-5 px-1 mb-5 text-xs text-fg-3">
            <span>Closed deals <b className="num font-semibold text-fg-2">{stats.closedDeals}</b></span>
            <span>Lost <b className="num font-semibold text-fg-2">{stats.lostDeals}</b></span>
            <span>Close rate <b className="num font-semibold text-fg-2">{stats.closeRate.toFixed(0)}%</b></span>
            <span>Average per closed deal <b className="num font-semibold text-fg-2">{formatMoney(stats.avgPerClosed)}</b></span>
          </div>

          <div className="flex flex-wrap items-center gap-2 mb-4">
            <Segmented
              value={stage}
              onChange={v => { setStage(v); setSelected(new Set()) }}
              options={STAGE_TABS.map(t => ({ value: t.value, label: t.label, count: stageCounts[t.value] }))}
            />
            <div className="ml-auto flex items-center gap-2">
              <select className="input w-32" value={jobFilter} onChange={e => setJobFilter(e.target.value as any)} aria-label="Job type">
                <option value="all">All types</option><option value="solar">Solar</option><option value="roofing">Roofing</option>
              </select>
              <input type="date" className="input w-[140px]" value={dateFrom} onChange={e => setDateFrom(e.target.value)} aria-label="From date" />
              <span className="text-fg-4">–</span>
              <input type="date" className="input w-[140px]" value={dateTo} onChange={e => setDateTo(e.target.value)} aria-label="To date" />
              <SearchInput value={search} onChange={setSearch} placeholder="Client or closer" className="w-52" />
            </div>
          </div>

          {loading ? <div className="text-sm text-fg-3 text-center py-16">Loading…</div>
            : visible.length === 0 ? (
              <div className="card">
                <EmptyState icon={HandCoins}
                  title={commissions.length === 0 ? 'Add your first job' : `Nothing in ${STAGE_TABS.find(t => t.value === stage)?.label}`}
                  description={commissions.length === 0 ? 'Log a solar or roofing appointment to start tracking payouts.' : 'Try another tab or clear the filters.'}
                  action={commissions.length === 0 ? { label: 'Add job', onClick: openNew } : undefined} />
              </div>
            ) : (
              <div className="card">
                <table className="tbl">
                  <thead>
                    <tr>
                      <th className="w-8"><input type="checkbox" checked={allVisibleSelected} onChange={toggleSelectAll} aria-label="Select all" /></th>
                      <th>Client</th>
                      <th>Type</th>
                      <th>Appointment</th>
                      <th>Status</th>
                      <th className="text-right">Payout</th>
                      <th className="w-40"></th>
                    </tr>
                  </thead>
                  <tbody>
                    {visible.map(c => {
                      const p = payoutInfo(c)
                      const closedUnpaid = isOwed(c) || isInvoiced(c)
                      return (
                        <tr key={c.id} onDoubleClick={() => openEdit(c)} className={`group ${selected.has(c.id) ? 'bg-accent/[0.05]' : 'hover:bg-fg/[0.02]'}`}>
                          <td><input type="checkbox" checked={selected.has(c.id)} onChange={() => toggleSelect(c.id)} aria-label={`Select ${c.client_name}`} /></td>
                          <td>
                            <div className="text-sm font-medium text-fg">{c.client_name}</div>
                            <div className="text-xs text-fg-3">{c.closer_name || 'No closer'}</div>
                          </td>
                          <td><JobTypeTag type={c.job_type} /></td>
                          <td className="text-fg-2 whitespace-nowrap">{c.appointment_date ? formatDay(c.appointment_date) : '—'}</td>
                          <td><span className={`badge ${STATUS_STYLE[c.status]}`}>{statusLabel(c.status)}</span></td>
                          <td className="text-right whitespace-nowrap">
                            {p.value == null
                              ? <span className="inline-flex items-center gap-1 text-amber text-sm font-medium"><AlertTriangle className="w-3.5 h-3.5" />Review</span>
                              : <div><div className={`num text-sm font-semibold ${TONE_TEXT[p.tone]}`}>{formatMoney(p.value)}</div><div className="text-2xs text-fg-3">{p.label}</div></div>}
                          </td>
                          <td>
                            <div className="flex items-center gap-1 justify-end">
                              {closedUnpaid && <button onClick={() => markPaid(c)} className="btn-secondary btn-sm">Paid</button>}
                              {isActive(c) && c.status !== 'needs_review' && <button onClick={() => markClosed(c)} className="btn-ghost btn-sm opacity-0 group-hover:opacity-100">Mark closed</button>}
                              <Menu items={[
                                { label: 'Edit', icon: Pencil, onClick: () => openEdit(c) },
                                'separator',
                                { label: 'Delete', icon: Trash2, danger: true, onClick: () => setDeleteId(c.id) },
                              ]} />
                            </div>
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            )}
        </>
      ) : (
        loading ? <div className="text-sm text-fg-3 text-center py-16">Loading…</div>
          : invoices.length === 0 ? (
            <div className="card">
              <EmptyState icon={FileText} title="No commission invoices yet"
                description="Bundle closed solar or roofing jobs into one payout invoice."
                action={{ label: 'New invoice', onClick: () => openGenerate() }} />
            </div>
          ) : (
            <div className="card">
              <table className="tbl">
                <thead>
                  <tr>
                    <th>Invoice</th>
                    <th>Jobs</th>
                    <th>Created</th>
                    <th>Status</th>
                    <th className="text-right">Total</th>
                    <th className="w-44"></th>
                  </tr>
                </thead>
                <tbody>
                  {invoices.map(inv => (
                    <tr key={inv.id} onClick={() => openInvoiceDetail(inv)} className="row-hover">
                      <td>
                        <div className="text-sm font-medium text-fg">{inv.invoice_number}</div>
                        <div className="text-xs text-fg-3">{inv.category === 'mixed' ? 'Solar and roofing' : inv.category === 'solar' ? 'Solar' : 'Roofing'}</div>
                      </td>
                      <td className="text-fg-2 num">{inv.job_count}</td>
                      <td className="text-fg-2 whitespace-nowrap">{formatDay(inv.created_at)}</td>
                      <td><span className={`badge ${INVOICE_STATUS_STYLE[inv.status]}`}>{inv.status[0].toUpperCase() + inv.status.slice(1)}</span></td>
                      <td className="text-right num font-semibold text-fg">{formatMoney(inv.total)}</td>
                      <td onClick={e => e.stopPropagation()}>
                        <div className="flex items-center justify-end gap-1">
                          {inv.status !== 'paid' && inv.status !== 'cancelled' && <button onClick={() => markInvoicePaid(inv)} className="btn-secondary btn-sm"><CheckCircle2 /> Mark paid</button>}
                          <Menu items={[
                            { label: 'View jobs', icon: Eye, onClick: () => openInvoiceDetail(inv) },
                            { label: 'Download PDF', icon: Download, onClick: () => downloadInvoice(inv) },
                            'separator',
                            { label: 'Delete', icon: Trash2, danger: true, onClick: () => setDeleteInvoiceId(inv.id) },
                          ]} />
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )
      )}

      {/* Bulk action bar */}
      <AnimatePresence>
        {view === 'jobs' && selected.size > 0 && (
          <motion.div
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 12 }}
            transition={{ duration: 0.15 }}
            className="fixed bottom-5 left-[calc(50%+110px)] -translate-x-1/2 z-40 flex items-center gap-2 pl-4 pr-2 h-12 rounded-[10px] bg-panel shadow-pop"
          >
            <span className="text-sm font-medium text-fg mr-1">{selected.size} selected</span>
            <button onClick={() => bulkPatch({ status: 'closed_waiting' }, 'marked closed')} className="btn-ghost btn-sm">Mark closed</button>
            <button onClick={() => openGenerate(Array.from(selected))} className="btn-secondary btn-sm">Invoice them</button>
            <button onClick={() => bulkPatch({ payment_status: 'paid', status: 'paid', paid_at: today }, 'marked paid')} className="btn-primary btn-sm">Mark paid</button>
            <button onClick={() => setSelected(new Set())} className="btn-icon-sm" aria-label="Clear selection"><X /></button>
          </motion.div>
        )}
      </AnimatePresence>

      {/* ===== Add / Edit job ===== */}
      <Modal
        isOpen={showForm}
        onClose={() => setShowForm(false)}
        title={editing ? 'Edit job' : 'Add job'}
        size="lg"
        footer={
          <>
            <span className="text-xs text-fg-3 truncate mr-auto">{validationError || ''}</span>
            <button onClick={() => setShowForm(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleSave} disabled={saving || !!validationError} className="btn-primary">{saving ? 'Saving…' : 'Save job'}</button>
          </>
        }
      >
        <div className="space-y-5">
          <div className="grid grid-cols-2 gap-3">
            {(['solar', 'roofing'] as CommissionJobType[]).map(t => (
              <button key={t} type="button" onClick={() => update('job_type', t)}
                className={`text-left p-3.5 rounded-lg border transition-colors ${form.job_type === t ? 'border-accent/60 bg-accent/[0.07]' : 'border-line hover:border-line-strong hover:bg-fg/[0.02]'}`}>
                <div className="flex items-center gap-2">
                  {t === 'solar' ? <Sun className={`w-4 h-4 ${form.job_type === t ? 'text-accent-text' : 'text-fg-3'}`} /> : <Home className={`w-4 h-4 ${form.job_type === t ? 'text-accent-text' : 'text-fg-3'}`} />}
                  <span className="text-sm font-semibold text-fg">{t === 'solar' ? 'Solar' : 'Roofing'}</span>
                </div>
                <div className="text-xs text-fg-3 mt-1">{t === 'solar' ? '$50 per kW' : '$250 to $500 per closed job'}</div>
              </button>
            ))}
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="col-span-2">
              <label className="label">Client</label>
              <input className="input" value={form.client_name} onChange={e => update('client_name', e.target.value)} placeholder="Client name" autoFocus />
            </div>
            <div>
              <label className="label">Appointment date</label>
              <input type="date" className="input" value={form.appointment_date} onChange={e => update('appointment_date', e.target.value)} />
            </div>
            <div>
              <label className="label">Closer</label>
              <input className="input" value={form.closer_name} onChange={e => update('closer_name', e.target.value)} placeholder="Optional" />
            </div>
          </div>
          <div>
            <label className="label">{form.job_type === 'solar' ? 'System size (kW)' : 'Contract amount'}</label>
            {form.job_type === 'solar'
              ? <input type="number" step="0.01" min="0" className="input num" value={form.system_size_kw} onChange={e => update('system_size_kw', e.target.value)} placeholder="6.16" />
              : <div className="relative"><span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">$</span><input type="number" step="0.01" min="0" className="input pl-6 num" value={form.contract_amount} onChange={e => update('contract_amount', e.target.value)} placeholder="25000" /></div>}
            <div className="hint">{form.job_type === 'solar' ? (form.system_size_kw ? `${form.system_size_kw} kW × $50 = ${formatMoney((Number(form.system_size_kw) || 0) * 50)}` : 'Payout is kW × $50') : '$20,000 or less pays $250. $30,000 or more pays $500. In between needs a manual payout.'}</div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="label">Deal status</label>
              <select className="input" value={form.status} onChange={e => update('status', e.target.value as CommissionStatus)}>{STATUS_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select>
            </div>
            <div>
              <label className="label">Payment</label>
              <select className="input" value={form.payment_status} onChange={e => update('payment_status', e.target.value as CommissionPaymentStatus)}>{PAYMENT_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select>
            </div>
          </div>
          <div className={`rounded-lg p-3.5 border ${previewBucket.tone === 'review' ? 'border-amber/40 bg-amber/[0.06]' : 'border-line bg-fg/[0.02]'}`}>
            <div className="flex items-center justify-between">
              <span className="text-sm text-fg-2">{previewBucket.label}</span>
              <span className={`num text-lg font-semibold ${TONE_TEXT[previewBucket.tone]}`}>{previewBucket.value == null ? 'Needs review' : formatMoney(previewBucket.value)}</span>
            </div>
            <div className="text-xs text-fg-3 mt-1">{previewBucket.hint}</div>
          </div>
          <div className="border-t border-line pt-3">
            <button type="button" onClick={() => setShowMore(v => !v)} className="btn-ghost btn-sm -ml-2">
              <ChevronDown className={`transition-transform ${showMore ? 'rotate-180' : ''}`} /> Override and notes
            </button>
            {showMore && (
              <div className="space-y-3 mt-3">
                <div>
                  <label className="label">Manual payout</label>
                  <div className="relative"><span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">$</span><input type="number" step="0.01" min="0" className="input pl-6 num" value={form.manual_override} onChange={e => update('manual_override', e.target.value)} placeholder="Replaces the calculated payout" /></div>
                </div>
                <div>
                  <label className="label">Notes</label>
                  <textarea className="input" rows={2} value={form.notes} onChange={e => update('notes', e.target.value)} placeholder="Anything worth remembering" />
                </div>
              </div>
            )}
          </div>
        </div>
      </Modal>

      {/* ===== New commission invoice ===== */}
      <Modal
        isOpen={showGen}
        onClose={() => setShowGen(false)}
        title="New commission invoice"
        description="Bundle closed jobs into one payout invoice."
        size="lg"
        footer={
          <>
            <div className="mr-auto">
              <span className="text-xs text-fg-3">Total </span>
              <span className="num text-base font-semibold text-fg">{formatMoney(genTotal)}</span>
            </div>
            <button onClick={() => setShowGen(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleGenerate} disabled={generating || genSelected.length === 0} className="btn-primary">
              {generating ? 'Creating…' : `Create invoice for ${genSelected.length} job${genSelected.length === 1 ? '' : 's'}`}
            </button>
          </>
        }
      >
        <div className="space-y-4">
          <div className="flex flex-wrap items-center gap-3">
            <Segmented
              value={genCategory}
              onChange={v => setGenCategory(v)}
              options={[
                { value: 'both', label: 'Solar and roofing' },
                { value: 'solar', label: 'Solar' },
                { value: 'roofing', label: 'Roofing' },
              ]}
            />
            <div className="flex items-center gap-2">
              <input type="date" className="input w-[140px]" value={genFrom} onChange={e => setGenFrom(e.target.value)} aria-label="From date" />
              <span className="text-fg-4">–</span>
              <input type="date" className="input w-[140px]" value={genTo} onChange={e => setGenTo(e.target.value)} aria-label="To date" />
            </div>
            <label className="flex items-center gap-2 text-sm text-fg-2 cursor-pointer select-none">
              <input type="checkbox" checked={genIncludeInvoiced} onChange={e => setGenIncludeInvoiced(e.target.checked)} /> Include already invoiced
            </label>
          </div>

          <div className="rounded-lg border border-line overflow-hidden">
            <div className="px-3 h-8 bg-fg/[0.03] border-b border-line text-xs text-fg-3 flex items-center justify-between">
              <span>{genSelected.length} of {genEligible.length} eligible jobs</span><span>Untick to leave one out</span>
            </div>
            <div className="max-h-[300px] overflow-y-auto">
              {genEligible.length === 0 ? <div className="px-3 py-8 text-center text-sm text-fg-3">No closed jobs match these filters.</div>
                : genEligible.map(c => {
                  const excluded = genExcluded.has(c.id)
                  return (
                    <label key={c.id} className={`flex items-center gap-3 px-3 h-12 border-b border-line last:border-b-0 cursor-pointer ${excluded ? 'opacity-45' : ''}`}>
                      <input type="checkbox" checked={!excluded} onChange={() => setGenExcluded(prev => { const n = new Set(prev); n.has(c.id) ? n.delete(c.id) : n.add(c.id); return n })} />
                      <JobTypeTag type={c.job_type} />
                      <div className="flex-1 min-w-0">
                        <div className="text-sm text-fg truncate">{c.client_name}</div>
                        <div className="text-xs text-fg-3">{c.appointment_date ? formatDay(c.appointment_date) : '—'} · {c.job_type === 'solar' ? `${c.system_size_kw ?? 0} kW` : formatMoney(c.contract_amount ?? 0)}{c.invoice_status === 'invoiced' ? ' · already invoiced' : ''}</div>
                      </div>
                      <span className="num text-sm font-semibold text-fg">{formatMoney(effectiveOrZero(c))}</span>
                    </label>
                  )
                })}
            </div>
          </div>
        </div>
      </Modal>

      {/* Commission invoice detail */}
      <Modal
        isOpen={!!viewInvoice}
        onClose={() => setViewInvoice(null)}
        title={viewInvoice?.invoice_number || 'Invoice'}
        description={viewInvoice ? `${viewInvoice.category === 'mixed' ? 'Solar and roofing' : viewInvoice.category === 'solar' ? 'Solar' : 'Roofing'} · ${viewInvoice.job_count} job${viewInvoice.job_count === 1 ? '' : 's'} · ${formatMoney(viewInvoice.total)}` : undefined}
        size="lg"
        footer={viewInvoice && (
          <>
            <button onClick={() => downloadInvoice(viewInvoice)} className="btn-secondary">Download PDF</button>
            {viewInvoice.status !== 'paid' && viewInvoice.status !== 'cancelled' && <button onClick={() => { markInvoicePaid(viewInvoice); setViewInvoice(null) }} className="btn-primary"><CheckCircle2 /> Mark paid</button>}
          </>
        )}
      >
        {viewInvoice && (
          <div className="rounded-lg border border-line max-h-[360px] overflow-y-auto">
            {(viewInvoice.jobs || []).map(j => (
              <div key={j.id} className="flex items-center gap-3 px-3 h-12 border-b border-line last:border-b-0">
                <JobTypeTag type={j.job_type} />
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-fg truncate">{j.client_name}</div>
                  <div className="text-xs text-fg-3">{j.appointment_date ? formatDay(j.appointment_date) : '—'} · {j.job_type === 'solar' ? `${j.system_size_kw ?? 0} kW` : formatMoney(j.contract_amount ?? 0)}</div>
                </div>
                <span className="num text-sm font-semibold text-fg">{formatMoney(effectiveOrZero(j))}</span>
              </div>
            ))}
          </div>
        )}
      </Modal>

      <ConfirmDialog isOpen={deleteId !== null} onClose={() => setDeleteId(null)} onConfirm={handleDelete} title="Delete this job?" message="This removes the commission job for good." />
      <ConfirmDialog isOpen={deleteInvoiceId !== null} onClose={() => setDeleteInvoiceId(null)} onConfirm={handleDeleteInvoice} title="Delete this invoice?" message="Its unpaid jobs go back to Owed so you can invoice them again." />
    </div>
  )
}

function JobTypeTag({ type }: { type: CommissionJobType }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-sm text-fg-2 shrink-0">
      {type === 'solar' ? <Sun className="w-3.5 h-3.5 text-amber" /> : <Home className="w-3.5 h-3.5 text-blue" />}
      {type === 'solar' ? 'Solar' : 'Roofing'}
    </span>
  )
}
