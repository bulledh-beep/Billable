import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowLeft, Play, Square, Pencil, Trash2, FileText } from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import StatusBadge from '../components/StatusBadge'
import { formatHours, formatMoney, formatDate, formatTime, formatDuration } from '../utils/format'
import type { Project, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

interface Props {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  isTimerRunning: boolean
  activeEntry: TimeEntry | null
  elapsed: string
}

export default function ProjectDetail({ onStartTimer, onStopTimer, isTimerRunning, activeEntry, elapsed }: Props) {
  const { id } = useParams()
  const navigate = useNavigate()
  const [project, setProject] = useState<Project | null>(null)
  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [showEdit, setShowEdit] = useState(false)
  const [showDelete, setShowDelete] = useState(false)
  const [deleteEntryId, setDeleteEntryId] = useState<number | null>(null)
  const [editEntry, setEditEntry] = useState<TimeEntry | null>(null)
  const [editForm, setEditForm] = useState({ description: '', duration_minutes: 0, is_billable: true })
  const [form, setForm] = useState({ name: '', description: '', rate: 0, status: 'active' as string, color: '#F5A623' })

  useEffect(() => {
    if (id) loadData(parseInt(id))
  }, [id])

  // Reload entries when a timer starts/stops so the table stays in sync
  useEffect(() => {
    if (id) loadData(parseInt(id))
  }, [isTimerRunning])

  const loadData = async (projectId: number) => {
    const [p, e] = await Promise.all([
      window.api.projects.get(projectId),
      window.api.time.list(projectId),
    ])
    setProject(p)
    setEntries(e)
    if (p) setForm({ name: p.name, description: p.description, rate: p.rate, status: p.status, color: p.color })
  }

  const handleUpdate = async () => {
    if (!project) return
    await window.api.projects.update(project.id, form)
    toast.success('Project updated')
    setShowEdit(false)
    loadData(project.id)
  }

  const handleDelete = async () => {
    if (!project) return
    await window.api.projects.delete(project.id)
    toast.success('Project deleted')
    navigate('/projects')
  }

  const handleEditEntrySave = async () => {
    if (!editEntry || !project) return
    const newEnd = new Date(new Date(editEntry.start_time).getTime() + editForm.duration_minutes * 60000)
    await window.api.time.update(editEntry.id, {
      description: editForm.description,
      duration_minutes: editForm.duration_minutes,
      end_time: newEnd.toISOString(),
      is_billable: editForm.is_billable ? 1 : 0,
    })
    toast.success('Entry updated')
    setEditEntry(null)
    loadData(project.id)
  }

  const handleAddTime = async (entryId: number, minutesToAdd: number) => {
    const entry = entries.find(e => e.id === entryId)
    if (!entry || !project) return
    const newDuration = entry.duration_minutes + minutesToAdd
    const newEnd = new Date(new Date(entry.start_time).getTime() + newDuration * 60000)
    await window.api.time.update(entryId, {
      duration_minutes: newDuration,
      end_time: newEnd.toISOString(),
    })
    toast.success(`Added ${minutesToAdd >= 60 ? `${minutesToAdd / 60}h` : `${minutesToAdd}m`}`)
    loadData(project.id)
  }

  const handleDeleteEntry = async () => {
    if (!deleteEntryId || !project) return
    await window.api.time.delete(deleteEntryId)
    toast.success('Entry deleted')
    setDeleteEntryId(null)
    loadData(project.id)
  }

  if (!project) return null

  const unbilledAmount = (project.unbilled_hours || 0) * project.rate
  const totalEarned = (project.billed_total || 0) + unbilledAmount

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="p-8">
      <button
        onClick={() => navigate('/projects')}
        className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary transition-colors mb-6"
      >
        <ArrowLeft className="w-4 h-4" /> Projects
      </button>

      <div className="flex items-start justify-between mb-8">
        <div className="flex items-start gap-4">
          <div className="w-4 h-4 rounded-full mt-2" style={{ backgroundColor: project.color }} />
          <div>
            <h1 className="text-2xl font-bold text-text-primary">{project.name}</h1>
            <p className="text-sm text-text-secondary">{project.client_name} · {formatMoney(project.rate)}/hr</p>
            {project.description && <p className="text-sm text-text-tertiary mt-1">{project.description}</p>}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <StatusBadge status={project.status} />
          {isTimerRunning && activeEntry?.project_id === project.id ? (
            <button onClick={() => onStopTimer()} className="btn-danger flex items-center gap-2">
              <Square className="w-4 h-4 fill-current" /> Stop {elapsed}
            </button>
          ) : (
            <button onClick={() => onStartTimer(project.id)} className="btn-primary flex items-center gap-2">
              <Play className="w-4 h-4" /> Start Timer
            </button>
          )}
          <button onClick={() => setShowEdit(true)} className="btn-secondary p-2">
            <Pencil className="w-4 h-4" />
          </button>
          <button onClick={() => setShowDelete(true)} className="btn-danger p-2">
            <Trash2 className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-5 gap-4 mb-8">
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Total Hours</div>
          <div className="font-mono text-lg font-semibold">{formatHours(project.total_hours || 0)}h</div>
        </div>
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Total Earned</div>
          <div className="font-mono text-lg font-semibold text-accent">{formatMoney(totalEarned)}</div>
        </div>
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Billed</div>
          <div className="font-mono text-lg font-semibold text-status-paid">{formatMoney(project.billed_total || 0)}</div>
        </div>
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Unbilled Hours</div>
          <div className="font-mono text-lg font-semibold text-status-paused">{formatHours(project.unbilled_hours || 0)}h</div>
        </div>
        <div className="glass-panel p-4">
          <div className="text-xs text-text-tertiary mb-1">Unbilled Amount</div>
          <div className="font-mono text-lg font-semibold text-accent">{formatMoney(unbilledAmount)}</div>
        </div>
      </div>

      {/* Generate invoice button */}
      {(project.unbilled_hours || 0) > 0 && (
        <div className="mb-6">
          <button
            onClick={() => navigate(`/invoices/new?project_id=${project.id}`)}
            className="btn-primary flex items-center gap-2"
          >
            <FileText className="w-4 h-4" /> Generate Invoice ({formatMoney(unbilledAmount)})
          </button>
        </div>
      )}

      {/* Time Entries Table */}
      <h2 className="text-sm font-semibold text-text-primary mb-3">Time Entries</h2>
      <div className="glass-panel overflow-hidden">
        <table className="w-full">
          <thead>
            <tr className="border-b border-white/[0.04]">
              <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Date</th>
              <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Description</th>
              <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Time</th>
              <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">Duration</th>
              <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">Amount</th>
              <th className="w-48"></th>
            </tr>
          </thead>
          <tbody>
            {entries.filter((e: any) => e.end_time).map((entry: any) => (
              <tr key={entry.id} className="border-b border-white/[0.02] hover:bg-surface-200/30 transition-colors">
                <td className="px-4 py-3 text-sm text-text-secondary">{formatDate(entry.start_time)}</td>
                <td className="px-4 py-3 text-sm text-text-primary">{entry.description || '—'}</td>
                <td className="px-4 py-3 text-sm text-text-tertiary font-mono">
                  {formatTime(entry.start_time)} – {entry.end_time ? formatTime(entry.end_time) : '...'}
                </td>
                <td className="px-4 py-3 text-sm text-text-primary font-mono text-right">
                  {formatDuration(entry.duration_minutes)}
                </td>
                <td className="px-4 py-3 text-sm font-mono text-right text-text-primary">
                  {formatMoney((entry.duration_minutes / 60) * project.rate)}
                </td>
                <td className="px-4 py-3">
                  <div className="flex items-center gap-0.5 justify-end">
                    {[15, 30, 60].map(mins => (
                      <button
                        key={mins}
                        onClick={() => handleAddTime(entry.id, mins)}
                        className="px-1.5 py-0.5 text-[10px] font-mono font-medium text-text-tertiary hover:text-accent
                                   hover:bg-accent/10 rounded transition-colors"
                        title={`Add ${mins >= 60 ? `${mins / 60}h` : `${mins}m`}`}
                      >
                        +{mins >= 60 ? `${mins / 60}h` : `${mins}m`}
                      </button>
                    ))}
                    <div className="w-px h-4 bg-white/[0.04] mx-0.5" />
                    <button
                      onClick={() => {
                        setEditEntry(entry)
                        setEditForm({
                          description: entry.description || '',
                          duration_minutes: entry.duration_minutes,
                          is_billable: !!entry.is_billable,
                        })
                      }}
                      className="p-1.5 hover:bg-surface-300 rounded transition-colors"
                    >
                      <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
                    </button>
                    <button
                      onClick={() => setDeleteEntryId(entry.id)}
                      className="p-1.5 hover:bg-red-500/10 rounded transition-colors"
                    >
                      <Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {entries.filter((e: any) => e.end_time).length === 0 && (
          <p className="text-sm text-text-tertiary text-center py-8">No time entries yet</p>
        )}
      </div>

      {/* Edit Modal */}
      <Modal isOpen={showEdit} onClose={() => setShowEdit(false)} title="Edit Project">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Name</label>
            <input className="input-field" value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <textarea className="input-field" rows={2} value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))} />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Rate</label>
              <input className="input-field" type="number" value={form.rate || ''} onChange={e => setForm(f => ({ ...f, rate: parseFloat(e.target.value) || 0 }))} />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Status</label>
              <select className="input-field" value={form.status} onChange={e => setForm(f => ({ ...f, status: e.target.value }))}>
                <option value="active">Active</option>
                <option value="paused">Paused</option>
                <option value="complete">Complete</option>
                <option value="archived">Archived</option>
              </select>
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button onClick={() => setShowEdit(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleUpdate} className="btn-primary">Save Changes</button>
          </div>
        </div>
      </Modal>

      {/* Edit Time Entry Modal */}
      <Modal isOpen={!!editEntry} onClose={() => setEditEntry(null)} title="Edit Time Entry" size="sm">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <input
              className="input-field"
              value={editForm.description}
              onChange={e => setEditForm(f => ({ ...f, description: e.target.value }))}
              placeholder="What did you work on?"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Duration (minutes)</label>
            <input
              className="input-field"
              type="number"
              value={editForm.duration_minutes ? Math.round(editForm.duration_minutes) : ''}
              onChange={e => setEditForm(f => ({ ...f, duration_minutes: parseInt(e.target.value) || 0 }))}
            />
            <div className="flex gap-1.5 mt-2">
              <span className="text-xs text-text-tertiary self-center mr-1">Quick add:</span>
              {[
                { label: '+15m', mins: 15 },
                { label: '+30m', mins: 30 },
                { label: '+1h', mins: 60 },
                { label: '+2h', mins: 120 },
              ].map(({ label, mins }) => (
                <button
                  key={mins}
                  type="button"
                  onClick={() => setEditForm(f => ({ ...f, duration_minutes: f.duration_minutes + mins }))}
                  className="px-2.5 py-1 text-xs font-mono font-medium text-accent bg-accent/10
                             hover:bg-accent/20 rounded-md transition-colors"
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={editForm.is_billable}
              onChange={e => setEditForm(f => ({ ...f, is_billable: e.target.checked }))}
              className="rounded border-white/20 bg-surface-300 text-accent focus:ring-accent"
            />
            <span className="text-sm text-text-secondary">Billable</span>
          </label>
          <div className="flex justify-end gap-3">
            <button onClick={() => setEditEntry(null)} className="btn-secondary">Cancel</button>
            <button onClick={handleEditEntrySave} className="btn-primary">Save</button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog
        isOpen={showDelete}
        onClose={() => setShowDelete(false)}
        onConfirm={handleDelete}
        title="Delete Project"
        message="This will permanently delete this project and all its time entries."
      />

      <ConfirmDialog
        isOpen={deleteEntryId !== null}
        onClose={() => setDeleteEntryId(null)}
        onConfirm={handleDeleteEntry}
        title="Delete Time Entry"
        message="Are you sure you want to delete this time entry?"
      />
    </motion.div>
  )
}
