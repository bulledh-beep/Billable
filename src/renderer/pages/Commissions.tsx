import { useState, useEffect, useMemo } from 'react'
import { motion } from 'framer-motion'
import {
  Plus, Download, Pencil, Trash2, Search, Sun, Home, ChevronDown,
  AlertTriangle, HandCoins,
} from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import { formatMoney, formatDate, todayISO } from '../utils/format'
import type {
  Commission, CommissionJobType, CommissionStatus, CommissionPaymentStatus,
} from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

// Friendly, consistent labels used everywhere
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
const PAYMENT_STYLE: Record<CommissionPaymentStatus, string> = {
  unpaid: 'bg-status-overdue/10 text-status-overdue',
  pending: 'bg-status-paused/15 text-status-paused',
  paid: 'bg-status-paid/10 text-status-paid',
}

const PENDING_STATUSES: CommissionStatus[] = ['appointment_set', 'appointment_attended', 'needs_review']

const statusLabel = (s: CommissionStatus) => STATUS_OPTIONS.find(o => o.value === s)?.label || s
const paymentLabel = (p: CommissionPaymentStatus) => PAYMENT_OPTIONS.find(o => o.value === p)?.label || p

/** Effective payout: override wins; roofing review gap → null (indeterminate). */
function effectiveCommission(c: Commission): number | null {
  if (c.manual_override != null) return c.manual_override
  if (c.needs_review) return null
  return c.calculated_commission
}
const effectiveOrZero = (c: Commission) => effectiveCommission(c) ?? 0
const isEarned = (c: Commission) => c.status === 'closed_waiting' || c.status === 'paid'
const isReceived = (c: Commission) => c.payment_status === 'paid'
const isPending = (c: Commission) => PENDING_STATUSES.includes(c.status)

type PayoutTone = 'potential' | 'owed' | 'paid' | 'review'
function payoutInfo(c: Commission): { label: string; value: number | null; tone: PayoutTone } {
  if (c.needs_review && c.manual_override == null) return { label: 'Needs Review', value: null, tone: 'review' }
  const v = effectiveOrZero(c)
  if (isReceived(c)) return { label: 'Paid', value: v, tone: 'paid' }
  if (isEarned(c)) return { label: 'Owed', value: v, tone: 'owed' }
  return { label: 'Potential', value: v, tone: 'potential' }
}
const TONE_TEXT: Record<PayoutTone, string> = {
  potential: 'text-text-secondary',
  owed: 'text-accent',
  paid: 'text-status-paid',
  review: 'text-accent',
}

/** Live payout preview while editing (mirrors the server rule). */
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
  client_name: string
  job_type: CommissionJobType
  appointment_date: string
  closer_name: string
  status: CommissionStatus
  payment_status: CommissionPaymentStatus
  system_size_kw: string
  contract_amount: string
  manual_override: string
  notes: string
}
const emptyForm = (): FormState => ({
  client_name: '', job_type: 'solar', appointment_date: todayISO(), closer_name: '',
  status: 'appointment_set', payment_status: 'unpaid',
  system_size_kw: '', contract_amount: '', manual_override: '', notes: '',
})

