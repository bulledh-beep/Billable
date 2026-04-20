import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import {
  Play, Square, Plus, Clock, Trash2, Pencil, Search, TimerReset,
} from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import { formatDate, formatTime, formatDuration, formatMoney, todayISO } from '../utils/format'
import type { TimeEntry, Project } from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

interface Props {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  isTimerRunning: boolean
  activeEntry: TimeEntry | null
  elapsed: string
  checkActive: () => void
}

export default function TimeTracking({ onStartTimer, onStopTimer, isTimerRunning, activeEntry, elapsed, checkActive }: Props) {
  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [search, setSearch] = useState('')
  const [showManual, setShowManual] = useState(false)
  const [showStartForm, setShowStartForm] = useState(false)
  const [editEntry, setEditEntry] = useState<TimeEntry | null>(null)
  const [deleteId, setDeleteId] = useState<number | null>(null)

  // Start timer form
  const [startProjectId, setStartProjectId] = useState(0)
  const [startDescription, setStartDescription] = useState('')

  // Manual entry form
  const [manualForm, setManualForm] = useState({
    project_id: 0,
    description: '',
    date: todayISO(),
    start_time: '09:00',
    end_time: '10:00',
    is_billable: true,
  })

  // Edit form
  const [editForm, setEditForm] = useState({
    description: '',
    duration_minutes: 0,
    is_billable: true,
  })

  useEffect(() => { loadData() }, [])

  // Reload when the timer transitions (starts or stops, including from tray/sidebar)
  useEffect(() => { loadData() }, [isTimerRunning])

  const loadData = async () => {
    const [e, p] = await Promise.all([
      window.api.time.list(),
      window.api.projects.list(),
    ])
    setEntries(e)
    setProjects(p.filter((p: any) => p.status === 'active'))
    if (p.length > 0 && !startProjectId) setStartProjectId(p[0].id)
    if (p.length > 0 && !manualForm.project_id) setManualForm(f => ({ ...f, project_id: p[0].id }))
  }

  const handleStart = async () => {
    if (!startProjectId) return toast.error('Select a project')
    await onStartTimer(startProjectId, startDescription)
    setShowStartForm(false)
    setStartDescription('')
    toast.success('Timer started')
    loadData()
  }

  const handleStop = async () => {
    await onStopTimer()
    toast.success('Timer stopped')
    loadData()
    checkActive()
  }

  const handleManualEntry = async () => {
    if (!manualForm.project_id) return toast.error('Select a project')
    const startDT = `${manualForm.date}T${manualForm.start_time}:00`
    const endDT = `${manualForm.date}T${manualForm.end_time}:00`
    const startMs = new Date(startDT).getTime()
    const endMs = new Date(endDT).getTime()
    const durationMinutes = Math.max(0, (endMs - startMs) / 60000)

    await window.api.time.create({
      project_id: manualForm.project_id,
      description: manualForm.description,
      start_time: new Date(startDT).toISOString(),
      end_time: new Date(endDT).toISOString(),
      duration_minutes: durationMinutes,
      is_billable: manualForm.is_billable ? 1 : 0,
      is_invoiced: 0,
    })
    toast.success('Time entry added')
    setShowManual(false)
    loadData()
  }

  const handleEditSave = async () => {
    if (!editEntry) return
    await window.api.time.update(editEntry.id, {
      description: editForm.description,
      duration_minutes: editForm.duration_minutes,
      is_billable: editForm.is_billable ? 1 : 0,
    })
    toast.success('Entry updated')
    setEditEntry(null)
    loadData()
  }

  const handleAddTime = async (entryId: number, minutesToAdd: number) => {
    const entry = entries.find(e => e.id === entryId)
    if (!entry) return
    const newDuration = entry.duration_minutes + minutesToAdd
    // Also adjust end_time to match
    const newEnd = new Date(new Date(entry.start_time).getTime() + newDuration * 60000)
    await window.api.time.update(entryId, {
      duration_minutes: newDuration,
      end_time: newEnd.toISOString(),
    })
    toast.success(`Added ${minutesToAdd >= 60 ? `${minutesToAdd / 60}h` : `${minutesToAdd}m`}`)
    loadData()
  }

  const handleDelete = async () => {
    if (deleteId) {
      await window.api.time.delete(deleteId)
      toast.success('Entry deleted')
      setDeleteId(null)
      loadData()
    }
  }

  const completedEntries = entries.filter((e: any) => e.end_time)
  const filtered = completedEntries.filter((e: any) =>
    (e.description || '').toLowerCase().includes(search.toLowerCase()) ||
    (e.project_name || '').toLowerCase().includes(search.toLowerCase())
  )

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Time Tracking</h1>
          <p className="text-sm text-text-secondary mt-1">{completedEntries.length} entries</p>
        </div>
        <div className="flex gap-2">
          <button onClick={() => setShowManual(true)} className="btn-secondary flex items-center gap-2">
            <Plus className="w-4 h-4" /> Manual Entry
          </button>
          {isTimerRunning ? (
            <button onClick={handleStop} className="btn-danger flex items-center gap-2">
              <Square className="w-4 h-4 fill-current" /> Stop {elapsed}
            </button>
          ) : (
            <button onClick={() => setShowStartForm(true)} className="btn-primary flex items-center gap-2">
              <Play className="w-4 h-4" /> Start Timer
            </button>
          )}
        </div>
      </motion.div>

      {/* Active Timer Display */}
      {isTimerRunning && activeEntry && (
        <motion.div
          variants={item}
          className="glass-panel p-6 mb-6 border border-accent/20"
        >
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <motion.div
                animate={{ scale: [1, 1.2, 1] }}
                transition={{ duration: 2, repeat: Infinity }}
                className="w-3 h-3 rounded-full bg-accent"
              />
              <div>
                <div className="font-mono text-3xl font-bold text-accent tracking-wider">{elapsed}</div>
                <div className="text-sm text-text-secondary mt-1">
                  {activeEntry.project_name} · {activeEntry.description || 'No description'}
                </div>
              </div>
            </div>
            <button onClick={handleStop} className="btn-danger flex items-center gap-2">
              <Square className="w-4 h-4 fill-current" /> Stop Timer
            </button>
          </div>
        </motion.div>
      )}

      {completedEntries.length > 0 && (
        <motion.div variants={item} className="mb-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
            <input
              type="text"
              placeholder="Search entries..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="input-field pl-10"
            />
          </div>
        </motion.div>
      )}

      {filtered.length === 0 && completedEntries.length === 0 ? (
        <EmptyState
          icon={Clock}
          title="No time entries"
          description="Start a timer or add a manual entry to begin tracking your time."
        />
      ) : (
        <motion.div variants={item} className="glass-panel overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="border-b border-white/[0.04]">
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Date</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Project</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Description</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">Duration</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">Amount</th>
                <th className="w-48"></th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((entry: any) => (
                <tr key={entry.id} className="border-b border-white/[0.02] hover:bg-surface-200/30 transition-colors">
                  <td className="px-4 py-3 text-sm text-text-secondary">{formatDate(entry.start_time)}</td>
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 rounded-full" style={{ backgroundColor: entry.project_color }} />
                      <span className="text-sm text-text-primary">{entry.project_name}</span>
                    </div>
                  </td>
                  <td className="px-4 py-3 text-sm text-text-secondary">{entry.description || '—'}</td>
                  <td className="px-4 py-3 font-mono text-sm text-text-primary text-right">
                    {formatDuration(entry.duration_minutes)}
                  </td>
                  <td className="px-4 py-3 font-mono text-sm text-text-primary text-right">
                    {entry.is_billable ? formatMoney((entry.duration_minutes / 60) * (entry.rate || 0)) : '—'}
                  </td>
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-0.5 justify-end">
                      {/* Quick-add duration presets */}
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
                            description: entry.description,
                            duration_minutes: entry.duration_minutes,
                            is_billable: !!entry.is_billable,
                          })
                        }}
                        className="p-1.5 hover:bg-surface-300 rounded transition-colors"
                      >
                        <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
                      </button>
                      <button
                        onClick={() => setDeleteId(entry.id)}
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
        </motion.div>
      )}

      {/* Start Timer Modal */}
      <Modal isOpen={showStartForm} onClose={() => setShowStartForm(false)} title="Start Timer" size="sm">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Project</label>
            <select
              className="input-field"
              value={startProjectId}
              onChange={e => setStartProjectId(parseInt(e.target.value))}
            >
              {projects.map(p => <option key={p.id} value={p.id}>{p.name} ({(p as any).client_name})</option>)}
            </select>
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <input
              className="input-field"
              value={startDescription}
              onChange={e => setStartDescription(e.target.value)}
              placeholder="What are you working on?"
              autoFocus
            />
          </div>
          <div className="flex justify-end gap-3">
            <button onClick={() => setShowStartForm(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleStart} className="btn-primary flex items-center gap-2">
              <Play className="w-4 h-4" /> Start
            </button>
          </div>
        </div>
      </Modal>

      {/* Manual Entry Modal */}
      <Modal isOpen={showManual} onClose={() => setShowManual(false)} title="Manual Time Entry">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Project</label>
            <select
              className="input-field"
              value={manualForm.project_id}
              onChange={e => setManualForm(f => ({ ...f, project_id: parseInt(e.target.value) }))}
            >
              {projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
            </select>
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <input
              className="input-field"
              value={manualForm.description}
              onChange={e => setManualForm(f => ({ ...f, description: e.target.value }))}
              placeholder="What did you work on?"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Date</label>
            <input
              className="input-field"
              type="date"
              value={manualForm.date}
              onChange={e => setManualForm(f => ({ ...f, date: e.target.value }))}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Start Time</label>
              <input
                className="input-field"
                type="time"
                value={manualForm.start_time}
                onChange={e => setManualForm(f => ({ ...f, start_time: e.target.value }))}
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">End Time</label>
              <input
                className="input-field"
                type="time"
                value={manualForm.end_time}
                onChange={e => setManualForm(f => ({ ...f, end_time: e.target.value }))}
              />
            </div>
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={manualForm.is_billable}
              onChange={e => setManualForm(f => ({ ...f, is_billable: e.target.checked }))}
              className="rounded border-white/20 bg-surface-300 text-accent focus:ring-accent"
            />
            <span className="text-sm text-text-secondary">Billable</span>
          </label>
          <div className="flex justify-end gap-3">
            <button onClick={() => setShowManual(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleManualEntry} className="btn-primary">Add Entry</button>
          </div>
        </div>
      </Modal>

      {/* Edit Entry Modal */}
      <Modal isOpen={!!editEntry} onClose={() => setEditEntry(null)} title="Edit Time Entry" size="sm">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <input
              className="input-field"
              value={editForm.description}
              onChange={e => setEditForm(f => ({ ...f, description: e.target.value }))}
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
            <button onClick={handleEditSave} className="btn-primary">Save</button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog
        isOpen={deleteId !== null}
        onClose={() => setDeleteId(null)}
        onConfirm={handleDelete}
        title="Delete Time Entry"
        message="Are you sure you want to delete this time entry?"
      />
    </motion.div>
  )
}
