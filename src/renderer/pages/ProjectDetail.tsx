import { useState, useEffect, useMemo } from 'react'
import { useParams, useNavigate, Link } from 'react-router-dom'
import { Play, Trash2, CheckCircle2, Archive, Clock, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import ConfirmDialog from '../components/ConfirmDialog'
import StatusBadge from '../components/StatusBadge'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import Menu from '../components/Menu'
import Segmented from '../components/Segmented'
import EmptyState from '../components/EmptyState'
import EntryEditor from '../components/EntryEditor'
import ProjectForm, { type ProjectFormValues } from '../components/ProjectForm'
import CloseOutModal, { type CloseOutChoice } from '../components/CloseOutModal'
import { EntryRow } from './TimeTracking'
import { formatMoney, formatDay, formatHoursShort, relativeDays, formatDurationShort } from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import type { Client, Invoice, Project, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

interface Props {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  onPauseTimer: () => Promise<any>
  onResumeTimer: () => Promise<any>
  isTimerRunning: boolean
  isTimerPaused: boolean
  activeEntry: TimeEntry | null
}

type EntryFilter = 'all' | 'unbilled' | 'billed'

export default function ProjectDetail({ onStartTimer, isTimerRunning, isTimerPaused, activeEntry }: Props) {
  const { id } = useParams()
  const navigate = useNavigate()
  const [project, setProject] = useState<Project | null>(null)
  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [invoices, setInvoices] = useState<Invoice[]>([])
  const [clients, setClients] = useState<Client[]>([])
  const [filter, setFilter] = useState<EntryFilter>('all')
  const [showForm, setShowForm] = useState(false)
  const [showDelete, setShowDelete] = useState(false)
  const [closing, setClosing] = useState<{ values: ProjectFormValues } | null>(null)
  const [editorMode, setEditorMode] = useState<'add' | 'edit' | null>(null)
  const [editing, setEditing] = useState<TimeEntry | null>(null)
  const [deleting, setDeleting] = useState<TimeEntry | null>(null)

  const projectId = id ? parseInt(id) : 0

  useEffect(() => { if (projectId) loadData() }, [projectId, isTimerRunning, isTimerPaused])

  const loadData = async () => {
    const [p, e, inv, c] = await Promise.all([
      window.api.projects.get(projectId),
      window.api.time.list(projectId),
      window.api.invoices.list(),
      window.api.clients.list(),
    ])
    setProject(p)
    setEntries(e)
    setInvoices(inv)
    setClients(c)
  }

  const reload = () => { loadData(); notifyBillingChanged() }

  const done = useMemo(() => entries.filter(e => e.end_time), [entries])
  const visible = useMemo(() => done.filter(e =>
    filter === 'all' || (filter === 'unbilled' ? e.billing_state === 'unbilled' : !!e.invoice_id || e.billing_state === 'invoiced'),
  ), [done, filter])

  // Invoices that carry this project's time
  const projectInvoices = useMemo(() => {
    const ids = new Set(done.map(e => e.invoice_id).filter(Boolean) as number[])
    return invoices.filter(i => ids.has(i.id) || i.project_id === projectId)
  }, [done, invoices, projectId])

  if (!project) return null

  const isTimingThis = (isTimerRunning || isTimerPaused) && activeEntry?.project_id === project.id
  const unbilledIds = done.filter(e => e.billing_state === 'unbilled').map(e => e.id)

  // ---- Project changes (with the close-out check) ----
  const applyProject = async (values: ProjectFormValues) => {
    await window.api.projects.update(project.id, values)
    toast.success('Project updated')
    reload()
  }

  const handleSubmit = async (values: ProjectFormValues) => {
    setShowForm(false)
    const closingNow = (values.status === 'complete' || values.status === 'archived') && values.status !== project.status
    if (closingNow && unbilledIds.length > 0) {
      setClosing({ values })
      return
    }
    await applyProject(values)
  }

  const changeStatus = (status: Project['status']) => handleSubmit({
    client_id: project.client_id, name: project.name, description: project.description,
    rate: project.rate, status, color: project.color,
  })

  const handleCloseOut = async (choice: CloseOutChoice) => {
    if (!closing) return
    const { values } = closing
    setClosing(null)
    await window.api.projects.update(project.id, values)
    if (choice === 'dont-bill') {
      await window.api.time.setBillable(unbilledIds, false)
      toast.success(`Project marked ${values.status}. Its leftover time won't be billed.`)
    } else if (choice === 'invoice') {
      notifyBillingChanged()
      navigate(`/invoices/new?project_id=${project.id}`)
      return
    } else {
      toast.success(`Project marked ${values.status}. Its unbilled time stays on your Billing page.`)
    }
    reload()
  }

  const handleDeleteProject = async () => {
    await window.api.projects.delete(project.id)
    toast.success('Project deleted')
    notifyBillingChanged()
    navigate('/projects')
  }

  const handleAddTime = async (entry: TimeEntry, minutes: number) => {
    const newDuration = entry.duration_minutes + minutes
    const newEnd = new Date(new Date(entry.start_time).getTime() + newDuration * 60_000)
    await window.api.time.update(entry.id, { duration_minutes: newDuration, end_time: newEnd.toISOString() })
    toast.success(`Added ${formatDurationShort(minutes)}`)
    reload()
  }

  const handleDeleteEntry = async () => {
    if (!deleting) return
    await window.api.time.delete(deleting.id)
    toast.success('Entry deleted')
    setDeleting(null)
    reload()
  }

  const counts = {
    all: done.length,
    unbilled: unbilledIds.length,
    billed: done.filter(e => !!e.invoice_id || e.billing_state === 'invoiced').length,
  }
  const unbilledAmount = project.unbilled_amount || 0

  return (
    <div className="page">
      <PageHeader
        crumbs={[{ label: 'Projects', to: '/projects' }]}
        title={project.name}
        subtitle={project.client_name}
        meta={<StatusBadge status={project.status} />}
        actions={
          <>
            {unbilledAmount > 0 && (
              <button onClick={() => navigate(`/invoices/new?project_id=${project.id}`)} className="btn-secondary" title="Bill this project's unbilled time">
                Create invoice
              </button>
            )}
            <button onClick={() => setShowForm(true)} className="btn-secondary">Edit</button>
            <Menu
              items={[
                project.status !== 'complete' && { label: 'Mark complete', icon: CheckCircle2, onClick: () => changeStatus('complete') },
                project.status !== 'active' && { label: 'Mark active', icon: Play, onClick: () => changeStatus('active') },
                project.status !== 'archived' && { label: 'Archive', icon: Archive, onClick: () => changeStatus('archived') },
                'separator',
                { label: 'Delete project', icon: Trash2, danger: true, onClick: () => setShowDelete(true) },
              ]}
            />
          </>
        }
      />

      {/* Identity */}
      <div className="flex items-start gap-3 mb-5">
        <span className="w-3 h-3 rounded-full mt-1.5 shrink-0" style={{ backgroundColor: project.color }} />
        <div className="min-w-0">
          <div className="text-sm text-fg-2">
            <Link to={`/clients/${project.client_id}`} className="font-medium text-fg hover:underline underline-offset-2">{project.client_name}</Link>
            <span className="text-fg-4"> · </span>
            <span className="num">{formatMoney(project.rate)}</span>/hr
            {project.last_activity && <><span className="text-fg-4"> · </span>last tracked {relativeDays(project.last_activity)}</>}
          </div>
          {project.description && <p className="text-sm text-fg-3 mt-1 max-w-2xl">{project.description}</p>}
        </div>
      </div>

      <MetricStrip className="mb-5">
        <Metric label="Tracked" value={formatHoursShort(project.total_hours || 0)} sub={`${done.length} ${done.length === 1 ? 'entry' : 'entries'}`} />
        <Metric
          label="Unbilled"
          value={<Money amount={unbilledAmount} />}
          sub={unbilledAmount > 0 ? `${formatHoursShort(project.unbilled_hours || 0)} since ${formatDay(project.oldest_unbilled || '')}` : 'All time is billed'}
        />
        <Metric label="Invoiced" value={<Money amount={project.invoiced_amount || 0} />} sub="Before tax" />
        <Metric label="Paid" value={<Money amount={project.paid_amount || 0} />} sub={`${projectInvoices.filter(i => i.status === 'paid').length} paid invoice${projectInvoices.filter(i => i.status === 'paid').length === 1 ? '' : 's'}`} />
      </MetricStrip>

      <div className="grid grid-cols-[minmax(0,1fr)_280px] gap-6 items-start">
        {/* Time entries */}
        <section className="min-w-0">
          <div className="group-head">
            <h2 className="section-title">Time entries</h2>
            <div className="flex items-center gap-2">
              <Segmented
                value={filter}
                onChange={setFilter}
                options={[
                  { value: 'all', label: 'All', count: counts.all },
                  { value: 'unbilled', label: 'Unbilled', count: counts.unbilled },
                  { value: 'billed', label: 'Invoiced', count: counts.billed },
                ]}
              />
              <button onClick={() => { setEditing(null); setEditorMode('add') }} className="btn-secondary btn-sm">Add time</button>
              {!isTimingThis && project.status === 'active' && (
                <button onClick={() => onStartTimer(project.id)} className="btn-secondary btn-sm"><Play className="!w-2.5 !h-2.5 text-accent fill-current" /> Start timer</button>
              )}
            </div>
          </div>
          <div className="card overflow-hidden">
          {visible.length === 0 ? (
            <EmptyState
              compact
              icon={Clock}
              title={done.length === 0 ? 'No time on this project yet' : 'Nothing here'}
              description={done.length === 0 ? 'Start a timer or add time by hand.' : 'Try another filter.'}
            />
          ) : (
            visible.map(entry => (
              <EntryRow
                key={entry.id}
                entry={entry}
                showProject={false}
                onEdit={() => { setEditing(entry); setEditorMode('edit') }}
                onDelete={() => setDeleting(entry)}
                onRestart={() => onStartTimer(entry.project_id, entry.description)}
                onAddTime={m => handleAddTime(entry, m)}
                onSetBillable={async b => { await window.api.time.setBillable([entry.id], b); reload() }}
              />
            ))
          )}
          </div>
        </section>

        {/* Invoices */}
        <section>
          <div className="group-head">
            <h2 className="section-title">
              Invoices <span className="ml-1 font-normal text-fg-3 num">{projectInvoices.length}</span>
            </h2>
          </div>
          <div className="card overflow-hidden">
            {projectInvoices.length === 0 ? (
              <p className="px-4 py-5 text-[13px] text-fg-3 text-center">Not invoiced yet</p>
            ) : (
              projectInvoices.map(inv => (
                <button
                  key={inv.id}
                  onClick={() => navigate(`/invoices/${inv.id}`)}
                  className="list-row group w-full flex items-center gap-3 px-4 h-[44px] hover:bg-fg/[0.025] text-left"
                >
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] font-medium text-fg">{inv.invoice_number}</div>
                    <div className="text-xs text-fg-3">{formatDay(inv.issue_date)}</div>
                  </div>
                  <div className="text-right flex flex-col items-end gap-0.5">
                    <div className="text-[13px] num text-fg">{formatMoney(inv.total)}</div>
                    <StatusBadge status={inv.status} />
                  </div>
                  <ChevronRight className="w-3.5 h-3.5 text-fg-4 group-hover:text-fg-3" />
                </button>
              ))
            )}
          </div>
        </section>
      </div>

      <ProjectForm
        open={showForm}
        project={project}
        clients={clients}
        onClose={() => setShowForm(false)}
        onSubmit={handleSubmit}
      />

      <CloseOutModal
        project={closing ? project : null}
        nextStatus={closing?.values.status || 'complete'}
        onChoose={handleCloseOut}
        onClose={() => setClosing(null)}
      />

      <EntryEditor
        mode={editorMode}
        entry={editing}
        projects={[project]}
        defaultProjectId={project.id}
        onClose={() => setEditorMode(null)}
        onSaved={() => { setEditorMode(null); loadData() }}
      />

      <ConfirmDialog
        isOpen={showDelete}
        onClose={() => setShowDelete(false)}
        onConfirm={handleDeleteProject}
        title={`Delete ${project.name}?`}
        message="This permanently deletes the project and all of its time entries. Invoices stay, but lose their link to this project."
        confirmText="Delete project"
      />

      <ConfirmDialog
        isOpen={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={handleDeleteEntry}
        title="Delete this time entry?"
        message={deleting?.invoice_id
          ? <>It's on <b className="text-fg">{deleting.invoice_number}</b>. The invoice keeps its lines, but this time will no longer be linked to it.</>
          : 'This removes the entry for good.'}
      />
    </div>
  )
}