export default function Commissions() {
  const [commissions, setCommissions] = useState<Commission[]>([])
  const [loading, setLoading] = useState(true)
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<Commission | null>(null)
  const [deleteId, setDeleteId] = useState<number | null>(null)
  const [form, setForm] = useState<FormState>(emptyForm())
  const [showMore, setShowMore] = useState(false)
  const [saving, setSaving] = useState(false)

  // Filters
  const [search, setSearch] = useState('')
  const [jobFilter, setJobFilter] = useState<'all' | CommissionJobType>('all')
  const [statusFilter, setStatusFilter] = useState<'all' | CommissionStatus>('all')
  const [paymentFilter, setPaymentFilter] = useState<'all' | CommissionPaymentStatus>('all')
  const [dateFrom, setDateFrom] = useState('')
  const [dateTo, setDateTo] = useState('')

  useEffect(() => { load() }, [])
  const load = async () => {
    setLoading(true)
    try { setCommissions(await window.api.commissions.list()) }
    catch (err: any) { toast.error(`Failed to load: ${err.message || err}`) }
    finally { setLoading(false) }
  }

  const filtered = useMemo(() => commissions.filter(c => {
    if (jobFilter !== 'all' && c.job_type !== jobFilter) return false
    if (statusFilter !== 'all' && c.status !== statusFilter) return false
    if (paymentFilter !== 'all' && c.payment_status !== paymentFilter) return false
    if (dateFrom && (c.appointment_date || '') < dateFrom) return false
    if (dateTo && (c.appointment_date || '') > dateTo) return false
    if (search) {
      const q = search.toLowerCase()
      if (!c.client_name.toLowerCase().includes(q) && !(c.closer_name || '').toLowerCase().includes(q)) return false
    }
    return true
  }), [commissions, jobFilter, statusFilter, paymentFilter, dateFrom, dateTo, search])

  const stats = useMemo(() => {
    const owed = filtered.filter(c => isEarned(c) && !isReceived(c)).reduce((s, c) => s + effectiveOrZero(c), 0)
    const paid = filtered.filter(isReceived).reduce((s, c) => s + effectiveOrZero(c), 0)
    const expected = filtered.filter(isPending).reduce((s, c) => s + effectiveOrZero(c), 0)
    const pendingCount = filtered.filter(isPending).length
    const closedDeals = filtered.filter(isEarned).length
    const lostDeals = filtered.filter(c => c.status === 'lost').length
    const decided = closedDeals + lostDeals
    const closeRate = decided > 0 ? (closedDeals / decided) * 100 : 0
    const avgPerClosed = closedDeals > 0 ? (owed + paid) / closedDeals : 0
    return { owed, paid, expected, pendingCount, closedDeals, lostDeals, closeRate, avgPerClosed }
  }, [filtered])

  const breakdown = useMemo(() => {
    const group = (type: CommissionJobType) => {
      const rows = filtered.filter(c => c.job_type === type)
      return {
        count: rows.length,
        expected: rows.filter(isPending).reduce((s, c) => s + effectiveOrZero(c), 0),
        owed: rows.filter(c => isEarned(c) && !isReceived(c)).reduce((s, c) => s + effectiveOrZero(c), 0),
        paid: rows.filter(isReceived).reduce((s, c) => s + effectiveOrZero(c), 0),
      }
    }
    return { solar: group('solar'), roofing: group('roofing') }
  }, [filtered])

  const hasActiveFilters = jobFilter !== 'all' || statusFilter !== 'all' || paymentFilter !== 'all' || !!dateFrom || !!dateTo || !!search

  // ---- Form ----
  const openNew = () => { setEditing(null); setForm(emptyForm()); setShowMore(false); setShowForm(true) }
  const openEdit = (c: Commission) => {
    setEditing(c)
    setForm({
      client_name: c.client_name, job_type: c.job_type,
      appointment_date: (c.appointment_date || todayISO()).slice(0, 10),
      closer_name: c.closer_name, status: c.status, payment_status: c.payment_status,
      system_size_kw: c.system_size_kw != null ? String(c.system_size_kw) : '',
      contract_amount: c.contract_amount != null ? String(c.contract_amount) : '',
      manual_override: c.manual_override != null ? String(c.manual_override) : '',
      notes: c.notes,
    })
    setShowMore(c.manual_override != null || !!c.notes)
    setShowForm(true)
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

  // Live preview bucket + explanation in the modal
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
        client_name: form.client_name.trim(), job_type: form.job_type,
        appointment_date: form.appointment_date, closer_name: form.closer_name.trim(),
        status: form.status, payment_status: form.payment_status,
        system_size_kw: form.job_type === 'solar' && form.system_size_kw !== '' ? Number(form.system_size_kw) : null,
        contract_amount: form.job_type === 'roofing' && form.contract_amount !== '' ? Number(form.contract_amount) : null,
        manual_override: form.manual_override !== '' ? Number(form.manual_override) : null,
        notes: form.notes,
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
    if (filtered.length === 0) return toast.error('No commissions to export')
    const rows = filtered.map(c => {
      const p = payoutInfo(c)
      return {
        client: c.client_name, job_type: c.job_type, appointment_date: c.appointment_date || '',
        closer: c.closer_name, status: statusLabel(c.status), payment_status: paymentLabel(c.payment_status),
        system_size_kw: c.system_size_kw ?? '', contract_amount: c.contract_amount ?? '',
        payout_state: p.label, commission: p.value == null ? 'Needs Review' : p.value.toFixed(2), notes: c.notes,
      }
    })
    const result = await window.api.reports.exportCSV(rows, 'commissions.csv')
    if (result) toast.success('Exported commissions.csv')
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-5 sm:p-8 max-w-6xl mx-auto">
      {/* Header */}
      <motion.div variants={item} className="flex flex-wrap items-start justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Commissions</h1>
          <p className="text-sm text-text-secondary mt-1">Track solar and roofing appointment payouts.</p>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={handleExportCSV} className="btn-secondary flex items-center gap-2 text-sm">
            <Download className="w-4 h-4" /> <span className="hidden sm:inline">Export CSV</span><span className="sm:hidden">CSV</span>
          </button>
          <button onClick={openNew} className="btn-primary flex items-center gap-2 text-sm">
            <Plus className="w-4 h-4" /> Add Job
          </button>
        </div>
      </motion.div>

      {/* Hero cards — Owed is dominant */}
      <motion.div variants={item} className="grid grid-cols-2 lg:grid-cols-4 gap-3 mb-3">
        <div className="glass-panel p-4 ring-1 ring-accent/30 bg-accent/[0.05] col-span-2 lg:col-span-1">
          <div className="text-[10px] uppercase tracking-wider text-accent/80 font-semibold">Owed</div>
          <div className="font-mono text-2xl sm:text-3xl font-bold text-accent mt-1">{formatMoney(stats.owed)}</div>
          <div className="text-[11px] text-text-tertiary mt-0.5">Closed jobs not yet paid</div>
        </div>
        <HeroCard label="Paid" value={formatMoney(stats.paid)} sub="Received" tone="paid" />
        <HeroCard label="Expected" value={formatMoney(stats.expected)} sub="Potential from open jobs" />
        <HeroCard label="Pending Jobs" value={String(stats.pendingCount)} sub="Not yet closed" />
      </motion.div>

      {/* Supporting stats strip */}
      <motion.div variants={item} className="glass-panel px-4 py-2.5 mb-6 grid grid-cols-2 sm:grid-cols-4 divide-x divide-rim/[0.05]">
        <MiniStat label="Closed deals" value={String(stats.closedDeals)} />
        <MiniStat label="Lost deals" value={String(stats.lostDeals)} />
        <MiniStat label="Close rate" value={`${stats.closeRate.toFixed(0)}%`} />
        <MiniStat label="Avg / closed" value={formatMoney(stats.avgPerClosed)} />
      </motion.div>

      {/* Job-type breakdown — two compact cards */}
      <motion.div variants={item} className="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-6">
        <BreakdownCard type="solar" data={breakdown.solar} />
        <BreakdownCard type="roofing" data={breakdown.roofing} />
      </motion.div>

      {/* Filters toolbar */}
      <motion.div variants={item} className="flex flex-wrap items-center gap-2 mb-4">
        <div className="relative flex-1 min-w-[200px]">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
          <input className="input-field pl-10" placeholder="Search client or closer…" value={search} onChange={e => setSearch(e.target.value)} />
        </div>
        <select className="input-field w-auto" value={jobFilter} onChange={e => setJobFilter(e.target.value as any)}>
          <option value="all">All types</option><option value="solar">Solar</option><option value="roofing">Roofing</option>
        </select>
        <select className="input-field w-auto" value={statusFilter} onChange={e => setStatusFilter(e.target.value as any)}>
          <option value="all">All statuses</option>
          {STATUS_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
        <select className="input-field w-auto" value={paymentFilter} onChange={e => setPaymentFilter(e.target.value as any)}>
          <option value="all">All payments</option>
          {PAYMENT_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
        {/* Grouped date range control */}
        <div className="flex items-center gap-1.5 bg-surface-200 border border-rim/[0.06] rounded-lg px-2.5 h-[38px]">
          <input type="date" value={dateFrom} onChange={e => setDateFrom(e.target.value)} title="From appointment date"
            className="bg-transparent text-sm text-text-primary outline-none w-[120px] [color-scheme:dark]" />
          <span className="text-text-tertiary text-xs">→</span>
          <input type="date" value={dateTo} onChange={e => setDateTo(e.target.value)} title="To appointment date"
            className="bg-transparent text-sm text-text-primary outline-none w-[120px] [color-scheme:dark]" />
        </div>
      </motion.div>

      {/* Table / cards / empty */}
      {loading ? (
        <div className="text-sm text-text-tertiary text-center py-16">Loading…</div>
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={HandCoins}
          title={commissions.length === 0 ? 'No commission jobs yet' : 'No jobs match these filters'}
          description={commissions.length === 0
            ? 'Add your first solar or roofing appointment to start tracking payouts.'
            : 'Try clearing the search or filters.'}
          action={commissions.length === 0 ? { label: 'Add Commission Job', onClick: openNew } : undefined}
        />
      ) : (
        <>
          {/* Desktop table */}
          <motion.div variants={item} className="glass-panel overflow-hidden hidden md:block">
            <table className="w-full">
              <thead>
                <tr className="border-b border-rim/[0.05] text-[11px] uppercase tracking-wider text-text-tertiary">
                  <th className="text-left px-4 py-3 font-medium">Client</th>
                  <th className="text-left px-3 py-3 font-medium">Type</th>
                  <th className="text-left px-3 py-3 font-medium">Appt</th>
                  <th className="text-left px-3 py-3 font-medium">Status</th>
                  <th className="text-right px-3 py-3 font-medium">Payout</th>
                  <th className="text-left px-3 py-3 font-medium">Payment</th>
                  <th className="w-16"></th>
                </tr>
              </thead>
              <tbody>
                {filtered.map(c => {
                  const p = payoutInfo(c)
                  return (
                    <tr key={c.id} className="border-b border-rim/[0.03] last:border-0 hover:bg-surface-200/40 transition-colors group">
                      <td className="px-4 py-3">
                        <div className="text-sm text-text-primary font-medium">{c.client_name}</div>
                        <div className="text-[11px] text-text-tertiary">{c.closer_name || 'No closer'}</div>
                      </td>
                      <td className="px-3 py-3"><JobTypeTag type={c.job_type} /></td>
                      <td className="px-3 py-3 text-sm text-text-secondary whitespace-nowrap">{c.appointment_date ? formatDate(c.appointment_date) : '—'}</td>
                      <td className="px-3 py-3">
                        <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium ${STATUS_STYLE[c.status]}`}>
                          {statusLabel(c.status)}
                        </span>
                      </td>
                      <td className="px-3 py-3 text-right whitespace-nowrap">
                        {p.value == null
                          ? <span className="inline-flex items-center gap-1 text-accent text-sm font-medium"><AlertTriangle className="w-3.5 h-3.5" />Review</span>
                          : <div><span className={`font-mono text-sm font-semibold ${TONE_TEXT[p.tone]}`}>{formatMoney(p.value)}</span>
                              <div className="text-[10px] uppercase tracking-wide text-text-tertiary">{p.label}</div></div>}
                      </td>
                      <td className="px-3 py-3">
                        <span className={`inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium ${PAYMENT_STYLE[c.payment_status]}`}>{paymentLabel(c.payment_status)}</span>
                      </td>
                      <td className="px-3 py-3">
                        <div className="flex items-center gap-1 justify-end opacity-0 group-hover:opacity-100 transition-opacity">
                          <button onClick={() => openEdit(c)} className="p-1.5 hover:bg-surface-300 rounded transition-colors" title="Edit"><Pencil className="w-3.5 h-3.5 text-text-tertiary" /></button>
                          <button onClick={() => setDeleteId(c.id)} className="p-1.5 hover:bg-red-500/10 rounded transition-colors" title="Delete"><Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" /></button>
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </motion.div>

          {/* Mobile card list */}
          <motion.div variants={item} className="md:hidden space-y-2">
            {filtered.map(c => {
              const p = payoutInfo(c)
              return (
                <div key={c.id} className="glass-panel p-3.5" onClick={() => openEdit(c)}>
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <div className="text-sm font-medium text-text-primary truncate">{c.client_name}</div>
                      <div className="flex items-center gap-2 mt-1"><JobTypeTag type={c.job_type} />
                        <span className="text-[11px] text-text-tertiary">{c.appointment_date ? formatDate(c.appointment_date) : '—'}</span></div>
                    </div>
                    <div className="text-right flex-shrink-0">
                      {p.value == null ? <span className="text-accent text-sm font-medium">Review</span>
                        : <><div className={`font-mono text-sm font-semibold ${TONE_TEXT[p.tone]}`}>{formatMoney(p.value)}</div>
                            <div className="text-[10px] uppercase tracking-wide text-text-tertiary">{p.label}</div></>}
                    </div>
                  </div>
                  <div className="flex items-center justify-between mt-2.5 pt-2.5 border-t border-rim/[0.04]">
                    <div className="flex items-center gap-1.5">
                      <span className={`inline-flex px-2 py-0.5 rounded-full text-[10px] font-medium ${STATUS_STYLE[c.status]}`}>{statusLabel(c.status)}</span>
                      <span className={`inline-flex px-2 py-0.5 rounded-full text-[10px] font-medium ${PAYMENT_STYLE[c.payment_status]}`}>{paymentLabel(c.payment_status)}</span>
                    </div>
                    <button onClick={(e) => { e.stopPropagation(); setDeleteId(c.id) }} className="p-1.5 hover:bg-red-500/10 rounded"><Trash2 className="w-3.5 h-3.5 text-text-tertiary" /></button>
                  </div>
                </div>
              )
            })}
          </motion.div>
        </>
      )}

      {hasActiveFilters && filtered.length > 0 && (
        <div className="text-[11px] text-text-tertiary mt-3 text-center">Showing {filtered.length} of {commissions.length} jobs · totals reflect the current filters</div>
      )}

      {/* ===== Add / Edit modal ===== */}
      <Modal isOpen={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Commission Job' : 'Add Commission Job'} size="lg">
        <div className="space-y-5">
          <p className="text-xs text-text-tertiary -mt-2">Track a solar or roofing appointment payout.</p>

          {/* Step 1: job type cards */}
          <div className="grid grid-cols-2 gap-3">
            {(['solar', 'roofing'] as CommissionJobType[]).map(t => (
              <button key={t} type="button" onClick={() => update('job_type', t)}
                className={`text-left p-3.5 rounded-xl border transition-all ${form.job_type === t ? 'border-accent/50 bg-accent/[0.07] ring-1 ring-accent/20' : 'border-rim/[0.08] hover:border-rim/[0.15] hover:bg-surface-200/50'}`}>
                <div className="flex items-center gap-2">
                  {t === 'solar' ? <Sun className={`w-5 h-5 ${form.job_type === t ? 'text-accent' : 'text-text-tertiary'}`} /> : <Home className={`w-5 h-5 ${form.job_type === t ? 'text-accent' : 'text-text-tertiary'}`} />}
                  <span className={`text-sm font-semibold ${form.job_type === t ? 'text-text-primary' : 'text-text-secondary'}`}>{t === 'solar' ? 'Solar' : 'Roofing'}</span>
                </div>
                <div className="text-[11px] text-text-tertiary mt-1.5">{t === 'solar' ? '$50 per kW' : '$250–$500 per closed job'}</div>
              </button>
            ))}
          </div>

          {/* Step 2: client & appointment */}
          <div className="space-y-3">
            <FieldLabel>Client &amp; appointment</FieldLabel>
            <input className="input-field" value={form.client_name} onChange={e => update('client_name', e.target.value)} placeholder="Client name *" autoFocus />
            <div className="grid grid-cols-2 gap-3">
              <input type="date" className="input-field [color-scheme:dark]" value={form.appointment_date} onChange={e => update('appointment_date', e.target.value)} />
              <input className="input-field" value={form.closer_name} onChange={e => update('closer_name', e.target.value)} placeholder="Closer name" />
            </div>
          </div>

          {/* Step 3: commission details (dynamic) */}
          <div className="space-y-2">
            <FieldLabel>{form.job_type === 'solar' ? 'System size' : 'Contract'}</FieldLabel>
            {form.job_type === 'solar' ? (
              <input type="number" step="0.01" min="0" className="input-field font-mono" value={form.system_size_kw}
                onChange={e => update('system_size_kw', e.target.value)} placeholder="System size in kW (e.g. 6.16)" />
            ) : (
              <div className="relative">
                <span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span>
                <input type="number" step="0.01" min="0" className="input-field pl-7 font-mono" value={form.contract_amount}
                  onChange={e => update('contract_amount', e.target.value)} placeholder="Contract amount (e.g. 25000)" />
              </div>
            )}
            {/* live math line */}
            <div className="text-[11px] text-text-tertiary">
              {form.job_type === 'solar'
                ? (form.system_size_kw ? `${form.system_size_kw} kW × $50 = ${formatMoney((Number(form.system_size_kw) || 0) * 50)}` : 'Commission = kW × $50')
                : '≤ $20k → $250 · ≥ $30k → $500 · in between → Needs Review'}
            </div>
          </div>

          {/* Step 4: status & payment */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <FieldLabel>Deal status</FieldLabel>
              <select className="input-field mt-2" value={form.status} onChange={e => update('status', e.target.value as CommissionStatus)}>
                {STATUS_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </div>
            <div>
              <FieldLabel>Payment</FieldLabel>
              <select className="input-field mt-2" value={form.payment_status} onChange={e => update('payment_status', e.target.value as CommissionPaymentStatus)}>
                {PAYMENT_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </div>
          </div>

          {/* Live payout preview */}
          <div className={`rounded-xl p-3.5 border ${previewBucket.tone === 'review' ? 'border-accent/30 bg-accent/[0.06]' : 'border-rim/[0.06] bg-surface-200/50'}`}>
            <div className="flex items-center justify-between">
              <span className="text-xs text-text-secondary">{previewBucket.label}</span>
              <span className={`font-mono text-lg font-bold ${TONE_TEXT[previewBucket.tone]}`}>
                {previewBucket.value == null ? 'Needs Review' : formatMoney(previewBucket.value)}
              </span>
            </div>
            <div className="text-[11px] text-text-tertiary mt-1">{previewBucket.hint}</div>
          </div>

          {/* Step 5: more details (collapsible) */}
          <div className="border-t border-rim/[0.05] pt-3">
            <button type="button" onClick={() => setShowMore(v => !v)} className="flex items-center gap-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors">
              <ChevronDown className={`w-3.5 h-3.5 transition-transform ${showMore ? 'rotate-180' : ''}`} /> More details
            </button>
            {showMore && (
              <div className="space-y-3 mt-3">
                <div>
                  <FieldLabel>Manual payout override</FieldLabel>
                  <div className="relative mt-2">
                    <span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span>
                    <input type="number" step="0.01" min="0" className="input-field pl-7 font-mono" value={form.manual_override}
                      onChange={e => update('manual_override', e.target.value)} placeholder="Overrides the calculated payout" />
                  </div>
                </div>
                <div>
                  <FieldLabel>Notes</FieldLabel>
                  <textarea className="input-field mt-2" rows={2} value={form.notes} onChange={e => update('notes', e.target.value)} placeholder="Anything worth remembering" />
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer — pinned to the bottom of the scroll area */}
        <div className="sticky bottom-0 -mx-6 -mb-5 mt-5 px-6 py-3.5 bg-surface-100/95 backdrop-blur border-t border-rim/[0.06] flex items-center justify-between gap-3">
          <span className="text-[11px] text-text-tertiary truncate">{validationError || ''}</span>
          <div className="flex gap-2 flex-shrink-0">
            <button onClick={() => setShowForm(false)} className="btn-secondary text-sm">Cancel</button>
            <button onClick={handleSave} disabled={saving || !!validationError}
              className={`btn-primary text-sm ${saving || validationError ? 'opacity-50 cursor-not-allowed' : ''}`}>
              {saving ? 'Saving…' : 'Save Job'}
            </button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog isOpen={deleteId !== null} onClose={() => setDeleteId(null)} onConfirm={handleDelete}
        title="Delete Commission Job" message="Are you sure you want to delete this commission job? This cannot be undone." />
    </motion.div>
  )
}

function HeroCard({ label, value, sub, tone }: { label: string; value: string; sub: string; tone?: 'paid' }) {
  const color = tone === 'paid' ? 'text-status-paid' : 'text-text-primary'
  return (
    <div className="glass-panel p-4">
      <div className="text-[10px] uppercase tracking-wider text-text-tertiary font-semibold">{label}</div>
      <div className={`font-mono text-2xl sm:text-3xl font-bold mt-1 ${color}`}>{value}</div>
      <div className="text-[11px] text-text-tertiary mt-0.5">{sub}</div>
    </div>
  )
}

function MiniStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="px-3 first:pl-0 text-center sm:text-left">
      <div className="text-[10px] uppercase tracking-wider text-text-tertiary">{label}</div>
      <div className="font-mono text-sm font-semibold text-text-primary mt-0.5">{value}</div>
    </div>
  )
}

function JobTypeTag({ type }: { type: CommissionJobType }) {
  return (
    <span className="inline-flex items-center gap-1 text-xs text-text-secondary">
      {type === 'solar' ? <Sun className="w-3.5 h-3.5 text-accent" /> : <Home className="w-3.5 h-3.5 text-status-complete" />}
      {type === 'solar' ? 'Solar' : 'Roofing'}
    </span>
  )
}

function BreakdownCard({ type, data }: { type: CommissionJobType; data: { count: number; expected: number; owed: number; paid: number } }) {
  return (
    <div className="glass-panel p-4">
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          {type === 'solar' ? <Sun className="w-4 h-4 text-accent" /> : <Home className="w-4 h-4 text-status-complete" />}
          <span className="text-sm font-semibold text-text-primary">{type === 'solar' ? 'Solar' : 'Roofing'}</span>
        </div>
        <span className="text-[11px] text-text-tertiary">{data.count} job{data.count === 1 ? '' : 's'}</span>
      </div>
      <div className="grid grid-cols-3 gap-2 text-center">
        <div><div className="text-[10px] uppercase tracking-wider text-text-tertiary">Expected</div><div className="font-mono text-sm font-semibold text-text-primary mt-0.5">{formatMoney(data.expected)}</div></div>
        <div><div className="text-[10px] uppercase tracking-wider text-text-tertiary">Owed</div><div className="font-mono text-sm font-semibold text-accent mt-0.5">{formatMoney(data.owed)}</div></div>
        <div><div className="text-[10px] uppercase tracking-wider text-text-tertiary">Paid</div><div className="font-mono text-sm font-semibold text-status-paid mt-0.5">{formatMoney(data.paid)}</div></div>
      </div>
    </div>
  )
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <div className="text-[10px] font-semibold uppercase tracking-wider text-text-tertiary">{children}</div>
}
