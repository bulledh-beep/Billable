import { useState, useEffect, useMemo } from 'react'
import { motion } from 'framer-motion'
import {
  Plus, Download, Pencil, Trash2, Search, Sun, Home, DollarSign,
  TrendingUp, Clock, CheckCircle2, XCircle, AlertTriangle,
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

const STATUS_OPTIONS: { value: CommissionStatus; label: string }[] = [
  { value: 'appointment_set', label: 'Appointment Set' },
  { value: 'appointment_attended', label: 'Appointment Attended' },
  { value: 'closed_waiting', label: 'Closed - Waiting for Payment' },
  { value: 'paid', label: 'Paid' },
  { value: 'lost', label: 'Lost / Not Closed' },
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

const CLOSED_STATUSES: CommissionStatus[] = ['closed_waiting', 'paid']

const statusLabel = (s: CommissionStatus) => STATUS_OPTIONS.find(o => o.value === s)?.label || s
const paymentLabel = (p: CommissionPaymentStatus) => PAYMENT_OPTIONS.find(o => o.value === p)?.label || p

/** Effective commission: override wins; review gap → null (indeterminate). */
function effectiveCommission(c: Commission): number | null {
  if (c.manual_override != null) return c.manual_override
  if (c.needs_review) return null
  return c.calculated_commission
}
const effectiveOrZero = (c: Commission) => effectiveCommission(c) ?? 0
const isEarned = (c: Commission) => CLOSED_STATUSES.includes(c.status)
const isReceived = (c: Commission) => c.payment_status === 'paid'

/** Live preview of commission while editing the form (mirrors the server rule). */
function previewCommission(jobType: CommissionJobType, kw: string, contract: string, override: string): { value: number | null; review: boolean } {
  if (override !== '' && Number(override) >= 0) return { value: Number(override), review: false }
  if (jobType === 'roofing') {
    const amt = Number(contract) || 0
    if (!contract) return { value: null, review: false }
    if (amt <= 20000) return { value: 250, review: false }
    if (amt >= 30000) return { value: 500, review: false }
    return { value: null, review: true }
  }
  const k = Number(kw) || 0
  return { value: Math.round(k * 50 * 100) / 100, review: false }
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
  client_name: '',
  job_type: 'solar',
  appointment_date: todayISO(),
  closer_name: '',
  status: 'appointment_set',
  payment_status: 'unpaid',
  system_size_kw: '',
  contract_amount: '',
  manual_override: '',
  notes: '',
})

export default function Commissions() {
  const [commissions, setCommissions] = useState<Commission[]>([])
  const [loading, setLoading] = useState(true)
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<Commission | null>(null)
  const [deleteId, setDeleteId] = useState<number | null>(null)
  const [form, setForm] = useState<FormState>(emptyForm())
  const [saving, setSaving] = useState(false)

  // Filters
  const [search, setSearch] = useState('')
  const [jobFilter, setJobFilter] = useState<'all' | CommissionJobType>('all')
  const [statusFilter, setStatusFilter] = useState<'all' | CommissionStatus>('all')
  const [paymentFilter, setPaymentFilter] = useState<'all' | CommissionPaymentStatus>('all')
  const [closerFilter, setCloserFilter] = useState<string>('all')
  const [dateFrom, setDateFrom] = useState('')
  const [dateTo, setDateTo] = useState('')

  useEffect(() => { load() }, [])

  const load = async () => {
    setLoading(true)
    try {
      setCommissions(await window.api.commissions.list())
    } catch (err: any) {
      toast.error(`Failed to load commissions: ${err.message || err}`)
    } finally {
      setLoading(false)
    }
  }

  const closers = useMemo(() => {
    const set = new Set(commissions.map(c => c.closer_name).filter(Boolean))
    return Array.from(set).sort()
  }, [commissions])

  const filtered = useMemo(() => {
    return commissions.filter(c => {
      if (jobFilter !== 'all' && c.job_type !== jobFilter) return false
      if (statusFilter !== 'all' && c.status !== statusFilter) return false
      if (paymentFilter !== 'all' && c.payment_status !== paymentFilter) return false
      if (closerFilter !== 'all' && c.closer_name !== closerFilter) return false
      if (dateFrom && (c.appointment_date || '') < dateFrom) return false
      if (dateTo && (c.appointment_date || '') > dateTo) return false
      if (search) {
        const q = search.toLowerCase()
        if (!c.client_name.toLowerCase().includes(q) && !(c.closer_name || '').toLowerCase().includes(q)) return false
      }
      return true
    })
  }, [commissions, jobFilter, statusFilter, paymentFilter, closerFilter, dateFrom, dateTo, search])

  // Dashboard metrics reflect the current filtered view
  const stats = useMemo(() => {
    const earned = filtered.filter(isEarned)
    const expected = earned.reduce((s, c) => s + effectiveOrZero(c), 0)
    const paid = filtered.filter(isReceived).reduce((s, c) => s + effectiveOrZero(c), 0)
    const unpaid = earned.filter(c => !isReceived(c)).reduce((s, c) => s + effectiveOrZero(c), 0)
    const solar = earned.filter(c => c.job_type === 'solar').reduce((s, c) => s + effectiveOrZero(c), 0)
    const roofing = earned.filter(c => c.job_type === 'roofing').reduce((s, c) => s + effectiveOrZero(c), 0)
    const pendingAppointments = filtered.filter(c => c.status === 'appointment_set' || c.status === 'appointment_attended').length
    const closedDeals = earned.length
    const lostDeals = filtered.filter(c => c.status === 'lost').length
    const decided = closedDeals + lostDeals
    const closeRate = decided > 0 ? (closedDeals / decided) * 100 : 0
    const avgCommission = closedDeals > 0 ? expected / closedDeals : 0
    return { expected, paid, unpaid, solar, roofing, pendingAppointments, closedDeals, lostDeals, closeRate, avgCommission }
  }, [filtered])

  // Report grouped by job type (earned-based)
  const report = useMemo(() => {
    const group = (type: CommissionJobType) => {
      const rows = filtered.filter(c => c.job_type === type)
      const earned = rows.filter(isEarned)
      const expected = earned.reduce((s, c) => s + effectiveOrZero(c), 0)
      const paid = rows.filter(isReceived).reduce((s, c) => s + effectiveOrZero(c), 0)
      const unpaid = earned.filter(c => !isReceived(c)).reduce((s, c) => s + effectiveOrZero(c), 0)
      return { count: rows.length, expected, paid, unpaid }
    }
    return { solar: group('solar'), roofing: group('roofing') }
  }, [filtered])

  // ---- Form handling ----
  const openNew = () => { setEditing(null); setForm(emptyForm()); setShowForm(true) }
  const openEdit = (c: Commission) => {
    setEditing(c)
    setForm({
      client_name: c.client_name,
      job_type: c.job_type,
      appointment_date: (c.appointment_date || todayISO()).slice(0, 10),
      closer_name: c.closer_name,
      status: c.status,
      payment_status: c.payment_status,
      system_size_kw: c.system_size_kw != null ? String(c.system_size_kw) : '',
      contract_amount: c.contract_amount != null ? String(c.contract_amount) : '',
      manual_override: c.manual_override != null ? String(c.manual_override) : '',
      notes: c.notes,
    })
    setShowForm(true)
  }
  const update = <K extends keyof FormState>(key: K, value: FormState[K]) => setForm(f => ({ ...f, [key]: value }))

  const formPreview = previewCommission(form.job_type, form.system_size_kw, form.contract_amount, form.manual_override)

  const validate = (): string | null => {
    if (!form.client_name.trim()) return 'Client name is required'
    if (!form.appointment_date) return 'Appointment date is required'
    if (form.manual_override !== '' && Number(form.manual_override) < 0) return 'Override cannot be negative'
    const closed = CLOSED_STATUSES.includes(form.status)
    if (closed && form.job_type === 'solar' && (!form.system_size_kw || Number(form.system_size_kw) <= 0)) {
      return 'Closed solar jobs need a system size (kW)'
    }
    if (closed && form.job_type === 'roofing' && (!form.contract_amount || Number(form.contract_amount) <= 0)) {
      return 'Closed roofing jobs need a contract amount'
    }
    if (form.system_size_kw !== '' && Number(form.system_size_kw) < 0) return 'kW cannot be negative'
    if (form.contract_amount !== '' && Number(form.contract_amount) < 0) return 'Contract amount cannot be negative'
    return null
  }

  const handleSave = async () => {
    const err = validate()
    if (err) return toast.error(err)
    setSaving(true)
    try {
      const payload = {
        client_name: form.client_name.trim(),
        job_type: form.job_type,
        appointment_date: form.appointment_date,
        closer_name: form.closer_name.trim(),
        status: form.status,
        payment_status: form.payment_status,
        system_size_kw: form.job_type === 'solar' && form.system_size_kw !== '' ? Number(form.system_size_kw) : null,
        contract_amount: form.job_type === 'roofing' && form.contract_amount !== '' ? Number(form.contract_amount) : null,
        manual_override: form.manual_override !== '' ? Number(form.manual_override) : null,
        notes: form.notes,
      }
      if (editing) {
        await window.api.commissions.update(editing.id, payload)
        toast.success('Commission updated')
      } else {
        await window.api.commissions.create(payload)
        toast.success('Commission job added')
      }
      setShowForm(false)
      load()
    } catch (e: any) {
      toast.error(`Failed to save: ${e.message || e}`)
    } finally {
      setSaving(false)
    }
  }

  const handleDelete = async () => {
    if (!deleteId) return
    try {
      await window.api.commissions.delete(deleteId)
      toast.success('Commission deleted')
      setDeleteId(null)
      load()
    } catch (e: any) {
      toast.error(`Failed: ${e.message || e}`)
    }
  }

  const handleExportCSV = async () => {
    if (filtered.length === 0) return toast.error('No commissions to export')
    const rows = filtered.map(c => {
      const eff = effectiveCommission(c)
      return {
        client: c.client_name,
        job_type: c.job_type,
        appointment_date: c.appointment_date || '',
        closer: c.closer_name,
        status: statusLabel(c.status),
        payment_status: paymentLabel(c.payment_status),
        system_size_kw: c.system_size_kw ?? '',
        contract_amount: c.contract_amount ?? '',
        commission: c.needs_review && c.manual_override == null ? 'Needs Review' : (eff ?? 0).toFixed(2),
        earned: isEarned(c) ? 'yes' : 'no',
        notes: c.notes,
      }
    })
    const result = await window.api.reports.exportCSV(rows, 'commissions.csv')
    if (result) toast.success('Exported commissions.csv')
  }

  const commissionCell = (c: Commission) => {
    if (c.needs_review && c.manual_override == null) {
      return <span className="text-accent font-medium">Needs Review</span>
    }
    const eff = effectiveOrZero(c)
    return (
      <span className="font-mono">
        {formatMoney(eff)}
        {c.manual_override != null && <span className="ml-1 text-[10px] text-text-tertiary">(override)</span>}
      </span>
    )
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-6 sm:p-8">
      {/* Header */}
      <motion.div variants={item} className="flex flex-wrap items-center justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Commissions</h1>
          <p className="text-sm text-text-secondary mt-1">Appointment-setting commissions for solar &amp; roofing.</p>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={handleExportCSV} className="btn-secondary flex items-center gap-2">
            <Download className="w-4 h-4" /> Export CSV
          </button>
          <button onClick={openNew} className="btn-primary flex items-center gap-2">
            <Plus className="w-4 h-4" /> Add Commission Job
          </button>
        </div>
      </motion.div>

      {/* Dashboard cards */}
      <motion.div variants={item} className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-3 mb-4">
        <StatCard label="Expected" value={formatMoney(stats.expected)} icon={DollarSign} accent />
        <StatCard label="Unpaid (owed)" value={formatMoney(stats.unpaid)} icon={Clock} tone="overdue" />
        <StatCard label="Paid" value={formatMoney(stats.paid)} icon={CheckCircle2} tone="paid" />
        <StatCard label="Solar" value={formatMoney(stats.solar)} icon={Sun} />
        <StatCard label="Roofing" value={formatMoney(stats.roofing)} icon={Home} />
      </motion.div>
      <motion.div variants={item} className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-3 mb-6">
        <StatCard label="Pending appts" value={String(stats.pendingAppointments)} icon={Clock} />
        <StatCard label="Closed deals" value={String(stats.closedDeals)} icon={CheckCircle2} tone="paid" />
        <StatCard label="Lost deals" value={String(stats.lostDeals)} icon={XCircle} tone="overdue" />
        <StatCard label="Close rate" value={`${stats.closeRate.toFixed(0)}%`} icon={TrendingUp} />
        <StatCard label="Avg / closed" value={formatMoney(stats.avgCommission)} icon={DollarSign} />
      </motion.div>

      {/* Grouped report */}
      <motion.div variants={item} className="glass-panel p-4 mb-6">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-text-tertiary mb-3">Report by job type</h2>
        <div className="overflow-x-auto">
          <table className="w-full text-sm min-w-[480px]">
            <thead>
              <tr className="text-[11px] uppercase tracking-wider text-text-tertiary">
                <th className="text-left font-medium py-1.5">Type</th>
                <th className="text-right font-medium py-1.5">Jobs</th>
                <th className="text-right font-medium py-1.5">Expected</th>
                <th className="text-right font-medium py-1.5">Unpaid</th>
                <th className="text-right font-medium py-1.5">Paid</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {([['Solar', report.solar], ['Roofing', report.roofing]] as const).map(([name, g]) => (
                <tr key={name} className="border-t border-rim/[0.04]">
                  <td className="py-2 font-sans text-text-primary">{name}</td>
                  <td className="py-2 text-right text-text-secondary">{g.count}</td>
                  <td className="py-2 text-right text-text-primary">{formatMoney(g.expected)}</td>
                  <td className="py-2 text-right text-status-overdue">{formatMoney(g.unpaid)}</td>
                  <td className="py-2 text-right text-status-paid">{formatMoney(g.paid)}</td>
                </tr>
              ))}
              <tr className="border-t border-rim/[0.08] font-semibold">
                <td className="py-2 font-sans text-text-primary">Total</td>
                <td className="py-2 text-right text-text-secondary">{report.solar.count + report.roofing.count}</td>
                <td className="py-2 text-right text-accent">{formatMoney(report.solar.expected + report.roofing.expected)}</td>
                <td className="py-2 text-right text-status-overdue">{formatMoney(report.solar.unpaid + report.roofing.unpaid)}</td>
                <td className="py-2 text-right text-status-paid">{formatMoney(report.solar.paid + report.roofing.paid)}</td>
              </tr>
            </tbody>
          </table>
        </div>
      </motion.div>

      {/* Filters */}
      <motion.div variants={item} className="flex flex-wrap gap-2 mb-4">
        <div className="relative flex-1 min-w-[180px]">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
          <input className="input-field pl-10" placeholder="Search client or closer…" value={search} onChange={e => setSearch(e.target.value)} />
        </div>
        <select className="input-field w-auto" value={jobFilter} onChange={e => setJobFilter(e.target.value as any)}>
          <option value="all">All types</option>
          <option value="solar">Solar</option>
          <option value="roofing">Roofing</option>
        </select>
        <select className="input-field w-auto" value={statusFilter} onChange={e => setStatusFilter(e.target.value as any)}>
          <option value="all">All statuses</option>
          {STATUS_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
        <select className="input-field w-auto" value={paymentFilter} onChange={e => setPaymentFilter(e.target.value as any)}>
          <option value="all">All payments</option>
          {PAYMENT_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
        </select>
        <select className="input-field w-auto" value={closerFilter} onChange={e => setCloserFilter(e.target.value)}>
          <option value="all">All closers</option>
          {closers.map(c => <option key={c} value={c}>{c}</option>)}
        </select>
        <input type="date" className="input-field w-auto" value={dateFrom} onChange={e => setDateFrom(e.target.value)} title="From appointment date" />
        <input type="date" className="input-field w-auto" value={dateTo} onChange={e => setDateTo(e.target.value)} title="To appointment date" />
      </motion.div>

      {/* Table */}
      {loading ? (
        <div className="text-sm text-text-tertiary text-center py-12">Loading…</div>
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={DollarSign}
          title={commissions.length === 0 ? 'No commission jobs yet' : 'No matches'}
          description={commissions.length === 0 ? 'Add a job to start tracking appointment-setting commissions.' : 'Try adjusting the filters.'}
          action={commissions.length === 0 ? { label: 'Add Commission Job', onClick: openNew } : undefined}
        />
      ) : (
        <motion.div variants={item} className="glass-panel overflow-x-auto">
          <table className="w-full min-w-[860px]">
            <thead>
              <tr className="border-b border-rim/[0.06] text-[11px] uppercase tracking-wider text-text-tertiary">
                <th className="text-left px-4 py-3 font-medium">Client</th>
                <th className="text-left px-3 py-3 font-medium">Type</th>
                <th className="text-left px-3 py-3 font-medium">Appt date</th>
                <th className="text-left px-3 py-3 font-medium">Closer</th>
                <th className="text-left px-3 py-3 font-medium">Status</th>
                <th className="text-right px-3 py-3 font-medium">kW / Contract</th>
                <th className="text-right px-3 py-3 font-medium">Commission</th>
                <th className="text-left px-3 py-3 font-medium">Payment</th>
                <th className="w-20"></th>
              </tr>
            </thead>
            <tbody>
              {filtered.map(c => (
                <tr key={c.id} className="border-b border-rim/[0.03] hover:bg-surface-200/30 transition-colors group">
                  <td className="px-4 py-3">
                    <div className="text-sm text-text-primary">{c.client_name}</div>
                    {c.notes && <div className="text-[11px] text-text-tertiary truncate max-w-[180px]">{c.notes}</div>}
                  </td>
                  <td className="px-3 py-3">
                    <span className="inline-flex items-center gap-1 text-xs text-text-secondary">
                      {c.job_type === 'solar' ? <Sun className="w-3.5 h-3.5 text-accent" /> : <Home className="w-3.5 h-3.5 text-status-complete" />}
                      {c.job_type === 'solar' ? 'Solar' : 'Roofing'}
                    </span>
                  </td>
                  <td className="px-3 py-3 text-sm text-text-secondary">{c.appointment_date ? formatDate(c.appointment_date) : '—'}</td>
                  <td className="px-3 py-3 text-sm text-text-secondary">{c.closer_name || '—'}</td>
                  <td className="px-3 py-3">
                    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium ${STATUS_STYLE[c.status]}`}>
                      {c.needs_review && c.manual_override == null && <AlertTriangle className="w-3 h-3 mr-1" />}
                      {statusLabel(c.status)}
                    </span>
                  </td>
                  <td className="px-3 py-3 text-right text-sm font-mono text-text-secondary">
                    {c.job_type === 'solar'
                      ? (c.system_size_kw != null ? `${c.system_size_kw} kW` : '—')
                      : (c.contract_amount != null ? formatMoney(c.contract_amount) : '—')}
                  </td>
                  <td className="px-3 py-3 text-right text-sm text-text-primary">{commissionCell(c)}</td>
                  <td className="px-3 py-3">
                    <span className={`inline-flex px-2 py-0.5 rounded-full text-[11px] font-medium ${PAYMENT_STYLE[c.payment_status]}`}>
                      {paymentLabel(c.payment_status)}
                    </span>
                  </td>
                  <td className="px-3 py-3">
                    <div className="flex items-center gap-1 justify-end opacity-0 group-hover:opacity-100 transition-opacity">
                      <button onClick={() => openEdit(c)} className="p-1.5 hover:bg-surface-300 rounded transition-colors" title="Edit">
                        <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
                      </button>
                      <button onClick={() => setDeleteId(c.id)} className="p-1.5 hover:bg-red-500/10 rounded transition-colors" title="Delete">
                        <Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </motion.div>
      )}

      {/* Add / Edit modal */}
      <Modal isOpen={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Commission Job' : 'Add Commission Job'}>
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Client Name *</label>
            <input className="input-field" value={form.client_name} onChange={e => update('client_name', e.target.value)} placeholder="Customer name" autoFocus />
          </div>

          {/* Job type toggle */}
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Job Type *</label>
            <div className="grid grid-cols-2 gap-2">
              {(['solar', 'roofing'] as CommissionJobType[]).map(t => (
                <button
                  key={t}
                  type="button"
                  onClick={() => update('job_type', t)}
                  className={`flex items-center justify-center gap-2 py-2 rounded-lg text-sm font-medium border transition-colors ${
                    form.job_type === t ? 'border-accent/40 bg-accent/10 text-accent' : 'border-rim/[0.08] text-text-secondary hover:bg-surface-200'
                  }`}
                >
                  {t === 'solar' ? <Sun className="w-4 h-4" /> : <Home className="w-4 h-4" />}
                  {t === 'solar' ? 'Solar' : 'Roofing'}
                </button>
              ))}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Appointment Date *</label>
              <input type="date" className="input-field" value={form.appointment_date} onChange={e => update('appointment_date', e.target.value)} />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Closer Name</label>
              <input className="input-field" value={form.closer_name} onChange={e => update('closer_name', e.target.value)} placeholder="Who closes the deal" />
            </div>
          </div>

          {/* Dynamic: solar kW vs roofing contract */}
          {form.job_type === 'solar' ? (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">System Size (kW)</label>
              <input type="number" step="0.01" min="0" className="input-field font-mono" value={form.system_size_kw}
                onChange={e => update('system_size_kw', e.target.value)} placeholder="e.g. 6.16" />
              <p className="text-[10px] text-text-tertiary mt-1">Commission = kW × $50</p>
            </div>
          ) : (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Contract Amount</label>
              <div className="relative">
                <span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span>
                <input type="number" step="0.01" min="0" className="input-field pl-7 font-mono" value={form.contract_amount}
                  onChange={e => update('contract_amount', e.target.value)} placeholder="e.g. 25000" />
              </div>
              <p className="text-[10px] text-text-tertiary mt-1">≤ $20k → $250 · ≥ $30k → $500 · in between → Needs Review unless overridden</p>
            </div>
          )}

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Status</label>
              <select className="input-field" value={form.status} onChange={e => update('status', e.target.value as CommissionStatus)}>
                {STATUS_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Payment Status</label>
              <select className="input-field" value={form.payment_status} onChange={e => update('payment_status', e.target.value as CommissionPaymentStatus)}>
                {PAYMENT_OPTIONS.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
              </select>
            </div>
          </div>

          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Manual Commission Override</label>
            <div className="relative">
              <span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span>
              <input type="number" step="0.01" min="0" className="input-field pl-7 font-mono" value={form.manual_override}
                onChange={e => update('manual_override', e.target.value)} placeholder="Leave blank to use the calculated amount" />
            </div>
          </div>

          {/* Live commission preview */}
          <div className="flex items-center justify-between p-3 rounded-lg bg-surface-200/50 border border-rim/[0.04]">
            <span className="text-xs text-text-secondary">Commission</span>
            <span className="text-sm font-mono font-semibold">
              {formPreview.review
                ? <span className="text-accent">Needs Review</span>
                : formPreview.value == null
                  ? <span className="text-text-tertiary">—</span>
                  : <span className="text-accent">{formatMoney(formPreview.value)}</span>}
            </span>
          </div>

          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Notes</label>
            <textarea className="input-field" rows={2} value={form.notes} onChange={e => update('notes', e.target.value)} placeholder="Anything worth remembering" />
          </div>

          <div className="flex justify-end gap-3 pt-1">
            <button onClick={() => setShowForm(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleSave} disabled={saving} className="btn-primary">
              {saving ? 'Saving…' : editing ? 'Save Changes' : 'Add Job'}
            </button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog
        isOpen={deleteId !== null}
        onClose={() => setDeleteId(null)}
        onConfirm={handleDelete}
        title="Delete Commission Job"
        message="Are you sure you want to delete this commission job? This cannot be undone."
      />
    </motion.div>
  )
}

function StatCard({ label, value, icon: Icon, accent, tone }: {
  label: string
  value: string
  icon: typeof DollarSign
  accent?: boolean
  tone?: 'paid' | 'overdue'
}) {
  const color = accent ? 'text-accent' : tone === 'paid' ? 'text-status-paid' : tone === 'overdue' ? 'text-status-overdue' : 'text-text-primary'
  return (
    <div className="glass-panel p-3.5">
      <div className="flex items-center gap-1.5 mb-1.5">
        <Icon className={`w-3.5 h-3.5 ${color}`} />
        <span className="text-[10px] uppercase tracking-wider text-text-tertiary">{label}</span>
      </div>
      <div className={`font-mono text-lg font-semibold ${color}`}>{value}</div>
    </div>
  )
}
