import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import { Plus, Users, Search, ChevronRight, Trash2, Pencil } from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import { getInitials, getAvatarColor, formatMoney } from '../utils/format'
import type { Client } from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

export default function Clients() {
  const navigate = useNavigate()
  const [clients, setClients] = useState<Client[]>([])
  const [search, setSearch] = useState('')
  const [showForm, setShowForm] = useState(false)
  const [editClient, setEditClient] = useState<Client | null>(null)
  const [deleteId, setDeleteId] = useState<number | null>(null)
  const [form, setForm] = useState({
    name: '', company: '', email: '', address: '', default_rate: 100, currency: 'USD',
  })

  useEffect(() => { loadClients() }, [])

  const loadClients = async () => {
    const data = await window.api.clients.list()
    setClients(data)
  }

  const openNew = () => {
    setEditClient(null)
    setForm({ name: '', company: '', email: '', address: '', default_rate: 100, currency: 'USD' })
    setShowForm(true)
  }

  const openEdit = (client: Client, e: React.MouseEvent) => {
    e.stopPropagation()
    setEditClient(client)
    setForm({
      name: client.name,
      company: client.company,
      email: client.email,
      address: client.address,
      default_rate: client.default_rate,
      currency: client.currency,
    })
    setShowForm(true)
  }

  const handleSave = async () => {
    if (!form.name.trim()) return toast.error('Name is required')
    if (editClient) {
      const result: any = await window.api.clients.update(editClient.id, form)
      const cascaded = result?.cascaded_projects || 0
      if (cascaded > 0) {
        toast.success(`Client updated · ${cascaded} project${cascaded === 1 ? '' : 's'} repriced`)
      } else {
        toast.success('Client updated')
      }
    } else {
      await window.api.clients.create(form)
      toast.success('Client created')
    }
    setShowForm(false)
    loadClients()
  }

  const handleDelete = async () => {
    if (deleteId) {
      await window.api.clients.delete(deleteId)
      toast.success('Client deleted')
      setDeleteId(null)
      loadClients()
    }
  }

  const filtered = clients.filter(c =>
    c.name.toLowerCase().includes(search.toLowerCase()) ||
    c.company.toLowerCase().includes(search.toLowerCase())
  )

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Clients</h1>
          <p className="text-sm text-text-secondary mt-1">{clients.length} total clients</p>
        </div>
        <button onClick={openNew} className="btn-primary flex items-center gap-2">
          <Plus className="w-4 h-4" /> Add Client
        </button>
      </motion.div>

      {clients.length > 0 && (
        <motion.div variants={item} className="mb-6">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
            <input
              type="text"
              placeholder="Search clients..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="input-field pl-10"
            />
          </div>
        </motion.div>
      )}

      {filtered.length === 0 && clients.length === 0 ? (
        <EmptyState
          icon={Users}
          title="No clients yet"
          description="Add your first client to start tracking time and billing."
          action={{ label: 'Add Client', onClick: openNew }}
        />
      ) : filtered.length === 0 ? (
        <p className="text-sm text-text-tertiary text-center py-8">No clients match your search</p>
      ) : (
        <motion.div variants={item} className="space-y-2">
          {filtered.map(client => (
            <div
              key={client.id}
              onClick={() => navigate(`/clients/${client.id}`)}
              className="glass-panel-hover p-4 flex items-center gap-4 cursor-pointer group"
            >
              <div
                className="w-10 h-10 rounded-xl flex items-center justify-center text-sm font-semibold text-white flex-shrink-0"
                style={{ backgroundColor: getAvatarColor(client.name) }}
              >
                {getInitials(client.name)}
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-sm font-medium text-text-primary">{client.name}</div>
                <div className="text-xs text-text-tertiary">{client.company || client.email}</div>
              </div>
              <div className="text-right mr-2">
                <div className="font-mono text-sm text-text-secondary">
                  {formatMoney(client.default_rate)}/hr
                </div>
                <div className="text-xs text-text-tertiary">{client.currency}</div>
              </div>
              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                <button
                  onClick={(e) => openEdit(client, e)}
                  className="p-1.5 hover:bg-surface-300 rounded-lg transition-colors"
                >
                  <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
                </button>
                <button
                  onClick={(e) => { e.stopPropagation(); setDeleteId(client.id) }}
                  className="p-1.5 hover:bg-red-500/10 rounded-lg transition-colors"
                >
                  <Trash2 className="w-3.5 h-3.5 text-red-400" />
                </button>
              </div>
              <ChevronRight className="w-4 h-4 text-text-tertiary" />
            </div>
          ))}
        </motion.div>
      )}

      {/* Client Form Modal */}
      <Modal
        isOpen={showForm}
        onClose={() => setShowForm(false)}
        title={editClient ? 'Edit Client' : 'New Client'}
      >
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Name *</label>
            <input
              className="input-field"
              value={form.name}
              onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
              placeholder="Client name"
              autoFocus
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Company</label>
            <input
              className="input-field"
              value={form.company}
              onChange={e => setForm(f => ({ ...f, company: e.target.value }))}
              placeholder="Company name"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Email</label>
            <input
              className="input-field"
              type="email"
              value={form.email}
              onChange={e => setForm(f => ({ ...f, email: e.target.value }))}
              placeholder="email@example.com"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Address</label>
            <textarea
              className="input-field"
              rows={2}
              value={form.address}
              onChange={e => setForm(f => ({ ...f, address: e.target.value }))}
              placeholder="Street address, city, state"
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Hourly Rate</label>
              <input
                className="input-field"
                type="number"
                value={form.default_rate || ''}
                onChange={e => setForm(f => ({ ...f, default_rate: parseFloat(e.target.value) || 0 }))}
              />
              {editClient && form.default_rate !== editClient.default_rate && (
                <p className="text-xs text-accent/80 mt-1">
                  Projects still using {formatMoney(editClient.default_rate)}/hr will be updated. Already-invoiced time keeps its original rate.
                </p>
              )}
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Currency</label>
              <select
                className="input-field"
                value={form.currency}
                onChange={e => setForm(f => ({ ...f, currency: e.target.value }))}
              >
                <option value="USD">USD</option>
                <option value="EUR">EUR</option>
                <option value="GBP">GBP</option>
                <option value="CAD">CAD</option>
                <option value="AUD">AUD</option>
                <option value="JPY">JPY</option>
              </select>
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button onClick={() => setShowForm(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleSave} className="btn-primary">
              {editClient ? 'Update' : 'Create'} Client
            </button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog
        isOpen={deleteId !== null}
        onClose={() => setDeleteId(null)}
        onConfirm={handleDelete}
        title="Delete Client"
        message="This will permanently delete this client and all associated projects and time entries. This cannot be undone."
      />
    </motion.div>
  )
}
