import { useEffect, useState } from 'react'
import Modal from './Modal'
import { formatMoney } from '../utils/format'
import type { Client, Project } from '@shared/types'

export const PROJECT_COLORS = ['#F5A623', '#E5484D', '#3E8BF7', '#30A46C', '#8E4EC6', '#12A594', '#F76B15', '#D6409F']

export interface ProjectFormValues {
  client_id: number
  name: string
  description: string
  rate: number
  status: Project['status']
  color: string
}

/** Create or edit a project. */
export default function ProjectForm({ open, project, clients, defaultClientId, onClose, onSubmit }: {
  open: boolean
  project?: Project | null
  clients: Client[]
  defaultClientId?: number
  onClose: () => void
  onSubmit: (values: ProjectFormValues) => void
}) {
  const [form, setForm] = useState<ProjectFormValues>({
    client_id: 0, name: '', description: '', rate: 0, status: 'active', color: PROJECT_COLORS[0],
  })

  useEffect(() => {
    if (!open) return
    if (project) {
      setForm({
        client_id: project.client_id,
        name: project.name,
        description: project.description || '',
        rate: project.rate,
        status: project.status,
        color: project.color,
      })
    } else {
      const client = clients.find(c => c.id === defaultClientId) || clients[0]
      setForm({
        client_id: client?.id || 0,
        name: '',
        description: '',
        rate: client?.default_rate || 0,
        status: 'active',
        color: PROJECT_COLORS[Math.floor(Math.random() * PROJECT_COLORS.length)],
      })
    }
  }, [open, project, clients, defaultClientId])

  const valid = form.name.trim() && form.client_id
  const submit = () => { if (valid) onSubmit({ ...form, name: form.name.trim() }) }

  return (
    <Modal
      isOpen={open}
      onClose={onClose}
      title={project ? 'Edit project' : 'New project'}
      footer={
        <>
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button onClick={submit} disabled={!valid} className="btn-primary">{project ? 'Save changes' : 'Create project'}</button>
        </>
      }
    >
      <div className="space-y-4" onKeyDown={e => { if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') submit() }}>
        <div>
          <label className="label">Name</label>
          <input
            className="input"
            value={form.name}
            onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
            placeholder="Website redesign"
            autoFocus
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label">Client</label>
            <select
              className="input"
              value={form.client_id}
              onChange={e => {
                const cid = parseInt(e.target.value)
                const client = clients.find(c => c.id === cid)
                setForm(f => ({ ...f, client_id: cid, rate: project ? f.rate : (client?.default_rate ?? f.rate) }))
              }}
            >
              <option value={0}>Choose a client</option>
              {clients.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </div>
          <div>
            <label className="label">Hourly rate</label>
            <div className="relative">
              <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">$</span>
              <input
                className="input pl-6 num"
                type="number"
                min={0}
                step="0.01"
                value={form.rate || ''}
                onChange={e => setForm(f => ({ ...f, rate: parseFloat(e.target.value) || 0 }))}
              />
            </div>
          </div>
        </div>
        {!project && form.client_id > 0 && (
          <p className="hint -mt-2">
            Uses {clients.find(c => c.id === form.client_id)?.name}'s rate of {formatMoney(clients.find(c => c.id === form.client_id)?.default_rate || 0)}/hr. Change it for this project if needed.
          </p>
        )}
        <div>
          <label className="label">Description</label>
          <textarea
            className="input"
            rows={2}
            value={form.description}
            onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
            placeholder="Optional"
          />
        </div>
        <div className="grid grid-cols-2 gap-3">
          {project && (
            <div>
              <label className="label">Status</label>
              <select className="input" value={form.status} onChange={e => setForm(f => ({ ...f, status: e.target.value as Project['status'] }))}>
                <option value="active">Active</option>
                <option value="paused">Paused</option>
                <option value="complete">Complete</option>
                <option value="archived">Archived</option>
              </select>
            </div>
          )}
          <div>
            <label className="label">Color</label>
            <div className="flex items-center gap-1.5 h-8">
              {PROJECT_COLORS.map(c => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setForm(f => ({ ...f, color: c }))}
                  className={`w-5 h-5 rounded-full transition-transform ${form.color === c ? 'ring-2 ring-offset-2 ring-offset-panel ring-fg/40' : 'hover:scale-110'}`}
                  style={{ backgroundColor: c }}
                  aria-label={`Color ${c}`}
                />
              ))}
            </div>
          </div>
        </div>
      </div>
    </Modal>
  )
}
