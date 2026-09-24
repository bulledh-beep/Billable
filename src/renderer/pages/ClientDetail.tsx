import { useState, useEffect, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { Mail, MapPin, FileText, Trash2, Merge, Plus, FolderKanban, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import StatusBadge from '../components/StatusBadge'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import Menu from '../components/Menu'
import ConfirmDialog from '../components/ConfirmDialog'
import ClientForm, { type ClientFormValues } from '../components/ClientForm'
import MergeClientsModal from '../components/MergeClientsModal'
import ProjectForm, { type ProjectFormValues } from '../components/ProjectForm'
import EmptyState from '../components/EmptyState'
import { ClientAvatar } from './Clients'
import { formatMoney, formatDay, formatHoursShort, relativeDays } from '../utils/format'
import { notifyBillingChanged, onBillingChanged } from '../utils/events'
import type { Client, Project, Invoice } from '@shared/types'
import toast from 'react-hot-toast'

export default function ClientDetail() {
  const { id } = useParams()
  const navigate = useNavigate()
  const clientId = id ? parseInt(id) : 0
  const [client, setClient] = useState<Client | null>(null)
  const [clients, setClients] = useState<Client[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [invoices, setInvoices] = useState<Invoice[]>([])
  const [editOpen, setEditOpen] = useState(false)
  const [projectOpen, setProjectOpen] = useState(false)
  const [mergeOpen, setMergeOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)

  useEffect(() => { if (clientId) loadData() }, [clientId])
  useEffect(() => onBillingChanged(() => { if (clientId) loadData() }), [clientId])

  const loadData = async () => {
    const [c, all, p, inv] = await Promise.all([
      window.api.clients.get(clientId),
      window.api.clients.list(),
      window.api.projects.list(clientId),
      window.api.invoices.list(),
    ])
    setClient(c)
    setClients(all)
    setProjects(p)
    setInvoices(inv.filter((i: Invoice) => i.client_id === clientId))
  }

  const sortedProjects = useMemo(() => [...projects].sort((a, b) =>
    (b.unbilled_amount || 0) - (a.unbilled_amount || 0) ||
    String(b.last_activity || b.created_at).localeCompare(String(a.last_activity || a.created_at))), [projects])

  if (!client) return null

  const handleSave = async (values: ClientFormValues) => {
    const result: any = await window.api.clients.update(client.id, values)
    const cascaded = result?.cascaded_projects || 0
    toast.success(cascaded > 0 ? `Client updated · ${cascaded} project${cascaded === 1 ? '' : 's'} moved to the new rate` : 'Client updated')
    setEditOpen(false)
    notifyBillingChanged()
    loadData()
  }

  const handleCreateProject = async (values: ProjectFormValues) => {
    const created = await window.api.projects.create(values)
    toast.success('Project created')
    setProjectOpen(false)
    navigate(`/projects/${created.id}`)
  }

  const handleDelete = async () => {
    await window.api.clients.delete(client.id)
    toast.success('Client deleted')
    notifyBillingChanged()
    navigate('/clients')
  }

  const unbilled = client.unbilled_amount || 0
  const outstanding = client.outstanding_amount || 0
  const overdue = client.overdue_amount || 0
  const paidCount = invoices.filter(i => i.status === 'paid').length

  return (
    <div className="page">
      <PageHeader
        crumbs={[{ label: 'Clients', to: '/clients' }]}
        title={client.name}
        actions={
          <>
            {unbilled > 0 && (
              <button onClick={() => navigate(`/invoices/new?client_id=${client.id}`)} className="btn-secondary">
                Invoice unbilled time
              </button>
            )}
            <button onClick={() => setEditOpen(true)} className="btn-secondary">Edit</button>
            <Menu
              items={[
                { label: 'New project', icon: Plus, onClick: () => setProjectOpen(true) },
                clients.length > 1 && { label: 'Merge into another client…', icon: Merge, onClick: () => setMergeOpen(true) },
                'separator',
                { label: 'Delete client', icon: Trash2, danger: true, onClick: () => setDeleteOpen(true) },
              ]}
            />
          </>
        }
      />

      {/* Identity */}
      <div className="flex items-center gap-4 mb-5">
        <ClientAvatar name={client.name} size={40} />
        <div className="min-w-0">
          <div className="text-[17px] leading-6 font-semibold text-fg">{client.name}</div>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-fg-3">
            {client.company && <span>{client.company}</span>}
            {client.email && <span className="flex items-center gap-1.5"><Mail className="w-3.5 h-3.5" />{client.email}</span>}
            {client.address && <span className="flex items-center gap-1.5 truncate max-w-[420px]"><MapPin className="w-3.5 h-3.5 shrink-0" />{client.address.split('\n').map(l => l.trim().replace(/,$/, '')).filter(Boolean).join(', ')}</span>}
          </div>
        </div>
      </div>

      <MetricStrip className="mb-5">
        <Metric
          label="Unbilled"
          value={<Money amount={unbilled} />}
          sub={unbilled > 0 ? `${formatHoursShort(client.unbilled_hours || 0)} since ${formatDay(client.oldest_unbilled || '')}` : 'Nothing waiting'}
        />
        <Metric
          label="Outstanding"
          value={<Money amount={outstanding} className={overdue > 0 ? 'text-red' : ''} />}
          sub={overdue > 0 ? `${formatMoney(overdue)} overdue` : outstanding > 0 ? 'Sent, not yet paid' : 'Nothing owed'}
        />
        <Metric label="Paid" value={<Money amount={client.paid_amount || 0} />} sub={`${paidCount} invoice${paidCount === 1 ? '' : 's'}`} />
        <Metric label="Rate" value={<><Money amount={client.default_rate} /><span className="text-sm font-normal text-fg-3">/hr</span></>} sub={client.currency} />
      </MetricStrip>

      <div className="grid grid-cols-2 gap-6 items-start">
        {/* Projects */}
        <section className="min-w-0">
          <div className="group-head">
            <h2 className="section-title">Projects <span className="ml-1 font-normal text-fg-3 num">{sortedProjects.length}</span></h2>
            <button onClick={() => setProjectOpen(true)} className="head-link">New project</button>
          </div>
          <div className="card overflow-hidden">
          {sortedProjects.length === 0 ? (
            <EmptyState compact icon={FolderKanban} title="No projects yet" description="Create a project to start tracking time for this client." />
          ) : sortedProjects.map(p => (
            <button
              key={p.id}
              onClick={() => navigate(`/projects/${p.id}`)}
              className="list-row [--inset:36px] w-full flex items-center gap-3 px-4 h-[44px] hover:bg-fg/[0.025] text-left"
            >
              <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: p.color }} />
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium text-fg truncate">{p.name}</div>
                <div className="text-xs text-fg-3">
                  {formatHoursShort(p.total_hours || 0)} tracked{p.last_activity ? ` · ${relativeDays(p.last_activity)}` : ''}
                </div>
              </div>
              {(p.unbilled_amount || 0) > 0 && (
                <div className="text-right">
                  <div className="text-[13px] num font-medium text-amber">{formatMoney(p.unbilled_amount || 0)}</div>
                  <div className="text-2xs text-fg-3">unbilled</div>
                </div>
              )}
              <StatusBadge status={p.status} />
            </button>
          ))}
          </div>
        </section>

        {/* Invoices */}
        <section className="min-w-0">
          <div className="group-head">
            <h2 className="section-title">Invoices <span className="ml-1 font-normal text-fg-3 num">{invoices.length}</span></h2>
            <button onClick={() => navigate(`/invoices/new?client_id=${client.id}`)} className="head-link">New invoice</button>
          </div>
          <div className="card overflow-hidden">
          {invoices.length === 0 ? (
            <EmptyState compact icon={FileText} title="No invoices yet" description="Invoices you create for this client show up here." />
          ) : invoices.map(inv => (
            <button
              key={inv.id}
              onClick={() => navigate(`/invoices/${inv.id}`)}
              className="list-row group w-full flex items-center gap-3 px-4 h-[44px] hover:bg-fg/[0.025] text-left"
            >
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium text-fg">{inv.invoice_number}</div>
                <div className="text-xs text-fg-3 truncate">
                  {formatDay(inv.issue_date)}
                  {inv.status === 'paid' && inv.payment_date ? ` · paid ${formatDay(inv.payment_date)}` : inv.status !== 'draft' ? ` · due ${formatDay(inv.due_date)}` : ''}
                </div>
              </div>
              <div className="text-[13px] num text-fg">{formatMoney(inv.total)}</div>
              <StatusBadge status={inv.status} />
              <ChevronRight className="w-3.5 h-3.5 text-fg-4 group-hover:text-fg-3" />
            </button>
          ))}
          </div>
        </section>
      </div>

      <ClientForm open={editOpen} client={client} onClose={() => setEditOpen(false)} onSubmit={handleSave} />
      <ProjectForm open={projectOpen} clients={clients} defaultClientId={client.id} onClose={() => setProjectOpen(false)} onSubmit={handleCreateProject} />
      <MergeClientsModal
        open={mergeOpen}
        clients={clients}
        sourceId={client.id}
        onClose={() => setMergeOpen(false)}
        onMerged={target => { setMergeOpen(false); navigate(`/clients/${target}`) }}
      />
      <ConfirmDialog
        isOpen={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        onConfirm={handleDelete}
        title={`Delete ${client.name}?`}
        message="This permanently deletes the client with all of their projects, time entries, and invoices. If they were added twice, merge them instead."
        confirmText="Delete client"
      />
    </div>
  )
}
