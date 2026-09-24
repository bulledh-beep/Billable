import { useState, useEffect, useMemo } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { motion, AnimatePresence } from 'framer-motion'
import { FolderKanban, Play, Pause, X } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import EmptyState from '../components/EmptyState'
import StatusBadge from '../components/StatusBadge'
import Segmented from '../components/Segmented'
import SearchInput from '../components/SearchInput'
import ProjectForm, { type ProjectFormValues } from '../components/ProjectForm'
import { formatHoursShort, formatMoney, formatDay, relativeDays } from '../utils/format'
import type { Project, Client, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'
import { onBillingChanged } from '../utils/events'

interface ProjectsProps {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  onPauseTimer: () => Promise<any>
  onResumeTimer: () => Promise<any>
  isTimerRunning: boolean
  isTimerPaused: boolean
  activeEntry: TimeEntry | null
}

type StatusFilter = 'all' | Project['status']
const STATUS_ORDER: Record<string, number> = { active: 0, paused: 1, complete: 2, archived: 3 }

export default function Projects({
  onStartTimer, onPauseTimer, onResumeTimer, isTimerRunning, isTimerPaused, activeEntry,
}: ProjectsProps) {
  const navigate = useNavigate()
  const [searchParams, setSearchParams] = useSearchParams()
  const [projects, setProjects] = useState<Project[]>([])
  const [clients, setClients] = useState<Client[]>([])
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all')
  const [showForm, setShowForm] = useState(false)
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set())

  useEffect(() => { loadData() }, [isTimerRunning, isTimerPaused])
  useEffect(() => onBillingChanged(loadData), [])

  // ⌘⇧N from the menu
  useEffect(() => {
    if (searchParams.get('action') === 'new') {
      setShowForm(true)
      setSearchParams({}, { replace: true })
    }
  }, [searchParams, setSearchParams])

  const loadData = async () => {
    const [p, c] = await Promise.all([window.api.projects.list(), window.api.clients.list()])
    setProjects(p)
    setClients(c)
  }

  const handleCreate = async (values: ProjectFormValues) => {
    const created = await window.api.projects.create(values)
    toast.success('Project created')
    setShowForm(false)
    navigate(`/projects/${created.id}`)
  }

  const counts = useMemo(() => {
    const c: Record<string, number> = { all: projects.length, active: 0, paused: 0, complete: 0, archived: 0 }
    for (const p of projects) c[p.status] = (c[p.status] || 0) + 1
    return c
  }, [projects])

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase()
    return projects
      .filter(p => statusFilter === 'all' || p.status === statusFilter)
      .filter(p => !q || p.name.toLowerCase().includes(q) || (p.client_name || '').toLowerCase().includes(q))
      .sort((a, b) =>
        (STATUS_ORDER[a.status] - STATUS_ORDER[b.status]) ||
        String(b.last_activity || b.created_at).localeCompare(String(a.last_activity || a.created_at)))
  }, [projects, search, statusFilter])

  const toggleSelect = (id: number) => setSelectedIds(prev => {
    const next = new Set(prev)
    next.has(id) ? next.delete(id) : next.add(id)
    return next
  })

  const selected = projects.filter(p => selectedIds.has(p.id))
  const selectedClientIds = new Set(selected.map(p => p.client_id))
  const selectedUnbilled = selected.reduce((s, p) => s + (p.unbilled_amount || 0), 0)
  const canInvoiceSelection = selected.length > 0 && selectedClientIds.size === 1 && selectedUnbilled > 0

  const totals = useMemo(() => ({
    unbilled: filtered.reduce((s, p) => s + (p.unbilled_amount || 0), 0),
    hours: filtered.reduce((s, p) => s + (p.total_hours || 0), 0),
  }), [filtered])

  return (
    <div className="page">
      <PageHeader
        title="Projects"
        actions={<button onClick={() => setShowForm(true)} className="btn-secondary">New project</button>}
      />

      {projects.length === 0 ? (
        <div className="card">
          <EmptyState
            icon={FolderKanban}
            title="Create your first project"
            description={clients.length === 0 ? 'Add a client first, then create a project to track time against.' : 'Projects hold your time and set the hourly rate.'}
            action={clients.length === 0
              ? { label: 'Add a client', onClick: () => navigate('/clients?action=new') }
              : { label: 'New project', onClick: () => setShowForm(true) }}
          />
        </div>
      ) : (
        <>
          <div className="flex items-center gap-3 mb-4">
            <Segmented
              value={statusFilter}
              onChange={setStatusFilter}
              options={[
                { value: 'all', label: 'All', count: counts.all },
                { value: 'active', label: 'Active', count: counts.active },
                { value: 'paused', label: 'Paused', count: counts.paused },
                { value: 'complete', label: 'Complete', count: counts.complete },
                { value: 'archived', label: 'Archived', count: counts.archived },
              ]}
            />
            <SearchInput value={search} onChange={setSearch} placeholder="Search projects or clients" className="ml-auto w-72" />
          </div>

          <div className="card">
            <table className="tbl">
              <thead>
                <tr>
                  <th className="w-8"></th>
                  <th>Project</th>
                  <th>Status</th>
                  <th className="text-right">Tracked</th>
                  <th className="text-right">Unbilled</th>
                  <th className="text-right">Invoiced</th>
                  <th>Last tracked</th>
                  <th className="w-12"></th>
                </tr>
              </thead>
              <tbody>
                {filtered.map(project => {
                  const isSelected = selectedIds.has(project.id)
                  const isCurrent = (isTimerRunning || isTimerPaused) && activeEntry?.project_id === project.id
                  const unbilled = project.unbilled_amount || 0
                  return (
                    <tr
                      key={project.id}
                      onClick={() => navigate(`/projects/${project.id}`)}
                      className={`row-hover group ${isSelected ? 'bg-accent/[0.05]' : ''}`}
                    >
                      <td onClick={e => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={() => toggleSelect(project.id)}
                          className={isSelected ? '' : 'opacity-0 group-hover:opacity-100'}
                          aria-label={`Select ${project.name}`}
                        />
                      </td>
                      <td>
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: project.color }} />
                          <div className="min-w-0">
                            <div className="text-sm font-medium text-fg truncate max-w-[300px]">{project.name}</div>
                            <div className="text-xs text-fg-3 truncate">{project.client_name} · <span className="num">{formatMoney(project.rate)}</span>/hr</div>
                          </div>
                        </div>
                      </td>
                      <td><StatusBadge status={project.status} /></td>
                      <td className="text-right num text-fg-2">{formatHoursShort(project.total_hours || 0)}</td>
                      <td className="text-right">
                        {unbilled > 0 ? (
                          <div>
                            <div className="num font-medium text-amber">{formatMoney(unbilled)}</div>
                            <div className="text-2xs text-fg-3">since {formatDay(project.oldest_unbilled || '')}</div>
                          </div>
                        ) : <span className="text-fg-4">—</span>}
                      </td>
                      <td className="text-right num text-fg-2">
                        {(project.invoiced_amount || 0) > 0 ? formatMoney(project.invoiced_amount || 0) : <span className="text-fg-4">—</span>}
                      </td>
                      <td className="text-fg-3 whitespace-nowrap">
                        {project.last_activity ? relativeDays(project.last_activity) : '—'}
                      </td>
                      <td onClick={e => e.stopPropagation()}>
                        {isCurrent ? (
                          <button
                            onClick={() => (isTimerPaused ? onResumeTimer() : onPauseTimer())}
                            className="btn-icon-sm text-accent-text"
                            title={isTimerPaused ? 'Resume timer' : 'Pause timer'}
                          >
                            {isTimerPaused ? <Play className="fill-current" /> : <Pause className="fill-current" />}
                          </button>
                        ) : project.status === 'active' ? (
                          <button
                            onClick={() => onStartTimer(project.id)}
                            className="btn-icon-sm opacity-0 group-hover:opacity-100"
                            title="Start timer"
                            aria-label={`Start timer for ${project.name}`}
                          >
                            <Play />
                          </button>
                        ) : null}
                      </td>
                    </tr>
                  )
                })}
                {filtered.length === 0 && (
                  <tr><td colSpan={8} className="text-center text-fg-3 !h-20">No projects match</td></tr>
                )}
              </tbody>
              {filtered.length > 1 && (
                <tfoot>
                  <tr className="[&>td]:border-t [&>td]:border-line [&>td]:border-b-0 [&>td]:h-10">
                    <td></td>
                    <td className="text-xs text-fg-3">{filtered.length} projects</td>
                    <td></td>
                    <td className="text-right num text-xs text-fg-2">{formatHoursShort(totals.hours)}</td>
                    <td className="text-right num text-xs font-medium text-amber">{totals.unbilled > 0 ? formatMoney(totals.unbilled) : ''}</td>
                    <td colSpan={3}></td>
                  </tr>
                </tfoot>
              )}
            </table>
          </div>
        </>
      )}

      {/* Selection bar */}
      <AnimatePresence>
        {selected.length > 0 && (
          <motion.div
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 12 }}
            transition={{ duration: 0.15 }}
            className="fixed bottom-5 left-[calc(50%+110px)] -translate-x-1/2 z-40 flex items-center gap-4 pl-4 pr-2 h-12 rounded-[10px] bg-panel shadow-pop"
          >
            <span className="text-sm font-medium text-fg">{selected.length} selected</span>
            <span className="text-sm text-fg-3">
              <span className="num text-amber font-medium">{formatMoney(selectedUnbilled)}</span> unbilled
            </span>
            {selectedClientIds.size > 1 && <span className="text-xs text-fg-3">Pick projects from one client to invoice them together</span>}
            <button
              onClick={() => navigate(`/invoices/new?project_ids=${Array.from(selectedIds).join(',')}`)}
              disabled={!canInvoiceSelection}
              className="btn-primary btn-sm"
            >
              Create invoice
            </button>
            <button onClick={() => setSelectedIds(new Set())} className="btn-icon-sm" aria-label="Clear selection"><X /></button>
          </motion.div>
        )}
      </AnimatePresence>

      <ProjectForm
        open={showForm}
        clients={clients}
        onClose={() => setShowForm(false)}
        onSubmit={handleCreate}
      />
    </div>
  )
}

