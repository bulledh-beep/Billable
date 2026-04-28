import { useState, useEffect, useMemo } from 'react'
import { motion } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import { Plus, FileText, Search, Download } from 'lucide-react'
import EmptyState from '../components/EmptyState'
import StatusBadge from '../components/StatusBadge'
import { formatMoney, formatDate } from '../utils/format'
import type { Invoice } from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

export default function Invoices() {
  const navigate = useNavigate()
  const [invoices, setInvoices] = useState<Invoice[]>([])
  const [statusFilter, setStatusFilter] = useState('all')
  const [yearFilter, setYearFilter] = useState<'all' | number>('all')
  const [search, setSearch] = useState('')

  useEffect(() => { loadData() }, [])

  const loadData = async () => {
    const data = await window.api.invoices.list()
    setInvoices(data)
  }

  // Available tax years across all invoices (descending), with current year always included
  const availableYears = useMemo(() => {
    const years = new Set<number>()
    const currentYear = new Date().getFullYear()
    years.add(currentYear)
    invoices.forEach((i: any) => {
      const y = i.tax_year || (i.issue_date ? new Date(i.issue_date).getFullYear() : null)
      if (y) years.add(y)
    })
    return Array.from(years).sort((a, b) => b - a)
  }, [invoices])

  const filtered = useMemo(() => {
    return invoices.filter((i: any) => {
      const matchSearch = i.invoice_number.toLowerCase().includes(search.toLowerCase()) ||
        (i.client_name || '').toLowerCase().includes(search.toLowerCase())
      const matchStatus = statusFilter === 'all' || i.status === statusFilter
      const invYear = i.tax_year || (i.issue_date ? new Date(i.issue_date).getFullYear() : null)
      const matchYear = yearFilter === 'all' || invYear === yearFilter
      return matchSearch && matchStatus && matchYear
    })
  }, [invoices, search, statusFilter, yearFilter])

  // Summary metrics for the filtered set
  const summary = useMemo(() => {
    const totalInvoiced = filtered.reduce((s: number, i: any) => s + (i.total || 0), 0)
    const totalGST = filtered.reduce((s: number, i: any) => s + (i.gst_hst_amount || 0), 0)
    const totalPaid = filtered
      .filter((i: any) => i.status === 'paid')
      .reduce((s: number, i: any) => s + (i.total || 0), 0)
    const totalOutstanding = filtered
      .filter((i: any) => ['sent', 'overdue'].includes(i.status))
      .reduce((s: number, i: any) => s + (i.total || 0), 0)
    return { totalInvoiced, totalGST, totalPaid, totalOutstanding }
  }, [filtered])

  const handleExportCSV = async () => {
    if (filtered.length === 0) {
      toast.error('No invoices to export')
      return
    }
    const rows = filtered.map((i: any) => ({
      invoice_number: i.invoice_number,
      issue_date: i.issue_date,
      due_date: i.due_date,
      tax_year: i.tax_year || '',
      client_name: i.client_name || '',
      project_name: i.project_name || '',
      status: i.status,
      subtotal: (i.subtotal || 0).toFixed(2),
      gst_hst_rate: i.gst_hst_applicable ? (i.gst_hst_rate || 0) : '',
      gst_hst_amount: i.gst_hst_applicable ? (i.gst_hst_amount || 0).toFixed(2) : '',
      other_tax_rate: i.tax_rate || 0,
      total: (i.total || 0).toFixed(2),
      currency: i.currency || 'CAD',
      payment_date: i.payment_date || '',
      payment_method: i.payment_method || '',
    }))
    const filename = yearFilter === 'all'
      ? 'invoices-all.csv'
      : `invoices-${yearFilter}.csv`
    const result = await window.api.reports.exportCSV(rows, filename)
    if (result) toast.success('Invoices exported')
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Invoices</h1>
          <p className="text-sm text-text-secondary mt-1">
            {invoices.length} invoices · {formatMoney(summary.totalOutstanding)} outstanding
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleExportCSV}
            className="btn-secondary flex items-center gap-2"
            title="Export filtered invoices to CSV"
          >
            <Download className="w-4 h-4" /> Export CSV
          </button>
          <button onClick={() => navigate('/invoices/new')} className="btn-primary flex items-center gap-2">
            <Plus className="w-4 h-4" /> New Invoice
          </button>
        </div>
      </motion.div>

      {/* Tax Summary Bar */}
      {filtered.length > 0 && (
        <motion.div variants={item} className="grid grid-cols-4 gap-3 mb-6">
          <SummaryCard label="Invoiced" value={formatMoney(summary.totalInvoiced)} />
          <SummaryCard label="GST/HST Collected" value={formatMoney(summary.totalGST)} accent />
          <SummaryCard label="Paid" value={formatMoney(summary.totalPaid)} tone="paid" />
          <SummaryCard label="Outstanding" value={formatMoney(summary.totalOutstanding)} tone="outstanding" />
        </motion.div>
      )}

      {invoices.length > 0 && (
        <motion.div variants={item} className="flex flex-wrap gap-3 mb-6">
          <div className="relative flex-1 min-w-[200px]">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
            <input
              type="text"
              placeholder="Search invoices..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="input-field pl-10"
            />
          </div>
          <select
            className="input-field w-32"
            value={String(yearFilter)}
            onChange={e => setYearFilter(e.target.value === 'all' ? 'all' : parseInt(e.target.value))}
            title="Tax year"
          >
            <option value="all">All years</option>
            {availableYears.map(y => (
              <option key={y} value={y}>{y}</option>
            ))}
          </select>
          <div className="flex gap-1 bg-surface-100 rounded-lg p-0.5 border border-white/[0.04]">
            {['all', 'draft', 'sent', 'paid', 'overdue'].map(s => (
              <button
                key={s}
                onClick={() => setStatusFilter(s)}
                className={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors capitalize ${
                  statusFilter === s ? 'bg-surface-300 text-text-primary' : 'text-text-tertiary hover:text-text-secondary'
                }`}
              >
                {s}
              </button>
            ))}
          </div>
        </motion.div>
      )}

      {filtered.length === 0 && invoices.length === 0 ? (
        <EmptyState
          icon={FileText}
          title="No invoices yet"
          description="Create your first invoice from a project's unbilled hours."
          action={{ label: 'Create Invoice', onClick: () => navigate('/invoices/new') }}
        />
      ) : filtered.length === 0 ? (
        <p className="text-sm text-text-tertiary text-center py-12">No invoices match these filters</p>
      ) : (
        <motion.div variants={item} className="space-y-2">
          {filtered.map((invoice: any) => (
            <div
              key={invoice.id}
              onClick={() => navigate(`/invoices/${invoice.id}`)}
              className="glass-panel-hover p-4 flex items-center gap-4 cursor-pointer"
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-3">
                  <span className="text-sm font-medium text-text-primary">{invoice.invoice_number}</span>
                  <StatusBadge status={invoice.status} />
                  {invoice.gst_hst_applicable ? (
                    <span className="text-[10px] font-mono text-accent/80 px-1.5 py-0.5 rounded bg-accent/10">
                      GST {invoice.gst_hst_rate}%
                    </span>
                  ) : null}
                </div>
                <div className="text-xs text-text-tertiary mt-1">
                  {invoice.client_name} · {invoice.project_name || 'Multiple projects'}
                  {invoice.status === 'paid' && invoice.payment_method && (
                    <span className="ml-2 text-status-paid">· {invoice.payment_method}</span>
                  )}
                </div>
              </div>
              <div className="text-right">
                <div className="font-mono text-sm font-medium text-text-primary">{formatMoney(invoice.total)}</div>
                <div className="text-xs text-text-tertiary">
                  {invoice.status === 'paid' && invoice.payment_date
                    ? `Paid ${formatDate(invoice.payment_date)}`
                    : `Due ${formatDate(invoice.due_date)}`}
                </div>
              </div>
            </div>
          ))}
        </motion.div>
      )}
    </motion.div>
  )
}

function SummaryCard({
  label,
  value,
  accent,
  tone,
}: {
  label: string
  value: string
  accent?: boolean
  tone?: 'paid' | 'outstanding'
}) {
  const valueColor = accent
    ? 'text-accent'
    : tone === 'paid'
      ? 'text-status-paid'
      : tone === 'outstanding'
        ? 'text-status-overdue'
        : 'text-text-primary'

  return (
    <div className="glass-panel p-4">
      <div className="text-[10px] uppercase tracking-wider text-text-tertiary mb-1.5">{label}</div>
      <div className={`font-mono text-lg font-semibold ${valueColor}`}>{value}</div>
    </div>
  )
}
