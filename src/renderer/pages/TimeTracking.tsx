import { useState, useEffect, useMemo } from 'react'
import { motion } from 'framer-motion'
import {
  Play, Pause, Square, Plus, Clock, Trash2, Pencil, Search,
  ChevronLeft, ChevronRight, RotateCcw,
} from 'lucide-react'
import Modal from '../components/Modal'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import {
  formatTime, formatDurationShort, formatMoney, todayISO, addDays,
  toLocalISODate, parseLocalDate, parseDurationInput,
} from '../utils/format'
import type { TimeEntry, Project } from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

interface Props {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  onPauseTimer: () => Promise<any>
  onResumeTimer: () => Promise<any>
  isTimerRunning: boolean
  isTimerPaused: boolean
  activeEntry: TimeEntry | null
  elapsed: string
  checkActive: () => void
}

// ---- date helpers (all LOCAL time) ----
const pad = (n: number) => String(n).padStart(2, '0')
const entryDay = (e: TimeEntry) => toLocalISODate(new Date(e.start_time))
const hhmm = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`

/** Monday of the week containing dateStr. */
function mondayOf(dateStr: string): string {
  const dt = parseLocalDate(dateStr)
  const dow = (dt.getDay() + 6) % 7 // Mon = 0 … Sun = 6
  dt.setDate(dt.getDate() - dow)
  return toLocalISODate(dt)
}

/** Minutes from date+start to date+end; an end at or before start rolls to the next day. */
function minutesBetween(date: string, start: string, end: string): number {
  const s = new Date(`${date}T${start}:00`).getTime()
  let e = new Date(`${date}T${end}:00`).getTime()
  if (e <= s) e += 86_400_000
  return Math.round((e - s) / 60_000)
}

function endFromDuration(date: string, start: string, mins: number): string {
  const s = new Date(`${date}T${start}:00`)
  s.setMinutes(s.getMinutes() + mins)
  return hhmm(s)
}

function dayLabel(dateStr: string): string {
  const today = todayISO()
  if (dateStr === today) return 'Today'
  if (dateStr === addDays(today, -1)) return 'Yesterday'
  return parseLocalDate(dateStr).toLocaleDateString('en-US', { weekday: 'long', month: 'short', day: 'numeric' })
}

interface EditorForm {
  project_id: number
  description: string
  date: string
  start: string
  end: string
  durationText: string
  is_billable: boolean
}

export default function TimeTracking({
  onStartTimer, onStopTimer, onPauseTimer, onResumeTimer,
  isTimerRunning, isTimerPaused, activeEntry, elapsed, checkActive,
}: Props) {
  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [search, setSearch] = useState('')
  const [showStartForm, setShowStartForm] = useState(false)
  const [deleteId, setDeleteId] = useState<number | null>(null)

  // Week navigation
  const [weekStart, setWeekStart] = useState(() => mondayOf(todayISO()))
  const [selectedDay, setSelectedDay] = useState<string | null>(null)

  // Start timer form
  const [startProjectId, setStartProjectId] = useState(0)
  const [startDescription, setStartDescription] = useState('')

  // Shared add/edit editor
  const [editorMode, setEditorMode] = useState<'add' | 'edit' | null>(null)
  const [editingEntry, setEditingEntry] = useState<TimeEntry | null>(null)
  const [form, setForm] = useState<EditorForm>({
    project_id: 0, description: '', date: todayISO(), start: '09:00', end: '10:00', durationText: '1h', is_billable: true,
  })
  const [saving, setSaving] = useState(false)

  useEffect(() => { loadData() }, [])
  useEffect(() => { loadData() }, [isTimerRunning, isTimerPaused])

  const loadData = async () => {
    const [e, p] = await Promise.all([window.api.time.list(), window.api.projects.list()])
    setEntries(e)
    const active = p.filter((x: any) => x.status === 'active')
    setProjects(active)
    if (active.length > 0 && !startProjectId) setStartProjectId(active[0].id)
  }

  // ---- Timer controls ----
  const handleStart = async () => {
    if (!startProjectId) return toast.error('Select a project')
    await onStartTimer(startProjectId, startDescription)
    setShowStartForm(false); setStartDescription('')
    toast.success('Timer started'); loadData()
  }
  const handleStop = async () => { await onStopTimer(); toast.success('Timer stopped'); loadData(); checkActive() }
  const handlePause = async () => { await onPauseTimer(); toast.success('Timer paused'); loadData(); checkActive() }
  const handleResume = async () => { await onResumeTimer(); toast.success('Timer resumed'); loadData(); checkActive() }

  /** Start a fresh timer for the same project + description as a past entry. */
  const handleRestart = async (entry: TimeEntry) => {
    await onStartTimer(entry.project_id, entry.description)
    toast.success(`Restarted: ${entry.project_name}`); loadData()
  }

  // ---- Week / day derivations ----
  const weekDays = useMemo(() => Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)), [weekStart])
  const weekEnd = weekDays[6]
  const isCurrentWeek = weekStart === mondayOf(todayISO())
  const completed = useMemo(() => entries.filter(e => e.end_time), [entries])

  const inWeek = useMemo(
    () => completed.filter(e => { const d = entryDay(e); return d >= weekStart && d <= weekEnd }),
    [completed, weekStart, weekEnd],
  )
  const minutesByDay = useMemo(() => {
    const m = new Map<string, number>()
    for (const e of inWeek) m.set(entryDay(e), (m.get(entryDay(e)) || 0) + e.duration_minutes)
    return m
  }, [inWeek])
  const weekMinutes = useMemo(() => inWeek.reduce((s, e) => s + e.duration_minutes, 0), [inWeek])

  const visible = useMemo(() => {
    const q = search.toLowerCase()
    return inWeek.filter(e =>
      (!selectedDay || entryDay(e) === selectedDay) &&
      (!q || (e.description || '').toLowerCase().includes(q) || (e.project_name || '').toLowerCase().includes(q)),
    )
  }, [inWeek, selectedDay, search])

  /** Entries grouped by day, newest day first, newest entry first within a day. */
  const grouped = useMemo(() => {
    const map = new Map<string, TimeEntry[]>()
    for (const e of visible) {
      const d = entryDay(e)
      if (!map.has(d)) map.set(d, [])
      map.get(d)!.push(e)
    }
    return Array.from(map.entries())
      .sort((a, b) => b[0].localeCompare(a[0]))
      .map(([day, list]) => ({
        day,
        entries: list.sort((a, b) => b.start_time.localeCompare(a.start_time)),
        minutes: list.reduce((s, e) => s + e.duration_minutes, 0),
        amount: list.reduce((s, e) => s + (e.is_billable ? (e.duration_minutes / 60) * (e.rate || 0) : 0), 0),
      }))
  }, [visible])

  const weekRangeLabel = useMemo(() => {
    const a = parseLocalDate(weekStart), b = parseLocalDate(weekEnd)
    const fmt = (d: Date, withYear = false) =>
      d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', ...(withYear ? { year: 'numeric' } : {}) })
    return `${fmt(a)} – ${fmt(b, true)}`
  }, [weekStart, weekEnd])

  // ---- Editor (shared by Manual Entry + Edit) ----
  const openAdd = () => {
    setEditorMode('add'); setEditingEntry(null)
    const date = selectedDay || todayISO()
    setForm({
      project_id: projects[0]?.id || 0, description: '', date,
      start: '09:00', end: '10:00', durationText: formatDurationShort(60), is_billable: true,
    })
  }
  const openEdit = (entry: TimeEntry) => {
    const s = new Date(entry.start_time)
    const e = entry.end_time ? new Date(entry.end_time) : new Date(s.getTime() + entry.duration_minutes * 60_000)
    setEditorMode('edit'); setEditingEntry(entry)
    setForm({
      project_id: entry.project_id, description: entry.description || '',
      date: toLocalISODate(s), start: hhmm(s), end: hhmm(e),
      durationText: formatDurationShort(entry.duration_minutes), is_billable: !!entry.is_billable,
    })
  }
  const closeEditor = () => { setEditorMode(null); setEditingEntry(null) }

  // Keep start/end/duration in sync. Start or end changes → duration follows.
  // Duration changes → end follows. This is what makes editing feel natural.
  const setTimes = (patch: Partial<Pick<EditorForm, 'date' | 'start' | 'end'>>) => setForm(f => {
    const next = { ...f, ...patch }
    return { ...next, durationText: formatDurationShort(minutesBetween(next.date, next.start, next.end)) }
  })
  const setDurationText = (text: string) => setForm(f => {
    const mins = parseDurationInput(text)
    if (mins == null || mins <= 0) return { ...f, durationText: text }
    return { ...f, durationText: text, end: endFromDuration(f.date, f.start, mins) }
  })
  const editorMinutes = minutesBetween(form.date, form.start, form.end)
  const durationInvalid = parseDurationInput(form.durationText) == null

  const handleSaveEditor = async () => {
    if (editorMode === 'add' && !form.project_id) return toast.error('Select a project')
    if (editorMinutes <= 0) return toast.error('Duration must be greater than zero')
    setSaving(true)
    try {
      const startDate = new Date(`${form.date}T${form.start}:00`)
      const endDate = new Date(startDate.getTime() + editorMinutes * 60_000) // authoritative; handles overnight
      const payload = {
        description: form.description,
        start_time: startDate.toISOString(),
        end_time: endDate.toISOString(),
        duration_minutes: editorMinutes,
        is_billable: form.is_billable ? 1 : 0,
      }
      if (editorMode === 'edit' && editingEntry) {
        await window.api.time.update(editingEntry.id, payload)
        toast.success('Entry updated')
      } else {
        await window.api.time.create({ ...payload, project_id: form.project_id, is_invoiced: 0 })
        toast.success('Time entry added')
      }
      closeEditor(); loadData()
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  const handleAddTime = async (entry: TimeEntry, minutesToAdd: number) => {
    const newDuration = entry.duration_minutes + minutesToAdd
    const newEnd = new Date(new Date(entry.start_time).getTime() + newDuration * 60_000)
    await window.api.time.update(entry.id, { duration_minutes: newDuration, end_time: newEnd.toISOString() })
    toast.success(`Added ${formatDurationShort(minutesToAdd)}`); loadData()
  }

  const handleDelete = async () => {
    if (!deleteId) return
    await window.api.time.delete(deleteId)
    toast.success('Entry deleted'); setDeleteId(null); loadData()
  }

  const today = todayISO()

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      {/* Header */}
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Time Tracking</h1>
          <p className="text-sm text-text-secondary mt-1">
            <span className="font-mono text-text-primary">{formatDurationShort(weekMinutes)}</span> this week
          </p>
        </div>
        <div className="flex gap-2">
          <button onClick={openAdd} className="btn-secondary flex items-center gap-2">
            <Plus className="w-4 h-4" /> Manual Entry
          </button>
          {isTimerRunning || isTimerPaused ? (
            <div className="flex gap-2">
              <button onClick={isTimerPaused ? handleResume : handlePause} className={`${isTimerPaused ? 'btn-primary' : 'btn-secondary'} flex items-center gap-2`}>
                {isTimerPaused ? <Play className="w-4 h-4 fill-current" /> : <Pause className="w-4 h-4 fill-current" />}
                {isTimerPaused ? 'Resume' : 'Pause'} {elapsed}
              </button>
              <button onClick={handleStop} className="btn-danger flex items-center gap-2"><Square className="w-4 h-4 fill-current" /> Stop</button>
            </div>
          ) : (
            <button onClick={() => setShowStartForm(true)} className="btn-primary flex items-center gap-2"><Play className="w-4 h-4" /> Start Timer</button>
          )}
        </div>
      </motion.div>

      {/* Active Timer */}
      {(isTimerRunning || isTimerPaused) && activeEntry && (
        <motion.div variants={item} className={`glass-panel p-6 mb-6 border ${isTimerPaused ? 'border-status-paused/20' : 'border-accent/20'}`}>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-4">
              <motion.div animate={isTimerPaused ? undefined : { scale: [1, 1.2, 1] }} transition={isTimerPaused ? undefined : { duration: 2, repeat: Infinity }}
                className={`w-3 h-3 rounded-full ${isTimerPaused ? 'bg-status-paused' : 'bg-accent'}`} />
              <div>
                <div className={`font-mono text-3xl font-bold tracking-wider ${isTimerPaused ? 'text-status-paused' : 'text-accent'}`}>{elapsed}</div>
                <div className="text-sm text-text-secondary mt-1">{activeEntry.project_name} · {activeEntry.description || 'No description'} · {isTimerPaused ? 'Paused' : 'Recording'}</div>
              </div>
            </div>
            <div className="flex gap-2">
              <button onClick={isTimerPaused ? handleResume : handlePause} className={`${isTimerPaused ? 'btn-primary' : 'btn-secondary'} flex items-center gap-2`}>
                {isTimerPaused ? <Play className="w-4 h-4 fill-current" /> : <Pause className="w-4 h-4 fill-current" />}{isTimerPaused ? 'Resume' : 'Pause'}
              </button>
              <button onClick={handleStop} className="btn-danger flex items-center gap-2"><Square className="w-4 h-4 fill-current" /> Stop Timer</button>
            </div>
          </div>
        </motion.div>
      )}

      {/* Week strip */}
      <motion.div variants={item} className="glass-panel p-4 mb-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-1">
            <button onClick={() => { setWeekStart(addDays(weekStart, -7)); setSelectedDay(null) }} className="p-1.5 hover:bg-surface-300 rounded-lg transition-colors" title="Previous week">
              <ChevronLeft className="w-4 h-4 text-text-secondary" />
            </button>
            <button onClick={() => { setWeekStart(addDays(weekStart, 7)); setSelectedDay(null) }} className="p-1.5 hover:bg-surface-300 rounded-lg transition-colors" title="Next week">
              <ChevronRight className="w-4 h-4 text-text-secondary" />
            </button>
            <span className="text-sm font-medium text-text-primary ml-1">{weekRangeLabel}</span>
            {!isCurrentWeek && (
              <button onClick={() => { setWeekStart(mondayOf(today)); setSelectedDay(null) }} className="ml-2 text-xs text-accent hover:text-accent-light transition-colors">This week</button>
            )}
          </div>
          <div className="text-xs text-text-tertiary">
            {selectedDay
              ? <button onClick={() => setSelectedDay(null)} className="hover:text-text-primary transition-colors">Showing {dayLabel(selectedDay)} · <span className="text-accent">show whole week</span></button>
              : 'Click a day to focus it'}
          </div>
        </div>
        <div className="grid grid-cols-7 gap-1.5">
          {weekDays.map(d => {
            const mins = minutesByDay.get(d) || 0
            const isToday = d === today
            const isSelected = d === selectedDay
            const dt = parseLocalDate(d)
            return (
              <button key={d} onClick={() => setSelectedDay(isSelected ? null : d)}
                className={`rounded-lg px-2 py-2 text-center transition-colors border ${
                  isSelected ? 'bg-accent/15 border-accent/40'
                    : 'border-transparent hover:bg-surface-200/60'
                } ${isToday && !isSelected ? 'ring-1 ring-accent/30' : ''}`}>
                <div className="text-[10px] uppercase tracking-wider text-text-tertiary">{dt.toLocaleDateString('en-US', { weekday: 'short' })}</div>
                <div className={`text-sm font-semibold ${isToday ? 'text-accent' : 'text-text-primary'}`}>{dt.getDate()}</div>
                <div className={`font-mono text-[11px] mt-0.5 ${mins ? 'text-text-secondary' : 'text-text-tertiary/50'}`}>{mins ? formatDurationShort(mins) : '—'}</div>
              </button>
            )
          })}
        </div>
      </motion.div>

      {/* Search */}
      {inWeek.length > 0 && (
        <motion.div variants={item} className="mb-4 relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-tertiary" />
          <input type="text" placeholder="Search this week's entries…" value={search} onChange={e => setSearch(e.target.value)} className="input-field pl-10" />
        </motion.div>
      )}

      {/* Grouped entries */}
      {grouped.length === 0 ? (
        <EmptyState icon={Clock}
          title={completed.length === 0 ? 'No time entries' : selectedDay ? `Nothing logged ${dayLabel(selectedDay).toLowerCase() === 'today' ? 'today' : `on ${dayLabel(selectedDay)}`}` : 'Nothing logged this week'}
          description={completed.length === 0 ? 'Start a timer or add a manual entry to begin tracking your time.' : 'Try another week, or add a manual entry for this day.'}
          action={{ label: 'Manual Entry', onClick: openAdd }} />
      ) : (
        <motion.div variants={item} className="space-y-4">
          {grouped.map(group => (
            <div key={group.day} className="glass-panel overflow-hidden">
              {/* Day header */}
              <div className="flex items-center justify-between px-4 py-2.5 border-b border-rim/[0.05] bg-surface-200/30">
                <div className="flex items-baseline gap-2">
                  <span className={`text-sm font-semibold ${group.day === today ? 'text-accent' : 'text-text-primary'}`}>{dayLabel(group.day)}</span>
                  <span className="text-[11px] text-text-tertiary">{group.entries.length} {group.entries.length === 1 ? 'entry' : 'entries'}</span>
                </div>
                <div className="flex items-baseline gap-3 font-mono text-sm">
                  {group.amount > 0 && <span className="text-text-secondary">{formatMoney(group.amount)}</span>}
                  <span className="font-semibold text-text-primary">{formatDurationShort(group.minutes)}</span>
                </div>
              </div>
              <table className="w-full">
                <tbody>
                  {group.entries.map(entry => (
                    <tr key={entry.id} className="border-b border-rim/[0.02] last:border-0 hover:bg-surface-200/30 transition-colors group">
                      <td className="px-4 py-3 w-[140px]">
                        <div className="flex items-center gap-2">
                          <div className="w-2 h-2 rounded-full flex-shrink-0" style={{ backgroundColor: entry.project_color }} />
                          <span className="text-sm text-text-primary truncate">{entry.project_name}</span>
                        </div>
                      </td>
                      <td className="px-4 py-3 text-sm text-text-secondary">{entry.description || <span className="text-text-tertiary">—</span>}</td>
                      <td className="px-4 py-3 text-xs font-mono text-text-tertiary whitespace-nowrap text-right">
                        {formatTime(entry.start_time)} – {entry.end_time ? formatTime(entry.end_time) : '…'}
                      </td>
                      <td className="px-4 py-3 font-mono text-sm text-text-primary text-right whitespace-nowrap">{formatDurationShort(entry.duration_minutes)}</td>
                      <td className="px-4 py-3 font-mono text-sm text-text-primary text-right whitespace-nowrap w-[100px]">
                        {entry.is_billable ? formatMoney((entry.duration_minutes / 60) * (entry.rate || 0)) : <span className="text-text-tertiary">—</span>}
                      </td>
                      <td className="px-3 py-3 w-[220px]">
                        <div className="flex items-center gap-0.5 justify-end opacity-0 group-hover:opacity-100 transition-opacity">
                          {[15, 30, 60].map(mins => (
                            <button key={mins} onClick={() => handleAddTime(entry, mins)} title={`Add ${formatDurationShort(mins)}`}
                              className="px-1.5 py-0.5 text-[10px] font-mono font-medium text-text-tertiary hover:text-accent hover:bg-accent/10 rounded transition-colors">
                              +{mins >= 60 ? `${mins / 60}h` : `${mins}m`}
                            </button>
                          ))}
                          <div className="w-px h-4 bg-rim/[0.06] mx-0.5" />
                          <button onClick={() => handleRestart(entry)} className="p-1.5 hover:bg-accent/10 rounded transition-colors" title="Restart a timer for this">
                            <RotateCcw className="w-3.5 h-3.5 text-text-tertiary hover:text-accent" />
                          </button>
                          <button onClick={() => openEdit(entry)} className="p-1.5 hover:bg-surface-300 rounded transition-colors" title="Edit">
                            <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
                          </button>
                          <button onClick={() => setDeleteId(entry.id)} className="p-1.5 hover:bg-red-500/10 rounded transition-colors" title="Delete">
                            <Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" />
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </motion.div>
      )}

      {/* Start Timer Modal */}
      <Modal isOpen={showStartForm} onClose={() => setShowStartForm(false)} title="Start Timer" size="sm">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Project</label>
            <select className="input-field" value={startProjectId} onChange={e => setStartProjectId(parseInt(e.target.value))}>
              {projects.map(p => <option key={p.id} value={p.id}>{p.name} ({(p as any).client_name})</option>)}
            </select>
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <input className="input-field" value={startDescription} onChange={e => setStartDescription(e.target.value)} placeholder="What are you working on?" autoFocus />
          </div>
          <div className="flex justify-end gap-3">
            <button onClick={() => setShowStartForm(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleStart} className="btn-primary flex items-center gap-2"><Play className="w-4 h-4" /> Start</button>
          </div>
        </div>
      </Modal>

      {/* Add / Edit entry — one editor, start/end/duration stay in sync */}
      <Modal isOpen={editorMode !== null} onClose={closeEditor} title={editorMode === 'edit' ? 'Edit Time Entry' : 'Manual Time Entry'}>
        <div className="space-y-4">
          {editorMode === 'add' && (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Project</label>
              <select className="input-field" value={form.project_id} onChange={e => setForm(f => ({ ...f, project_id: parseInt(e.target.value) }))}>
                {projects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
          )}
          {editorMode === 'edit' && editingEntry && (
            <div className="flex items-center gap-2 text-sm text-text-secondary">
              <div className="w-2 h-2 rounded-full" style={{ backgroundColor: editingEntry.project_color }} />{editingEntry.project_name}
            </div>
          )}
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
            <input className="input-field" value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))} placeholder="What did you work on?" autoFocus={editorMode === 'add'} />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Date</label>
            <input className="input-field [color-scheme:dark]" type="date" value={form.date} onChange={e => setTimes({ date: e.target.value })} />
          </div>
          <div className="grid grid-cols-3 gap-3">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Start</label>
              <input className="input-field [color-scheme:dark]" type="time" value={form.start} onChange={e => setTimes({ start: e.target.value })} />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">End</label>
              <input className="input-field [color-scheme:dark]" type="time" value={form.end} onChange={e => setTimes({ end: e.target.value })} />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Duration</label>
              <input className={`input-field font-mono ${durationInvalid ? 'border-red-500/40' : ''}`} value={form.durationText}
                onChange={e => setDurationText(e.target.value)} placeholder="2h 30m" />
            </div>
          </div>
          <div className="flex items-center justify-between">
            <p className="text-[11px] text-text-tertiary">
              {durationInvalid ? 'Try "2h 30m", "1.5h", "2:30", or minutes' : `${formatDurationShort(editorMinutes)} · end past midnight rolls to the next day`}
            </p>
            <div className="flex gap-1">
              {[15, 30, 60].map(m => (
                <button key={m} type="button" onClick={() => setDurationText(formatDurationShort(editorMinutes + m))}
                  className="px-2 py-1 text-[11px] font-mono text-accent bg-accent/10 hover:bg-accent/20 rounded-md transition-colors">+{m >= 60 ? '1h' : `${m}m`}</button>
              ))}
            </div>
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" checked={form.is_billable} onChange={e => setForm(f => ({ ...f, is_billable: e.target.checked }))} className="rounded border-rim/20 bg-surface-300 text-accent focus:ring-accent" />
            <span className="text-sm text-text-secondary">Billable</span>
          </label>
          <div className="flex justify-end gap-3 pt-1">
            <button onClick={closeEditor} className="btn-secondary">Cancel</button>
            <button onClick={handleSaveEditor} disabled={saving || durationInvalid || editorMinutes <= 0}
              className={`btn-primary ${saving || durationInvalid || editorMinutes <= 0 ? 'opacity-50 cursor-not-allowed' : ''}`}>
              {saving ? 'Saving…' : editorMode === 'edit' ? 'Save' : 'Add Entry'}
            </button>
          </div>
        </div>
      </Modal>

      <ConfirmDialog isOpen={deleteId !== null} onClose={() => setDeleteId(null)} onConfirm={handleDelete}
        title="Delete Time Entry" message="Are you sure you want to delete this time entry?" />
    </motion.div>
  )
}
