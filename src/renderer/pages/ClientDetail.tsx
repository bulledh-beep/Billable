import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowLeft, Mail, MapPin, DollarSign } from 'lucide-react'
import StatusBadge from '../components/StatusBadge'
import { getInitials, getAvatarColor, formatMoney, formatHours, formatDate } from '../utils/format'
import type { Client, Project, Invoice } from '@shared/types'

export default function ClientDetail() {
  const { id } = useParams()
  const navigate = useNavigate()
  const [client, setClient] = useState<Client | null>(null)
  const [projects, setProjects] = useState<Project[]>([])
  const [invoices, setInvoices] = useState<Invoice[]>([])

  useEffect(() => {
    if (id) loadData(parseInt(id))
  }, [id])

  const loadData = async (clientId: number) => {
    const [c, p, allInvoices] = await Promise.all([
      window.api.clients.get(clientId),
      window.api.projects.list(clientId),
      window.api.invoices.list(),
    ])
    setClient(c)
    setProjects(p)
    setInvoices(allInvoices.filter((inv: any) => inv.client_id === clientId))
  }

  if (!client) return null

  const totalBilled = invoices
    .filter((i: any) => i.status === 'paid')
    .reduce((sum: number, i: any) => sum + i.total, 0)
  const totalOutstanding = invoices
    .filter((i: any) => ['sent', 'overdue'].includes(i.status))
    .reduce((sum: number, i: any) => sum + i.total, 0)

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="p-8">
      <button
        onClick={() => navigate('/clients')}
        className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary transition-colors mb-6"
      >
        <ArrowLeft className="w-4 h-4" /> Clients
      </button>

      {/* Client Header */}
      <div className="flex items-start gap-5 mb-8">
        <div
          className="w-14 h-14 rounded-2xl flex items-center justify-center text-lg font-semibold text-white"
          style={{ backgroundColor: getAvatarColor(client.name) }}
        >
          {getInitials(client.name)}
        </div>
        <div className="flex-1">
          <h1 className="text-2xl font-bold text-text-primary">{client.name}</h1>
          {client.company && <p className="text-sm text-text-secondary">{client.company}</p>}
          <div className="flex items-center gap-4 mt-2 text-xs text-text-tertiary">
            {client.email && (
              <span className="flex items-center gap-1"><Mail className="w-3 h-3" /> {client.email}</span>
            )}
            {client.address && (
              <span className="flex items-center gap-1"><MapPin className="w-3 h-3" /> {client.address}</span>
            )}
            <span className="flex items-center gap-1">
              <DollarSign className="w-3 h-3" /> {formatMoney(client.default_rate)}/hr
            </span>
          </div>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-3 gap-4 mb-8">
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Lifetime Billed</div>
          <div className="font-mono text-lg font-semibold text-status-paid">{formatMoney(totalBilled)}</div>
        </div>
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Outstanding</div>
          <div className="font-mono text-lg font-semibold text-status-overdue">{formatMoney(totalOutstanding)}</div>
        </div>
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Projects</div>
          <div className="font-mono text-lg font-semibold text-text-primary">{projects.length}</div>
        </div>
      </div>

      {/* Projects */}
      <h2 className="text-sm font-semibold text-text-primary mb-3">Projects</h2>
      <div className="space-y-2 mb-8">
        {projects.map((project: any) => (
          <div
            key={project.id}
            onClick={() => navigate(`/projects/${project.id}`)}
            className="glass-panel-hover p-4 flex items-center gap-3 cursor-pointer"
          >
            <div className="w-3 h-3 rounded-full" style={{ backgroundColor: project.color }} />
            <div className="flex-1">
              <span className="text-sm font-medium text-text-primary">{project.name}</span>
            </div>
            <StatusBadge status={project.status} />
            <span className="font-mono text-sm text-text-secondary ml-4">
              {formatHours(project.total_hours || 0)}h
            </span>
          </div>
        ))}
        {projects.length === 0 && (
          <p className="text-sm text-text-tertiary text-center py-4">No projects for this client</p>
        )}
      </div>

      {/* Invoices */}
      <h2 className="text-sm font-semibold text-text-primary mb-3">Invoices</h2>
      <div className="space-y-2">
        {invoices.map((invoice: any) => (
          <div
            key={invoice.id}
            onClick={() => navigate(`/invoices/${invoice.id}`)}
            className="glass-panel-hover p-4 flex items-center gap-3 cursor-pointer"
          >
            <div className="flex-1">
              <span className="text-sm font-medium text-text-primary">{invoice.invoice_number}</span>
              <span className="text-xs text-text-tertiary ml-3">{formatDate(invoice.issue_date)}</span>
            </div>
            <StatusBadge status={invoice.status} />
            <span className="font-mono text-sm font-medium text-text-primary ml-4">
              {formatMoney(invoice.total)}
            </span>
          </div>
        ))}
        {invoices.length === 0 && (
          <p className="text-sm text-text-tertiary text-center py-4">No invoices for this client</p>
        )}
      </div>
    </motion.div>
  )
}
