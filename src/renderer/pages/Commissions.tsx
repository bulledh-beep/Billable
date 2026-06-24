import { useState, useEffect, useMemo } from 'react'
import { motion } from 'framer-motion'
import {
  Plus, Download, Pencil, Trash2, Search, Sun, Home, ChevronDown,
  AlertTriangle, HandCoins, FileText, Check, CheckCircle2, X, Eye,
} from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import { formatMoney, formatDate, todayISO } from '../utils/format'
import type {
  Commission, CommissionJobType, CommissionStatus, CommissionPaymentStatus,
  CommissionInvoice, CommissionInvoiceStatus,
} from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

const STATUS_OPTIONS: { value: CommissionStatus; label: string }[] = [
  { value: 'appointment_set', label: 'Appointment Set' },
  { value: 'appointment_attended', label: 'Attended' },
  { value: 'closed_waiting', label: 'Closed, Unpaid' },
  { value: 'paid', label: 'Paid' },
  { value: 'lost', label: 'Lost' },
  { value: 'cancelled', label: 'Cancelled' },
  { value: 'needs_review', label: 'Needs Review' },
]
const PAYMENT_OPTIONS: { value: CommissionPaymentStatus; label: string }[] = [
  { value: 'unpaid', label: 'Unpaid' },
  { value: 'pending', label: 'Pending' },
  { value: 'paid', label: 'Paid' },
]
const STATUS_STYLE: Record<CommissionStatus, string> = {
  appointment_set: 'bg-status-sent/10 text-status-sent',
  appointment_attended: 'bg-status-complete/10 text-status-complete',
  closed_waiting: 'bg-status-paused/15 text-status-paused',
  paid: 'bg-status-paid/10 text-status-paid',
  lost: 'bg-status-overdue/10 text-status-overdue',
  cancelled: 'bg-text-tertiary/10 text-text-tertiary',
  needs_review: 'bg-accent/15 text-accent',
}
const INVOICE_STATUS_STYLE: Record<CommissionInvoiceStatus, string> = {
  draft: 'bg-text-tertiary/10 text-text-secondary',
  sent: 'bg-status-sent/10 text-status-sent',
  paid: 'bg-status-paid/10 text-status-paid',
  cancelled: 'bg-status-overdue/10 text-status-overdue',
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
  if (c.needs_review && c.manual_override == null) return { label: 'Needs Review', value: null, tone: 'review' }
  const v = effectiveOrZero(c)
  if (isPaid(c)) return { label: 'Paid', value: v, tone: 'paid' }
  if (isInvoiced(c)) return { label: 'Invoiced', value: v, tone: 'invoiced' }
  if (isOwed(c)) return { label: 'Owed', value: v, tone: 'owed' }
  return { label: 'Potential', value: v, tone: 'potential' }
}
const TONE_TEXT: Record<PayoutTone, string> = {
  potential: 'text-text-secondary', owed: 'text-accent', invoiced: 'text-status-sent',
  paid: 'text-status-paid', review: 'text-accent',
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
    if (!form.client_name.trim()) return 'Client name is required'
    if (!form.appointment_date) return 'Appointment date is required'
    if (form.manual_override !== '' && Number(form.manual_override) < 0) return 'Override cannot be negative'
    if (form.system_size_kw !== '' && Number(form.system_size_kw) < 0) return 'kW cannot be negative'
    if (form.contract_amount !== '' && Number(form.contract_amount) < 0) return 'Contract amount cannot be negative'
    if (isClosedStatus && form.job_type === 'solar' && (!form.system_size_kw || Number(form.system_size_kw) <= 0)) return 'Closed solar jobs need a system size'
    if (isClosedStatus && form.job_type === 'roofing' && (!form.contract_amount || Number(form.contract_amount) <= 0)) return 'Closed roofing jobs need a contract amount'
    return null
  }, [form, isClosedStatus])
  const previewBucket = useMemo(() => {
    if (preview.review) return { label: 'Needs Review', value: null as number | null, tone: 'review' as PayoutTone, hint: 'Roofing jobs between $20,001 and $29,999 need a manual payout.' }
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
    <motion.div variants={container} initial="hidden" animate="show" className="p-5 sm:p-8 max-w-6xl mx-auto pb-24">
      {/* Header */}
      <motion.div variants={item} className="flex flex-wrap items-start justify-between gap-3 mb-5">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Commissions</h1>
          <p className="text-sm text-text-secondary mt-1">Track solar and roofing appointment payouts.</p>
        </div>
        <div className="flex items-center gap-2">
          {view === 'jobs' && <button onClick={handleExportCSV} className="btn-secondary flex items-center gap-2 text-sm"><Download className="w-4 h-4" /> <span className="hidden sm:inline">CSV</span></button>}
          <button onClick={() => openGenerate()} className="btn-secondary flex items-center gap-2 text-sm"><FileText className="w-4 h-4" /> Generate Invoice</button>
          <button onClick={openNew} className="btn-primary flex items-center gap-2 text-sm"><Plus className="w-4 h-4" /> Add Job</button>
        </div>
      </motion.div>

      {/* View switch */}
      <motion.div variants={item} className="inline-flex gap-0.5 bg-surface-100 rounded-lg p-0.5 border border-rim/[0.05] mb-5">
        {(['jobs', 'invoices'] as const).map(v => (
          <button key={v} onClick={() => setView(v)}
            className={`px-4 py-1.5 rounded-md text-sm font-medium transition-colors capitalize ${view === v ? 'bg-surface-300 text-text-primary' : 'text-text-tertiary hover:text-text-secondary'}`}>
            {v === 'jobs' ? 'Jobs' : `Invoices${invoices.length ? ` (${invoices.length})` : ''}`}
          </button>
        ))}
      </motion.div>

      {view === 'jobs' ? (
        <>
          {/* Dashboard cards */}
          <motion.div variants={item} className="grid grid-cols-2 lg:grid-cols-4 gap-3 mb-3">
            <div className="glass-panel p-4 ring-1 ring-accent/30 bg-accent/[0.05] col-span-2 lg:col-span-1">
              <div className="text-[10px] uppercase tracking-wider text-accent/80 font-semibold">Owed</div>
              <div className="font-mono text-2xl sm:text-3xl font-bold text-accent mt-1">{formatMoney(stats.owed)}</div>
              <div className="text-[11px] text-text-tertiary mt-0.5">Closed, not invoiced or paid</div>
            </div>
            <HeroCard label="Invoiced" value={formatMoney(stats.invoiced)} sub="On an invoice, unpaid" tone="invoiced" />
            <HeroCard label="Paid" value={formatMoney(stats.paid)} sub="Paid out" tone="paid" />
            <HeroCard label="Expected" value={formatMoney(stats.expected)} sub="Potential from active jobs" />
          </motion.div>
          <motion.div variants={item} className="glass-panel px-4 py-2.5 mb-5 grid grid-cols-2 sm:grid-cols-4 divide-x divide-rim/[0.05]">
            <MiniStat label="Closed deals" value={String(stats.closedDeals)} />
            <MiniStat label="Lost deals" value={String(stats.lostDeals)} />
            <MiniStat label="Close rate" value={`${stats.closeRate.toFixed(0)}%`} />
            <MiniStat label="Avg / closed" value={formatMoney(stats.avgPerClosed)} />
          </motion.div>

          {/* Stage tabs */}
          <motion.div variants={item} className="flex flex-wrap items-center gap-1 mb-3">
            {STAGE_TABS.map(t => (
              <button key={t.value} onClick={() => { setStage(t.value); setSelected(new Set()) }}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${stage === t.value ? 'bg-accent/15 text-accent' : 'text-text-tertiary hover:text-text-secondary hover:bg-surface-200'}`}>
                {t.label}<span className="ml-1.5 opacity-60">{stageCounts[t.value]}</span>
              </button>
            ))}
          </motion.div>

          {/* Filters */}
          <motion.div variants={item} className="flex flex-wrap items-center gap-2 mb-4">
            <div className="relative flex-1 min-w-[180px]">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
              <input className="input-field pl-10" placeholder="Search client or closer…" value={search} onChange={e => setSearch(e.target.value)} />
            </div>
            <select className="input-field w-auto" value={jobFilter} onChange={e => setJobFilter(e.target.value as any)}>
              <option value="all">All types</option><option value="solar">Solar</option><option value="roofing">Roofing</option>
            </select>
            <div className="flex items-center gap-1.5 bg-surface-200 border border-rim/[0.06] rounded-lg px-2.5 h-[38px]">
              <input type="date" value={dateFrom} onChange={e => setDateFrom(e.target.value)} className="bg-transparent text-sm text-text-primary outline-none w-[118px] [color-scheme:dark]" />
              <span className="text-text-tertiary text-xs">→</span>
              <input type="date" value={dateTo} onChange={e => setDateTo(e.target.value)} className="bg-transparent text-sm text-text-primary outline-none w-[118px] [color-scheme:dark]" />
            </div>
          </motion.div>

          {/* Jobs table */}
          {loading ? <div className="text-sm text-text-tertiary text-center py-16">Loading…</div>
            : visible.length === 0 ? (
              <EmptyState icon={HandCoins}
                title={commissions.length === 0 ? 'No commission jobs yet' : `Nothing in ${STAGE_TABS.find(t => t.value === stage)?.label}`}
                description={commissions.length === 0 ? 'Add your first solar or roofing appointment to start tracking payouts.' : 'Try another tab or clear the filters.'}
                action={commissions.length === 0 ? { label: 'Add Commission Job', onClick: openNew } : undefined} />
            ) : (
              <>
                <motion.div variants={item} className="glass-panel overflow-hidden hidden md:block">
                  <table className="w-full">
                    <thead>
                      <tr className="border-b border-rim/[0.05] text-[11px] uppercase tracking-wider text-text-tertiary">
                        <th className="w-10 px-3 py-3"><input type="checkbox" checked={allVisibleSelected} onChange={toggleSelectAll} className="rounded border-rim/20 bg-surface-300 text-accent" /></th>
                        <th className="text-left px-2 py-3 font-medium">Client</th>
                        <th className="text-left px-3 py-3 font-medium">Type</th>
                        <th className="text-left px-3 py-3 font-medium">Appt</th>
                        <th className="text-left px-3 py-3 font-medium">Status</th>
                        <th className="text-right px-3 py-3 font-medium">Payout</th>
                        <th className="text-right px-3 py-3 font-medium">Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {visible.map(c => {
                        const p = payoutInfo(c)
                        const closedUnpaid = isOwed(c) || isInvoiced(c)
                        return (
                          <tr key={c.id} className={`border-b border-rim/[0.03] last:border-0 hover:bg-surface-200/40 transition-colors group ${selected.has(c.id) ? 'bg-accent/[0.04]' : ''}`}>
                            <td className="px-3 py-3"><input type="checkbox" checked={selected.has(c.id)} onChange={() => toggleSelect(c.id)} className="rounded border-rim/20 bg-surface-300 text-accent" /></td>
                            <td className="px-2 py-3"><div className="text-sm text-text-primary font-medium">{c.client_name}</div><div className="text-[11px] text-text-tertiary">{c.closer_name || 'No closer'}</div></td>
                            <td className="px-3 py-3"><JobTypeTag type={c.job_type} /></td>
                            <td className="px-3 py-3 text-sm text-text-secondary whitespace-nowrap">{c.appointment_date ? formatDate(c.appointment_date) : '—'}</td>
                            <td className="px-3 py-3"><span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium ${STATUS_STYLE[c.status]}`}>{statusLabel(c.status)}</span></td>
                            <td className="px-3 py-3 text-right whitespace-nowrap">
                              {p.value == null ? <span className="inline-flex items-center gap-1 text-accent text-sm font-medium"><AlertTriangle className="w-3.5 h-3.5" />Review</span>
                                : <div><span className={`font-mono text-sm font-semibold ${TONE_TEXT[p.tone]}`}>{formatMoney(p.value)}</span><div className="text-[10px] uppercase tracking-wide text-text-tertiary">{p.label}</div></div>}
                            </td>
                            <td className="px-3 py-3">
                              <div className="flex items-center gap-1 justify-end">
                                {closedUnpaid && <button onClick={() => markPaid(c)} className="px-2 py-1 rounded-md bg-status-paid/10 text-status-paid text-[11px] font-medium hover:bg-status-paid/20 transition-colors flex items-center gap-1"><Check className="w-3 h-3" />Paid</button>}
                                {isActive(c) && c.status !== 'needs_review' && <button onClick={() => markClosed(c)} className="px-2 py-1 rounded-md bg-status-paused/10 text-status-paused text-[11px] font-medium hover:bg-status-paused/20 transition-colors opacity-0 group-hover:opacity-100">Close</button>}
                                <button onClick={() => openEdit(c)} className="p-1.5 hover:bg-surface-300 rounded transition-colors opacity-0 group-hover:opacity-100" title="Edit"><Pencil className="w-3.5 h-3.5 text-text-tertiary" /></button>
                                <button onClick={() => setDeleteId(c.id)} className="p-1.5 hover:bg-red-500/10 rounded transition-colors opacity-0 group-hover:opacity-100" title="Delete"><Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" /></button>
                              </div>
                            </td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </motion.div>

                {/* Mobile cards */}
                <motion.div variants={item} className="md:hidden space-y-2">
                  {visible.map(c => {
                    const p = payoutInfo(c)
                    const closedUnpaid = isOwed(c) || isInvoiced(c)
                    return (
                      <div key={c.id} className={`glass-panel p-3.5 ${selected.has(c.id) ? 'ring-1 ring-accent/40' : ''}`}>
                        <div className="flex items-start justify-between gap-2">
                          <div className="flex items-start gap-2.5 min-w-0">
                            <input type="checkbox" checked={selected.has(c.id)} onChange={() => toggleSelect(c.id)} className="mt-1 rounded border-rim/20 bg-surface-300 text-accent" />
                            <div className="min-w-0">
                              <div className="text-sm font-medium text-text-primary truncate">{c.client_name}</div>
                              <div className="flex items-center gap-2 mt-1"><JobTypeTag type={c.job_type} /><span className="text-[11px] text-text-tertiary">{c.appointment_date ? formatDate(c.appointment_date) : '—'}</span></div>
                            </div>
                          </div>
                          <div className="text-right flex-shrink-0">
                            {p.value == null ? <span className="text-accent text-sm font-medium">Review</span>
                              : <><div className={`font-mono text-sm font-semibold ${TONE_TEXT[p.tone]}`}>{formatMoney(p.value)}</div><div className="text-[10px] uppercase tracking-wide text-text-tertiary">{p.label}</div></>}
                          </div>
                        </div>
                        <div className="flex items-center justify-between mt-2.5 pt-2.5 border-t border-rim/[0.04]">
                          <span className={`inline-flex px-2 py-0.5 rounded-full text-[10px] font-medium ${STATUS_STYLE[c.status]}`}>{statusLabel(c.status)}</span>
                          <div className="flex items-center gap-1.5">
                            {closedUnpaid && <button onClick={() => markPaid(c)} className="px-2 py-1 rounded-md bg-status-paid/10 text-status-paid text-[11px] font-medium flex items-center gap-1"><Check className="w-3 h-3" />Paid</button>}
                            <button onClick={() => openEdit(c)} className="p-1.5 hover:bg-surface-300 rounded"><Pencil className="w-3.5 h-3.5 text-text-tertiary" /></button>
                            <button onClick={() => setDeleteId(c.id)} className="p-1.5 hover:bg-red-500/10 rounded"><Trash2 className="w-3.5 h-3.5 text-text-tertiary" /></button>
                          </div>
                        </div>
                      </div>
                    )
                  })}
                </motion.div>
              </>
            )}
        </>
      ) : (
        /* ===== Invoices view ===== */
        loading ? <div className="text-sm text-text-tertiary text-center py-16">Loading…</div>
          : invoices.length === 0 ? (
            <EmptyState icon={FileText} title="No commission invoices yet"
              description="Generate an invoice from your closed solar or roofing jobs to bundle payouts together."
              action={{ label: 'Generate Invoice', onClick: () => openGenerate() }} />
          ) : (
            <motion.div variants={item} className="space-y-2">
              {invoices.map(inv => (
                <div key={inv.id} className="glass-panel p-4 flex flex-wrap items-center gap-3">
                  <div className="flex-1 min-w-[160px]">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-semibold text-text-primary">{inv.invoice_number}</span>
                      <span className={`inline-flex px-2 py-0.5 rounded-full text-[10px] font-medium capitalize ${INVOICE_STATUS_STYLE[inv.status]}`}>{inv.status}</span>
                    </div>
                    <div className="text-[11px] text-text-tertiary mt-0.5">
                      {inv.category === 'mixed' ? 'Solar & Roofing' : inv.category[0].toUpperCase() + inv.category.slice(1)} · {inv.job_count} job{inv.job_count === 1 ? '' : 's'} · {formatDate(inv.created_at)}
                    </div>
                  </div>
                  <div className="font-mono text-lg font-bold text-accent">{formatMoney(inv.total)}</div>
                  <div className="flex items-center gap-1">
                    <button onClick={() => openInvoiceDetail(inv)} className="p-2 hover:bg-surface-300 rounded-lg transition-colors" title="View"><Eye className="w-4 h-4 text-text-tertiary" /></button>
                    <button onClick={() => downloadInvoice(inv)} className="p-2 hover:bg-surface-300 rounded-lg transition-colors" title="Download PDF"><Download className="w-4 h-4 text-text-tertiary" /></button>
                    {inv.status !== 'paid' && inv.status !== 'cancelled' && <button onClick={() => markInvoicePaid(inv)} className="px-2.5 py-1.5 rounded-lg bg-status-paid/10 text-status-paid text-xs font-medium hover:bg-status-paid/20 transition-colors flex items-center gap-1"><CheckCircle2 className="w-3.5 h-3.5" />Mark Paid</button>}
                    <button onClick={() => setDeleteInvoiceId(inv.id)} className="p-2 hover:bg-red-500/10 rounded-lg transition-colors" title="Delete"><Trash2 className="w-4 h-4 text-text-tertiary hover:text-red-400" /></button>
                  </div>
                </div>
              ))}
            </motion.div>
          )
      )}

      {/* Bulk action bar */}
      {view === 'jobs' && selected.size > 0 && (
        <motion.div initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }}
          className="fixed bottom-5 left-1/2 -translate-x-1/2 z-40 glass-panel border border-accent/20 shadow-2xl px-4 py-2.5 flex items-center gap-3 rounded-xl">
          <span className="text-sm font-medium text-accent">{selected.size} selected</span>
          <div className="w-px h-5 bg-rim/[0.1]" />
          <button onClick={() => bulkPatch({ payment_status: 'paid', status: 'paid', paid_at: today }, 'marked paid')} className="text-xs font-medium text-status-paid hover:underline">Mark Paid</button>
          <button onClick={() => bulkPatch({ status: 'closed_waiting' }, 'marked closed')} className="text-xs font-medium text-status-paused hover:underline">Mark Closed</button>
          <button onClick={() => openGenerate(Array.from(selected))} className="text-xs font-medium text-text-secondary hover:text-text-primary">Generate Invoice</button>
          <button onClick={() => setSelected(new Set())} className="p-1 hover:bg-surface-300 rounded"><X className="w-4 h-4 text-text-tertiary" /></button>
        </motion.div>
      )}

      {/* ===== Add / Edit job modal ===== */}
      <Modal isOpen={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Commission Job' : 'Add Commission Job'} size="lg">
        <div className="space-y-5">
          <p className="text-xs text-text-tertiary -mt-2">Track a solar or roofing appointment payout.</p>
          <div className="grid grid-cols-2 gap-3">
            {(['solar', 'roofing'] as CommissionJobType[]).map(t => (
              <button key={t} type="button" onClick={() => update('job_type', t)}
                className={`text-left p-3.5 rounded-xl border transition-all ${form.job_type === t ? 'border-accent/50 bg-accent/[0.07] ring-1 ring-accent/20' : 'border-rim/[0.08] hover:border-rim/[0.15] hover:bg-surface-200/50'}`}>
                <div className="flex items-center gap-2">{t === 'solar' ? <Sun className={`w-5 h-5 ${form.job_type === t ? 'text-accent' : 'text-text-tertiary'}`} /> : <Home className={`w-5 h-5 ${form.job_type === t ? 'text-accent' : 'text-text-tertiary'}`} />}<span className={`text-sm font-semibold ${form.job_type === t ? 'text-text-primary' : 'text-text-secondary'}`}>{t === 'solar' ? 'Solar' : 'Roofing'}</span></div>
                <div className="text-[11px] text-text-tertiary mt-1.5">{t === 'solar' ? '$50 per kW' : '$250–$500 per closed job'}</div>
              </button>
            ))}
          </div>
          <div className="space-y-3">
            <FieldLabel>Client &amp; appointment</FieldLabel>
            <input className="input-field" value={form.client_name} onChange={e => update('client_name', e.target.value)} placeholder="Client name *" autoFocus />
            <div className="grid grid-cols-2 gap-3">
              <input type="date" className="input-field [color-scheme:dark]" value={form.appointment_date} onChange={e => update('appointment_date', e.target.value)} />
              <input className="input-field" value={form.closer_name} onChange={e => update('closer_name', e.target.value)} placeholder="Closer name" />
            </div>
          </div>
          <div className="space-y-2">
            <FieldLabel>{form.job_type === 'solar' ? 'System size' : 'Contract'}</FieldLabel>
            {form.job_type === 'solar'
              ? <input type="number" step="0.01" min="0" className="input-field font-mono" value={form.system_size_kw} onChange={e => update('system_size_kw', e.target.value)} placeholder="System size in kW (e.g. 6.16)" />
              : <div className="relative"><span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span><input type="number" step="0.01" min="0" className="input-field pl-7 font-mono" value={form.contract_amount} onChange={e => update('contract_amount', e.target.value)} placeholder="Contract amount (e.g. 25000)" /></div>}
            <div className="text-[11px] text-text-tertiary">{form.job_type === 'solar' ? (form.system_size_kw ? `${form.system_size_kw} kW × $50 = ${formatMoney((Number(form.system_size_kw) || 0) * 50)}` : 'Commission = kW × $50') : '≤ $20k → $250 · ≥ $30k → $500 · in between → Needs Review'}</div>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div><FieldLabel>Deal status</FieldLabel><select className="input-field mt-2" value={form.status} onChange={e => update('status', e.target.value as CommissionStatus)}>{STATUS_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select></div>
            <div><FieldLabel>Payment</FieldLabel><select className="input-field mt-2" value={form.payment_status} onChange={e => update('payment_status', e.target.value as CommissionPaymentStatus)}>{PAYMENT_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}</select></div>
          </div>
          <div className={`rounded-xl p-3.5 border ${previewBucket.tone === 'review' ? 'border-accent/30 bg-accent/[0.06]' : 'border-rim/[0.06] bg-surface-200/50'}`}>
            <div className="flex items-center justify-between"><span className="text-xs text-text-secondary">{previewBucket.label}</span><span className={`font-mono text-lg font-bold ${TONE_TEXT[previewBucket.tone]}`}>{previewBucket.value == null ? 'Needs Review' : formatMoney(previewBucket.value)}</span></div>
            <div className="text-[11px] text-text-tertiary mt-1">{previewBucket.hint}</div>
          </div>
          <div className="border-t border-rim/[0.05] pt-3">
            <button type="button" onClick={() => setShowMore(v => !v)} className="flex items-center gap-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors"><ChevronDown className={`w-3.5 h-3.5 transition-transform ${showMore ? 'rotate-180' : ''}`} /> More details</button>
            {showMore && (
              <div className="space-y-3 mt-3">
                <div><FieldLabel>Manual payout override</FieldLabel><div className="relative mt-2"><span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span><input type="number" step="0.01" min="0" className="input-field pl-7 font-mono" value={form.manual_override} onChange={e => update('manual_override', e.target.value)} placeholder="Overrides the calculated payout" /></div></div>
                <div><FieldLabel>Notes</FieldLabel><textarea className="input-field mt-2" rows={2} value={form.notes} onChange={e => update('notes', e.target.value)} placeholder="Anything worth remembering" /></div>
              </div>
            )}
          </div>
        </div>
        <div className="sticky bottom-0 -mx-6 -mb-5 mt-5 px-6 py-3.5 bg-surface-100/95 backdrop-blur border-t border-rim/[0.06] flex items-center justify-between gap-3">
          <span className="text-[11px] text-text-tertiary truncate">{validationError || ''}</span>
          <div className="flex gap-2 flex-shrink-0">
            <button onClick={() => setShowForm(false)} className="btn-secondary text-sm">Cancel</button>
            <button onClick={handleSave} disabled={saving || !!validationError} className={`btn-primary text-sm ${saving || validationError ? 'opacity-50 cursor-not-allowed' : ''}`}>{saving ? 'Saving…' : 'Save Job'}</button>
          </div>
        </div>
      </Modal>

      {/* ===== Generate Invoice modal ===== */}
      <Modal isOpen={showGen} onClose={() => setShowGen(false)} title="Generate Commission Invoice" size="lg">
        <div className="space-y-4">
          <p className="text-xs text-text-tertiary -mt-2">Bundle eligible closed jobs into one payout invoice.</p>
          <div className="grid grid-cols-3 gap-2">
            {(['solar', 'roofing', 'both'] as const).map(cat => (
              <button key={cat} type="button" onClick={() => setGenCategory(cat)}
                className={`py-2 rounded-lg text-sm font-medium border transition-colors capitalize ${genCategory === cat ? 'border-accent/50 bg-accent/[0.07] text-accent' : 'border-rim/[0.08] text-text-secondary hover:bg-surface-200'}`}>
                {cat}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-2">
            <div className="flex items-center gap-1.5 bg-surface-200 border border-rim/[0.06] rounded-lg px-2.5 h-[38px] flex-1">
              <input type="date" value={genFrom} onChange={e => setGenFrom(e.target.value)} className="bg-transparent text-sm text-text-primary outline-none flex-1 [color-scheme:dark]" />
              <span className="text-text-tertiary text-xs">→</span>
              <input type="date" value={genTo} onChange={e => setGenTo(e.target.value)} className="bg-transparent text-sm text-text-primary outline-none flex-1 [color-scheme:dark]" />
            </div>
            <label className="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer whitespace-nowrap">
              <input type="checkbox" checked={genIncludeInvoiced} onChange={e => setGenIncludeInvoiced(e.target.checked)} className="rounded border-rim/20 bg-surface-300 text-accent" /> Include already-invoiced
            </label>
          </div>

          <div className="border border-rim/[0.06] rounded-xl overflow-hidden">
            <div className="px-3 py-2 bg-surface-200/50 text-[11px] uppercase tracking-wider text-text-tertiary flex items-center justify-between">
              <span>{genSelected.length} of {genEligible.length} eligible jobs</span><span>Uncheck to exclude</span>
            </div>
            <div className="max-h-[280px] overflow-y-auto divide-y divide-rim/[0.04]">
              {genEligible.length === 0 ? <div className="px-3 py-8 text-center text-sm text-text-tertiary">No eligible jobs for this selection.</div>
                : genEligible.map(c => {
                  const excluded = genExcluded.has(c.id)
                  return (
                    <div key={c.id} className={`flex items-center gap-3 px-3 py-2.5 ${excluded ? 'opacity-40' : ''}`}>
                      <input type="checkbox" checked={!excluded} onChange={() => setGenExcluded(prev => { const n = new Set(prev); n.has(c.id) ? n.delete(c.id) : n.add(c.id); return n })} className="rounded border-rim/20 bg-surface-300 text-accent" />
                      <JobTypeTag type={c.job_type} />
                      <div className="flex-1 min-w-0"><div className="text-sm text-text-primary truncate">{c.client_name}</div><div className="text-[11px] text-text-tertiary">{c.appointment_date ? formatDate(c.appointment_date) : '—'} · {c.job_type === 'solar' ? `${c.system_size_kw ?? 0} kW` : formatMoney(c.contract_amount ?? 0)}{c.invoice_status === 'invoiced' ? ' · already invoiced' : ''}</div></div>
                      <span className="font-mono text-sm font-semibold text-text-primary">{formatMoney(effectiveOrZero(c))}</span>
                    </div>
                  )
                })}
            </div>
          </div>
        </div>
        <div className="sticky bottom-0 -mx-6 -mb-5 mt-5 px-6 py-3.5 bg-surface-100/95 backdrop-blur border-t border-rim/[0.06] flex items-center justify-between gap-3">
          <div><span className="text-[11px] text-text-tertiary uppercase tracking-wider">Total</span><div className="font-mono text-lg font-bold text-accent">{formatMoney(genTotal)}</div></div>
          <div className="flex gap-2">
            <button onClick={() => setShowGen(false)} className="btn-secondary text-sm">Cancel</button>
            <button onClick={handleGenerate} disabled={generating || genSelected.length === 0} className={`btn-primary text-sm ${generating || genSelected.length === 0 ? 'opacity-50 cursor-not-allowed' : ''}`}>{generating ? 'Creating…' : `Create Invoice (${genSelected.length})`}</button>
          </div>
        </div>
      </Modal>

      {/* Invoice detail modal */}
      <Modal isOpen={!!viewInvoice} onClose={() => setViewInvoice(null)} title={viewInvoice?.invoice_number || 'Invoice'} size="lg">
        {viewInvoice && (
          <div className="space-y-3">
            <div className="flex items-center justify-between text-sm">
              <span className="text-text-secondary">{viewInvoice.category === 'mixed' ? 'Solar & Roofing' : viewInvoice.category} · {viewInvoice.job_count} jobs</span>
              <span className="font-mono text-lg font-bold text-accent">{formatMoney(viewInvoice.total)}</span>
            </div>
            <div className="border border-rim/[0.06] rounded-xl divide-y divide-rim/[0.04] max-h-[360px] overflow-y-auto">
              {(viewInvoice.jobs || []).map(j => (
                <div key={j.id} className="flex items-center gap-3 px-3 py-2.5">
                  <JobTypeTag type={j.job_type} />
                  <div className="flex-1 min-w-0"><div className="text-sm text-text-primary truncate">{j.client_name}</div><div className="text-[11px] text-text-tertiary">{j.appointment_date ? formatDate(j.appointment_date) : '—'} · {j.job_type === 'solar' ? `${j.system_size_kw ?? 0} kW` : formatMoney(j.contract_amount ?? 0)}</div></div>
                  <span className="font-mono text-sm font-semibold text-text-primary">{formatMoney(effectiveOrZero(j))}</span>
                </div>
              ))}
            </div>
            <div className="flex justify-end gap-2 pt-1">
              <button onClick={() => downloadInvoice(viewInvoice)} className="btn-secondary text-sm flex items-center gap-2"><Download className="w-4 h-4" />Download PDF</button>
              {viewInvoice.status !== 'paid' && viewInvoice.status !== 'cancelled' && <button onClick={() => { markInvoicePaid(viewInvoice); setViewInvoice(null) }} className="btn-primary text-sm">Mark Paid</button>}
            </div>
          </div>
        )}
      </Modal>

      <ConfirmDialog isOpen={deleteId !== null} onClose={() => setDeleteId(null)} onConfirm={handleDelete} title="Delete Commission Job" message="Are you sure you want to delete this commission job? This cannot be undone." />
      <ConfirmDialog isOpen={deleteInvoiceId !== null} onClose={() => setDeleteInvoiceId(null)} onConfirm={handleDeleteInvoice} title="Delete Commission Invoice" message="This removes the invoice and releases its unpaid jobs back to Owed. Continue?" />
    </motion.div>
  )
}

function HeroCard({ label, value, sub, tone }: { label: string; value: string; sub: string; tone?: 'paid' | 'invoiced' }) {
  const color = tone === 'paid' ? 'text-status-paid' : tone === 'invoiced' ? 'text-status-sent' : 'text-text-primary'
  return (
    <div className="glass-panel p-4">
      <div className="text-[10px] uppercase tracking-wider text-text-tertiary font-semibold">{label}</div>
      <div className={`font-mono text-2xl sm:text-3xl font-bold mt-1 ${color}`}>{value}</div>
      <div className="text-[11px] text-text-tertiary mt-0.5">{sub}</div>
    </div>
  )
}
function MiniStat({ label, value }: { label: string; value: string }) {
  return <div className="px-3 first:pl-0 text-center sm:text-left"><div className="text-[10px] uppercase tracking-wider text-text-tertiary">{label}</div><div className="font-mono text-sm font-semibold text-text-primary mt-0.5">{value}</div></div>
}
function JobTypeTag({ type }: { type: CommissionJobType }) {
  return <span className="inline-flex items-center gap-1 text-xs text-text-secondary flex-shrink-0">{type === 'solar' ? <Sun className="w-3.5 h-3.5 text-accent" /> : <Home className="w-3.5 h-3.5 text-status-complete" />}{type === 'solar' ? 'Solar' : 'Roofing'}</span>
}
function FieldLabel({ children }: { children: React.ReactNode }) {
  return <div className="text-[10px] font-semibold uppercase tracking-wider text-text-tertiary">{children}</div>
}
