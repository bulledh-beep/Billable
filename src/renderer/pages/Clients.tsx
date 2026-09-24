import { useState, useEffect, useMemo } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Users, Pencil, Trash2, Merge, Copy } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import SearchInput from '../components/SearchInput'
import Menu from '../components/Menu'
import ClientForm, { type ClientFormValues } from '../components/ClientForm'
import MergeClientsModal from '../components/MergeClientsModal'
import { getInitials, formatMoney, relativeDays } from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import type { AttentionItem, Client } from '@shared/types'
import toast from 'react-hot-toast'

const AVATAR_COLORS = ['#FF9600', '#1CB0F6', '#58CC02', '#CE82FF', '#FF4B4B', '#2B70C9', '#FFC800', '#00CD9C']

/** A bright circle with the client's initials. */
export function ClientAvatar({ name, size = 32 }: { name: string; size?: number }) {
  let hash = 0
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash)
  const color = AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length]
  return (
    <div
      className="rounded-full flex items-center justify-center font-bold text-white shrink-0"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.4), backgroundColor: color }}
    >
      {getInitials(name)}
    </div>
  )
}

export default function Clients() {
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const [clients, setClients] = useState<Client[]>([])
  const [duplicates, setDuplicates] = useState<AttentionItem[]>([])
  const [search, setSearch] = useState('')
  const [formOpen, setFormOpen] = useState(false)
  const [editClient, setEditClient] = useState<Client | null>(null)
  const [deleteClient, setDeleteClient] = useState<Client | null>(null)
  const [merge, setMerge] = useState<{ source: number; target?: number } | null>(null)

  useEffect(() => { loadClients() }, [])

  // ⌘N from the menu
  useEffect(() => {
    if (searchParams.get('action') === 'new') {
      setEditClient(null)
      setFormOpen(true)
      setSearchParams({}, { replace: true })
    }
  }, [searchParams, setSearchParams])

  const loadClients = async () => {
    const [data, overview] = await Promise.all([window.api.clients.list(), window.api.billing.overview()])
    setClients(data)
    setDuplicates(overview.attention.filter((a: AttentionItem) => a.kind === 'duplicate_clients'))
  }

  const handleSave = async (values: ClientFormValues) => {
    if (editClient) {
      const result: any = await window.api.clients.update(editClient.id, values)
      const cascaded = result?.cascaded_projects || 0
      toast.success(cascaded > 0 ? `Client updated · ${cascaded} project${cascaded === 1 ? '' : 's'} moved to the new rate` : 'Client updated')
    } else {
      const created = await window.api.clients.create(values)
      toast.success('Client created')
      setFormOpen(false)
      navigate(`/clients/${created.id}`)
      return
    }
    setFormOpen(false)
    notifyBillingChanged()
    loadClients()
  }

  const handleDelete = async () => {
    if (!deleteClient) return
    await window.api.clients.delete(deleteClient.id)
    toast.success('Client deleted')
    setDeleteClient(null)
    notifyBillingChanged()
    loadClients()
  }

  // Keep the record with more history; fold the other one into it
  const suggestMerge = ([a, b]: number[]) => {
    const weight = (id: number) => {
      const c = clients.find(x => x.id === id)
      return (c?.invoice_count || 0) * 10 + (c?.project_count || 0)
    }
    return weight(a) >= weight(b) ? { source: b, target: a } : { source: a, target: b }
  }

  const dismissDuplicate = async (key: string) => {
    await window.api.billing.dismiss(key)
    setDuplicates(d => d.filter(x => x.key !== key))
  }

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return clients.filter(c =>
      !q || c.name.toLowerCase().includes(q) || (c.company || '').toLowerCase().includes(q) || (c.email || '').toLowerCase().includes(q))
  }, [clients, search])

  const totals = useMemo(() => ({
    unbilled: clients.reduce((s, c) => s + (c.unbilled_amount || 0), 0),
    outstanding: clients.reduce((s, c) => s + (c.outstanding_amount || 0), 0),
    paid: clients.reduce((s, c) => s + (c.paid_amount || 0), 0),
  }), [clients])

  return (
    <div className="page">
      <PageHeader
        title="Clients"
        actions={<button onClick={() => { setEditClient(null); setFormOpen(true) }} className="btn-secondary">New client</button>}
      />

      {duplicates.map(d => (
        <div key={d.key} className="card mb-4 px-4 py-3 flex items-center gap-3">
          <Copy className="w-4 h-4 text-fg-3 shrink-0" />
          <p className="text-sm text-fg-2 flex-1">
            <b className="text-fg">{d.client_names?.[0]}</b> and <b className="text-fg">{d.client_names?.[1]}</b> look like the same client.
          </p>
          <button onClick={() => dismissDuplicate(d.key)} className="btn-ghost btn-sm">They're different</button>
          <button onClick={() => setMerge(suggestMerge(d.client_ids!))} className="btn-secondary btn-sm">
            Merge
          </button>
        </div>
      ))}

      {clients.length === 0 ? (
        <div className="card">
          <EmptyState
            icon={Users}
            title="Add your first client"
            description="Clients hold your rate, billing address, and projects."
            action={{ label: 'New client', onClick: () => { setEditClient(null); setFormOpen(true) } }}
          />
        </div>
      ) : (
        <>
          <div className="flex items-center gap-3 mb-4">
            <div className="text-sm text-fg-3">
              {clients.length} client{clients.length === 1 ? '' : 's'}
              {totals.unbilled > 0 && <> · <span className="num text-amber font-medium">{formatMoney(totals.unbilled)}</span> unbilled</>}
              {totals.outstanding > 0 && <> · <span className="num text-blue font-medium">{formatMoney(totals.outstanding)}</span> outstanding</>}
            </div>
            <SearchInput value={search} onChange={setSearch} placeholder="Search clients" className="ml-auto w-72" />
          </div>

          <div className="card">
            <table className="tbl">
              <thead>
                <tr>
                  <th>Client</th>
                  <th className="text-right">Projects</th>
                  <th className="text-right">Unbilled</th>
                  <th className="text-right">Outstanding</th>
                  <th className="text-right">Paid</th>
                  <th className="text-right">Rate</th>
                  <th>Last tracked</th>
                  <th className="w-10"></th>
                </tr>
              </thead>
              <tbody>
                {filtered.map(client => (
                  <tr key={client.id} onClick={() => navigate(`/clients/${client.id}`)} className="row-hover group">
                    <td>
                      <div className="flex items-center gap-3 min-w-0">
                        <ClientAvatar name={client.name} />
                        <div className="min-w-0">
                          <div className="text-sm font-medium text-fg truncate">{client.name}</div>
                          <div className="text-xs text-fg-3 truncate max-w-[260px]">{client.company || client.email || '—'}</div>
                        </div>
                      </div>
                    </td>
                    <td className="text-right num text-fg-2">
                      {client.project_count || 0}
                      {(client.active_project_count || 0) > 0 && <span className="text-fg-4"> · {client.active_project_count} active</span>}
                    </td>
                    <td className="text-right num">
                      {(client.unbilled_amount || 0) > 0 ? <span className="font-medium text-amber">{formatMoney(client.unbilled_amount || 0)}</span> : <span className="text-fg-4">—</span>}
                    </td>
                    <td className="text-right num">
                      {(client.outstanding_amount || 0) > 0
                        ? <span className={`font-medium ${(client.overdue_amount || 0) > 0 ? 'text-red' : 'text-blue'}`}>{formatMoney(client.outstanding_amount || 0)}</span>
                        : <span className="text-fg-4">—</span>}
                    </td>
                    <td className="text-right num text-fg-2">
                      {(client.paid_amount || 0) > 0 ? formatMoney(client.paid_amount || 0) : <span className="text-fg-4">—</span>}
                    </td>
                    <td className="text-right num text-fg-2">{formatMoney(client.default_rate)}</td>
                    <td className="text-fg-3 whitespace-nowrap">{client.last_activity ? relativeDays(client.last_activity) : '—'}</td>
                    <td onClick={e => e.stopPropagation()}>
                      <Menu
                        items={[
                          { label: 'Edit', icon: Pencil, onClick: () => { setEditClient(client); setFormOpen(true) } },
                          clients.length > 1 && { label: 'Merge into another client…', icon: Merge, onClick: () => setMerge({ source: client.id }) },
                          'separator',
                          { label: 'Delete', icon: Trash2, danger: true, onClick: () => setDeleteClient(client) },
                        ]}
                      />
                    </td>
                  </tr>
                ))}
                {filtered.length === 0 && (
                  <tr><td colSpan={8} className="text-center text-fg-3 !h-20">No clients match</td></tr>
                )}
              </tbody>
            </table>
          </div>
        </>
      )}

      <ClientForm open={formOpen} client={editClient} onClose={() => setFormOpen(false)} onSubmit={handleSave} />

      <MergeClientsModal
        open={!!merge}
        clients={clients}
        sourceId={merge?.source ?? null}
        targetId={merge?.target ?? null}
        onClose={() => setMerge(null)}
        onMerged={() => { setMerge(null); loadClients() }}
      />

      <ConfirmDialog
        isOpen={deleteClient !== null}
        onClose={() => setDeleteClient(null)}
        onConfirm={handleDelete}
        title={`Delete ${deleteClient?.name}?`}
        message="This permanently deletes the client with all of their projects, time entries, and invoices. If they were added twice, merge them instead."
        confirmText="Delete client"
      />
    </div>
  )
}
