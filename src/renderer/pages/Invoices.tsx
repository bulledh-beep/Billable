import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import { Plus, FileText, Search } from 'lucide-react'
import EmptyState from '../components/EmptyState'
import StatusBadge from '../components/StatusBadge'
import { formatMoney, formatDate } from '../utils/format'
import type { Invoice } from '@shared/types'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

export default function Invoices() {
  const navigate = useNavigate()
  const [invoices, setInvoices] = useState<Invoice[]>([])
  const [statusFilter, setStatusFilter] = useState('all')
  const [search, setSearch] = useState('')

  useEffect(() => { loadData() }, [])

  const loadData = async () => {
    const data = await window.api.invoices.list()
    setInvoices(data)
  }

  const filtered = invoices.filter((i: any) => {
    const matchSearch = i.invoice_number.toLowerCase().includes(search.toLowerCase()) ||
      (i.client_name || '').toLowerCase().includes(search.toLowerCase())
    const matchStatus = statusFilter === 'all' || i.status === statusFilter
    return matchSearch && matchStatus
  })

  const totalOutstanding = invoices
    .filter((i: any) => ['sent', 'overdue'].includes(i.status))
    .reduce((s: number, i: any) => s + i.total, 0)

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Invoices</h1>
          <p className="text-sm text-text-secondary mt-1">
            {invoices.length} invoices · {formatMoney(totalOutstanding)} outstanding
          </p>
        </div>
        <button onClick={() => navigate('/invoices/new')} className="btn-primary flex items-center gap-2">
          <Plus className="w-4 h-4" /> New Invoice
        </button>
      </motion.div>

      {invoices.length > 0 && (
        <motion.div variants={item} className="flex gap-3 mb-6">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
            <input
              type="text"
              placeholder="Search invoices..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="input-field pl-10"
            />
          </div>
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
                </div>
                <div className="text-xs text-text-tertiary mt-1">
                  {invoice.client_name} · {invoice.project_name || 'Multiple projects'}
                </div>
              </div>
              <div className="text-right">
                <div className="font-mono text-sm font-medium text-text-primary">{formatMoney(invoice.total)}</div>
                <div className="text-xs text-text-tertiary">
                  Due {formatDate(invoice.due_date)}
                </div>
              </div>
            </div>
          ))}
        </motion.div>
      )}
    </motion.div>
  )
}
