import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import { Plus, FolderKanban, Search, Play, Square as StopIcon, ChevronRight, FileText, CheckSquare, Square, XCircle } from 'lucide-react'
import Modal from '../components/Modal'
import EmptyState from '../components/EmptyState'
import StatusBadge from '../components/StatusBadge'
import { formatHours, formatMoney } from '../utils/format'
import type { Project, Client, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

const PROJECT_COLORS = ['#F5A623', '#E74C3C', '#3498DB', '#2ECC71', '#9B59B6', '#1ABC9C', '#E67E22', '#EC407A']

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

interface ProjectsProps {
  onStartTimer: (projectId: number, description?: string, isBillable?: number) => Promise<any>
  onStopTimer: () => Promise<any>
  isTimerRunning: boolean
  activeEntry: TimeEntry | null
}

export default function Projects({ onStartTimer, onStopTimer, isTimerRunning, activeEntry }: ProjectsProps) {
  const navigate = useNavigate()
  const [projects, setProjects] = useState<Project[]>([])
  const [clients, setClients] = useState<Client[]>([])
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<string>('all')
  const [showForm, setShowForm] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set())
  const [form, setForm] = useState({
    client_id: 0, name: '', description: '', rate: 100, status: 'active' as const, color: PROJECT_COLORS[0],
  })

  useEffect(() => { loadData() }, [])

  // Refresh when timer starts/stops so hours/amounts stay current
  useEffect(() => { loadData() }, [isTimerRunning])

  const loadData = async () => {
    const [p, c] = await Promise.all([
      window.api.projects.list(),
      window.api.clients.list(),
    ])
    setProjects(p)
    setClients(c)
  }

  const openNew = () => {
    const firstClient = clients[0]
    setForm({
      client_id: firstClient?.id || 0,
      name: '', description: '',
      rate: firstClient?.default_rate || 100,
      status: 'active',
      color: PROJECT_COLORS[Math.floor(Math.random() * PROJECT_COLORS.length)],
    })
    setShowForm(true)
  }

  const handleSave = async () => {
    if (!form.name.trim()) return toast.error('Name is required')
    if (!form.client_id) return toast.error('Select a client')
    await window.api.projects.create(form)
    toast.success('Project created')
    setShowForm(false)
    loadData()
  }

  const filtered = projects.filter(p => {
    const matchSearch = p.name.toLowerCase().includes(search.toLowerCase()) ||
      (p.client_name || '').toLowerCase().includes(search.toLowerCase())
    const matchStatus = statusFilter === 'all' || p.status === statusFilter
    return matchSearch && matchStatus
  })

  const toggleSelect = (id: number, e: React.MouseEvent) => {
    e.stopPropagation()
    setSelectedIds(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const selectedProjects = projects.filter(p => selectedIds.has(p.id))
  const combinedUnbilledHours = selectedProjects.reduce((sum, p) => sum + (p.unbilled_hours || 0), 0)
  const combinedUnbilledAmount = selectedProjects.reduce((sum, p) => sum + (p.unbilled_hours || 0) * p.rate, 0)
  const combinedTotalHours = selectedProjects.reduce((sum, p) => sum + (p.total_hours || 0), 0)

  const handleGenerateCombinedInvoice = () => {
    const ids = Array.from(selectedIds).join(',')
    navigate(`/invoices/new?project_ids=${ids}`)
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Projects</h1>
          <p className="text-sm text-text-secondary mt-1">{projects.length} total projects</p>
        </div>
        <button onClick={openNew} className="btn-primary flex items-center gap-2">
          <Plus className="w-4 h-4" /> New Project
        </button>
      </motion.div>

      {projects.length > 0 && (
        <motion.div variants={item} className="flex gap-3 mb-6">
          <div className="relative flex-1">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
            <input
              type="text"
              placeholder="Search projects..."
              value={search}
              onChange={e => setSearch(e.target.value)}
              className="input-field pl-10"
            />
          </div>
          <div className="flex gap-1 bg-surface-100 rounded-lg p-0.5 border border-rim/[0.04]">
            {['all', 'active', 'paused', 'complete', 'archived'].map(s => (
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

      {filtered.length === 0 && projects.length === 0 ? (
        <EmptyState
          icon={FolderKanban}
          title="No projects yet"
          description="Create your first project to start tracking time."
          action={{ label: 'New Project', onClick: openNew }}
        />
      ) : (
        <motion.div variants={item} className="space-y-2">
          {filtered.map(project => {
            const isSelected = selectedIds.has(project.id)
            return (
              <div
                key={project.id}
                onClick={() => navigate(`/projects/${project.id}`)}
                className={`glass-panel-hover p-4 flex items-center gap-4 cursor-pointer group ${
                  isSelected ? 'ring-1 ring-accent/40 bg-accent/[0.03]' : ''
                }`}
              >
                <button
                  onClick={(e) => toggleSelect(project.id, e)}
                  className="flex-shrink-0 p-0.5 rounded hover:bg-surface-300 transition-colors"
                  title={isSelected ? 'Deselect' : 'Select for invoice'}
                >
                  {isSelected
                    ? <CheckSquare className="w-4 h-4 text-accent" />
                    : <Square className="w-4 h-4 text-text-tertiary opacity-0 group-hover:opacity-100 transition-opacity" />
                  }
                </button>
                <div className="w-3 h-3 rounded-full flex-shrink-0" style={{ backgroundColor: project.color }} />
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-text-primary">{project.name}</div>
                  <div className="text-xs text-text-tertiary">{project.client_name}</div>
                </div>
                <StatusBadge status={project.status} />
                <div className="text-right min-w-[100px]">
                  <div className="font-mono text-sm text-accent">{formatMoney((project.billed_total || 0) + (project.unbilled_hours || 0) * project.rate)}</div>
                  <div className="text-xs text-text-tertiary">{formatHours(project.total_hours || 0)}h · {formatHours(project.unbilled_hours || 0)}h unbilled</div>
                </div>
                {isTimerRunning && activeEntry?.project_id === project.id ? (
                  <button
                    onClick={(e) => { e.stopPropagation(); onStopTimer() }}
                    className="p-2 hover:bg-red-500/10 rounded-lg transition-all"
                    title="Stop timer"
                  >
                    <StopIcon className="w-4 h-4 text-red-400 fill-current" />
                  </button>
                ) : (
                  <button
                    onClick={(e) => { e.stopPropagation(); onStartTimer(project.id) }}
                    className="p-2 opacity-0 group-hover:opacity-100 hover:bg-accent/10 rounded-lg transition-all"
                    title="Start timer"
                  >
                    <Play className="w-4 h-4 text-accent" />
                  </button>
                )}
                <ChevronRight className="w-4 h-4 text-text-tertiary" />
              </div>
            )
          })}
        </motion.div>
      )}

      {/* Selection action bar */}
      {selectedIds.size > 0 && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 20 }}
          className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 glass-panel border border-accent/20 shadow-2xl px-6 py-3 flex items-center gap-6 rounded-xl"
        >
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-accent">{selectedIds.size} project{selectedIds.size > 1 ? 's' : ''}</span>
            <button
              onClick={() => setSelectedIds(new Set())}
              className="p-0.5 hover:bg-surface-300 rounded transition-colors"
              title="Clear selection"
            >
              <XCircle className="w-4 h-4 text-text-tertiary hover:text-text-primary" />
            </button>
          </div>
          <div className="w-px h-6 bg-rim/[0.06]" />
          <div className="text-sm text-text-secondary">
            <span className="font-mono text-text-primary">{formatHours(combinedTotalHours)}h</span> total
          </div>
          <div className="text-sm text-text-secondary">
            <span className="font-mono text-text-primary">{formatHours(combinedUnbilledHours)}h</span> unbilled
          </div>
          <div className="text-sm text-text-secondary">
            <span className="font-mono text-accent">{formatMoney(combinedUnbilledAmount)}</span>
          </div>
          {combinedUnbilledHours > 0 && (
            <>
              <div className="w-px h-6 bg-rim/[0.06]" />
              <button
                onClick={handleGenerateCombinedInvoice}
                className="btn-primary flex items-center gap-2 text-sm py-2"
              >
                <FileText className="w-4 h-4" /> Generate Invoice
              </button>
            </>
          )}
        </motion.div>
      )}

      <Modal isOpen={showForm} onClose={() => setShowForm(false)} title="New Project">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Client *</label>
            <select
              className="input-field"
              value={form.client_id}
              onChange={e => {
                const cid = parseInt(e.target.value)
                const client = clients.find(c => c.id === cid)
                setForm(f => ({ ...f, client_id: cid, rate: client?.default_rate ?? f.rate }))
              }}
            >
              <option value={0}>Select client...</option>
              {clients.map(c => <option key={c.id} value={c.id}>{c.name} — {formatMoney(c.default_rate)}/hr</option>)}
            </select>
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Project Name *</label>
            <input
              className="input-field"
              value={form.name}
              onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
              placeholder="Project name"
              autoFocus
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <textarea
              className="input-field"
              rows={2}
              value={form.description}
              onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
              placeholder="Brief description"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Hourly Rate</label>
            <input
              className="input-field"
              type="number"
              value={form.rate || ''}
              onChange={e => setForm(f => ({ ...f, rate: parseFloat(e.target.value) || 0 }))}
            />
            {form.client_id > 0 && (
              <p className="text-xs text-text-tertiary mt-1">
                Auto-filled from client rate — edit to override
              </p>
            )}
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Color</label>
            <div className="flex gap-2">
              {PROJECT_COLORS.map(c => (
                <button
                  key={c}
                  onClick={() => setForm(f => ({ ...f, color: c }))}
                  className={`w-8 h-8 rounded-lg transition-all ${form.color === c ? 'ring-2 ring-white/40 scale-110' : 'hover:scale-105'}`}
                  style={{ backgroundColor: c }}
                />
              ))}
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button onClick={() => setShowForm(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleSave} className="btn-primary">Create Project</button>
          </div>
        </div>
      </Modal>
    </motion.div>
  )
}
