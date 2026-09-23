import { useState, useEffect, useMemo } from 'react'
import { useSearchParams } from 'react-router-dom'
import {
  Play, Pause, Square, Plus, Clock, Pencil, Trash2, ChevronLeft, ChevronRight, RotateCcw, Ban, CircleDollarSign,
} from 'lucide-react'
import PageHeader from '../components/PageHeader'
import ConfirmDialog from '../components/ConfirmDialog'
import EmptyState from '../components/EmptyState'
import EntryEditor, { isLockedEntry } from '../components/EntryEditor'
import Menu from '../components/Menu'
import Segmented from '../components/Segmented'
import SearchInput from '../components/SearchInput'
import { BillingChip } from '../components/StatusBadge'
import {
  formatTime, formatDurationShort, formatMoney, todayISO, addDays, toLocalISODate, parseLocalDate,
} from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import { useElapsed } from '../hooks/useTimer'
import type { TimeEntry, Project } from '@shared/types'
import toast from 'react-hot-toast'

interface Props {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  onPauseTimer: () => Promise<any>
  onResumeTimer: () => Promise<any>
  isTimerRunning: boolean
  isTimerPaused: boolean
  activeEntry: TimeEntry | null
  checkActive: () => void
}

type BillingFilter = 'all' | 'unbilled' | 'billed'

const entryDay = (e: TimeEntry) => toLocalISODate(new Date(e.start_time))

/** Monday of the week containing dateStr. */
function mondayOf(dateStr: string): string {
  const dt = parseLocalDate(dateStr)
  dt.setDate(dt.getDate() - ((dt.getDay() + 6) % 7))
  return toLocalISODate(dt)
}

function dayLabel(dateStr: string): string {
  const today = todayISO()
  if (dateStr === today) return 'Today'
  if (dateStr === addDays(today, -1)) return 'Yesterday'
  return parseLocalDate(dateStr).toLocaleDateString('en-US', { weekday: 'long', month: 'short', day: 'numeric' })
}

const entryValue = (e: TimeEntry) => (e.is_billable ? (e.duration_minutes / 60) * (e.rate || 0) : 0)

export default function TimeTracking({
  onStartTimer, onStopTimer, onPauseTimer, onResumeTimer,
  isTimerRunning, isTimerPaused, activeEntry, checkActive,
}: Props) {
  const elapsed = useElapsed(activeEntry)
  const [searchParams, setSearchParams] = useSearchParams()
  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [search, setSearch] = useState('')
  const [filter, setFilter] = useState<BillingFilter>('all')
  const [weekStart, setWeekStart] = useState(() => mondayOf(todayISO()))
  const [selectedDay, setSelectedDay] = useState<string | null>(null)
  const [editorMode, setEditorMode] = useState<'add' | 'edit' | null>(null)
  const [editing, setEditing] = useState<TimeEntry | null>(null)
  const [deleting, setDeleting] = useState<TimeEntry | null>(null)

  useEffect(() => { loadData() }, [isTimerRunning, isTimerPaused])

  // ⌘T from the menu opens the editor
  useEffect(() => {
    if (searchParams.get('action') === 'new') {
      setEditing(null)
      setEditorMode('add')
      setSearchParams({}, { replace: true })
    }
  }, [searchParams, setSearchParams])

  const loadData = async () => {
    const [e, p] = await Promise.all([window.api.time.list(), window.api.projects.list()])
    setEntries(e)
    setProjects(p.filter((x: Project) => x.status === 'active'))
  }

  const reload = () => { loadData(); notifyBillingChanged() }

  // ---- Actions ----
  const handleStop = async () => {
    const r = await onStopTimer()
    if (r?.discarded) toast('Timer ran under a minute, so it was discarded')
    else toast.success('Timer stopped')
    loadData(); checkActive()
  }
  const handlePause = async () => { await onPauseTimer(); loadData(); checkActive() }
  const handleResume = async () => { await onResumeTimer(); loadData(); checkActive() }

  const handleRestart = async (entry: TimeEntry) => {
    await onStartTimer(entry.project_id, entry.description)
    toast.success(`Timer started for ${entry.project_name}`)
    loadData()
  }

  const handleAddTime = async (entry: TimeEntry, minutesToAdd: number) => {
    const newDuration = entry.duration_minutes + minutesToAdd
    const newEnd = new Date(new Date(entry.start_time).getTime() + newDuration * 60_000)
    await window.api.time.update(entry.id, { duration_minutes: newDuration, end_time: newEnd.toISOString() })
    toast.success(`Added ${formatDurationShort(minutesToAdd)}`)
    reload()
  }

  const setBillable = async (entry: TimeEntry, billable: boolean) => {
    await window.api.time.setBillable([entry.id], billable)
    toast.success(billable ? 'Marked billable' : "Marked as don't bill")
    reload()
  }

  const handleDelete = async () => {
    if (!deleting) return
    await window.api.time.delete(deleting.id)
    toast.success('Entry deleted')
    setDeleting(null)
    reload()
  }

  // ---- Week / day derivations ----
  const today = todayISO()
  const weekDays = useMemo(() => Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)), [weekStart])
  const weekEnd = weekDays[6]
  const isCurrentWeek = weekStart === mondayOf(today)
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
  const maxDayMinutes = Math.max(60, ...Array.from(minutesByDay.values()))
  // Each day's bar is split by project color
  const segmentsByDay = useMemo(() => {
    const m = new Map<string, Map<number, { name: string; color: string; minutes: number }>>()
    for (const e of inWeek) {
      const d = entryDay(e)
      let day = m.get(d)
      if (!day) { day = new Map(); m.set(d, day) }
      const seg = day.get(e.project_id) || { name: e.project_name || 'Project', color: e.project_color || '#8E8E93', minutes: 0 }
      seg.minutes += e.duration_minutes
      day.set(e.project_id, seg)
    }
    return new Map(Array.from(m.entries()).map(([d, day]) => [d, Array.from(day.values()).sort((a, b) => b.minutes - a.minutes)]))
  }, [inWeek])
  const weekMinutes = inWeek.reduce((s, e) => s + e.duration_minutes, 0)
  const weekValue = inWeek.reduce((s, e) => s + entryValue(e), 0)
  const weekUnbilled = inWeek.filter(e => e.billing_state === 'unbilled').reduce((s, e) => s + entryValue(e), 0)

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase()
    return inWeek.filter(e =>
      (!selectedDay || entryDay(e) === selectedDay) &&
      (filter === 'all' || (filter === 'unbilled' ? e.billing_state === 'unbilled' : !!e.invoice_id || e.billing_state === 'invoiced')) &&
      (!q || (e.description || '').toLowerCase().includes(q) || (e.project_name || '').toLowerCase().includes(q) || (e.client_name || '').toLowerCase().includes(q)),
    )
  }, [inWeek, selectedDay, search, filter])

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
        amount: list.reduce((s, e) => s + entryValue(e), 0),
      }))
  }, [visible])

  const weekRangeLabel = useMemo(() => {
    const a = parseLocalDate(weekStart), b = parseLocalDate(weekEnd)
    const sameMonth = a.getMonth() === b.getMonth()
    const left = a.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
    const right = sameMonth ? String(b.getDate()) : b.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
    return `${left} – ${right}, ${b.getFullYear()}`
  }, [weekStart, weekEnd])

  const counts = useMemo(() => ({
    all: inWeek.filter(e => !selectedDay || entryDay(e) === selectedDay).length,
    unbilled: inWeek.filter(e => (!selectedDay || entryDay(e) === selectedDay) && e.billing_state === 'unbilled').length,
    billed: inWeek.filter(e => (!selectedDay || entryDay(e) === selectedDay) && (!!e.invoice_id || e.billing_state === 'invoiced')).length,
  }), [inWeek, selectedDay])

  const openAdd = () => { setEditing(null); setEditorMode('add') }
  const openEdit = (e: TimeEntry) => { setEditing(e); setEditorMode('edit') }

  return (
    <div className="page">
      <PageHeader
        title="Time"
        actions={<button onClick={openAdd} className="btn-secondary">Add time</button>}
      />

      {/* Running timer */}
      {(isTimerRunning || isTimerPaused) && activeEntry && (
        <div className="card mb-5 px-4 py-3.5 flex items-center gap-4">
          <span
            className={`w-2 h-2 rounded-full shrink-0 ${isTimerPaused ? 'bg-fg-4' : 'bg-accent animate-[timer-breathe_2s_ease-in-out_infinite]'}`}
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1">
            <div className="text-xs text-fg-3">{isTimerPaused ? 'Paused' : 'Recording'}</div>
            <div className="text-[13px] font-semibold text-fg truncate">
              {activeEntry.project_name}
              {activeEntry.description && <span className="font-normal text-fg-3"> · {activeEntry.description}</span>}
            </div>
          </div>
          <div className={`num text-[26px] leading-none font-semibold tracking-[-0.02em] ${isTimerPaused ? 'text-fg-3' : 'text-fg'}`}>{elapsed}</div>
          <div className="flex gap-2">
            <button onClick={isTimerPaused ? handleResume : handlePause} className="btn-secondary">
              {isTimerPaused ? <Play className="fill-current" /> : <Pause className="fill-current" />}
              {isTimerPaused ? 'Resume' : 'Pause'}
            </button>
            <button onClick={handleStop} className="btn-primary"><Square className="fill-current !w-3 !h-3" /> Stop</button>
          </div>
        </div>
      )}

      {/* Week */}
      <div className="card mb-5">
        <div className="flex items-center justify-between px-3 h-[42px] border-b border-line">
          <div className="flex items-center gap-1">
            <button onClick={() => { setWeekStart(addDays(weekStart, -7)); setSelectedDay(null) }} className="btn-icon-sm" title="Previous week" aria-label="Previous week">
              <ChevronLeft />
            </button>
            <button onClick={() => { setWeekStart(addDays(weekStart, 7)); setSelectedDay(null) }} className="btn-icon-sm" title="Next week" aria-label="Next week">
              <ChevronRight />
            </button>
            <span className="text-[13px] font-semibold text-fg ml-1.5">{weekRangeLabel}</span>
            {!isCurrentWeek && (
              <button onClick={() => { setWeekStart(mondayOf(today)); setSelectedDay(null) }} className="btn-ghost btn-sm ml-1">
                This week
              </button>
            )}
          </div>
          <div className="flex items-center gap-4 text-xs pr-1">
            {weekUnbilled > 0 && (
              <span className="text-fg-3">Unbilled <span className="num font-medium text-[13px] text-amber">{formatMoney(weekUnbilled)}</span></span>
            )}
            <span className="text-fg-3">Value <span className="num font-medium text-[13px] text-fg">{formatMoney(weekValue)}</span></span>
            <span className="text-fg-3">Total <span className="num font-semibold text-[13px] text-fg">{formatDurationShort(weekMinutes)}</span></span>
          </div>
        </div>
        <div className="grid grid-cols-7 gap-1 p-1.5">
          {weekDays.map(d => {
            const mins = minutesByDay.get(d) || 0
            const segments = segmentsByDay.get(d) || []
            const isToday = d === today
            const isSelected = d === selectedDay
            const dt = parseLocalDate(d)
            return (
              <button
                key={d}
                onClick={() => setSelectedDay(isSelected ? null : d)}
                title={segments.map(sg => `${sg.name} ${formatDurationShort(sg.minutes)}`).join('\n') || undefined}
                className={`rounded-[7px] px-2.5 pt-2 pb-2 text-left transition-colors ${
                  isSelected ? 'bg-fg/[0.07]' : 'hover:bg-fg/[0.035]'
                }`}
              >
                <div className="flex items-center justify-between h-5">
                  <span className={`text-2xs font-medium ${isToday ? 'text-accent-text' : 'text-fg-3'}`}>
                    {dt.toLocaleDateString('en-US', { weekday: 'short' })}
                  </span>
                  {isToday ? (
                    <span className="min-w-[20px] h-5 px-1 rounded-full bg-accent text-accent-fg text-xs font-semibold num flex items-center justify-center">{dt.getDate()}</span>
                  ) : (
                    <span className="text-[13px] font-semibold num text-fg">{dt.getDate()}</span>
                  )}
                </div>
                <div className="h-10 mt-2 flex items-end">
                  {mins > 0 ? (
                    <div
                      className="w-full flex flex-col-reverse gap-px rounded-[3px] overflow-hidden"
                      style={{ height: `${Math.max(16, (mins / maxDayMinutes) * 100)}%` }}
                    >
                      {segments.map(sg => (
                        <div key={sg.name} style={{ flex: `${sg.minutes} 1 0px`, backgroundColor: sg.color }} />
                      ))}
                    </div>
                  ) : (
                    <div className="w-full h-[2px] rounded-full bg-fg/[0.07]" />
                  )}
                </div>
                <div className={`num text-xs font-medium mt-1.5 ${mins ? 'text-fg-2' : 'text-fg-4'}`}>{mins ? formatDurationShort(mins) : '—'}</div>
              </button>
            )
          })}
        </div>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-3 mb-4">
        <Segmented
          value={filter}
          onChange={setFilter}
          options={[
            { value: 'all', label: 'All', count: counts.all },
            { value: 'unbilled', label: 'Unbilled', count: counts.unbilled },
            { value: 'billed', label: 'On an invoice', count: counts.billed },
          ]}
        />
        {selectedDay && (
          <button onClick={() => setSelectedDay(null)} className="btn-ghost btn-sm">
            Showing {dayLabel(selectedDay)} · show the whole week
          </button>
        )}
        <SearchInput value={search} onChange={setSearch} placeholder="Search this week" className="ml-auto w-64" />
      </div>

      {/* Entries by day */}
      {grouped.length === 0 ? (
        <div className="card">
          <EmptyState
            icon={Clock}
            title={completed.length === 0 ? 'No time tracked yet' : filter !== 'all' || search ? 'Nothing matches' : selectedDay ? `Nothing logged ${dayLabel(selectedDay) === 'Today' ? 'today' : `on ${dayLabel(selectedDay)}`}` : 'Nothing logged this week'}
            description={completed.length === 0 ? 'Start a timer from the toolbar or add time by hand.' : 'Try another week or filter, or add time by hand.'}
            action={{ label: 'Add time', onClick: openAdd }}
          />
        </div>
      ) : (
        <div className="space-y-4">
          {grouped.map(group => (
            <div key={group.day} className="card overflow-hidden">
              {/* Same trailing columns as the rows below: duration, value, actions */}
              <div className="grid grid-cols-[minmax(0,1fr)_64px_84px_60px] items-center gap-3 px-4 h-[34px] border-b border-line bg-fg/[0.018]">
                <div className="flex items-baseline gap-2 min-w-0">
                  <span className="text-[13px] font-semibold text-fg">{dayLabel(group.day)}</span>
                  <span className="text-xs text-fg-3">{group.entries.length} {group.entries.length === 1 ? 'entry' : 'entries'}</span>
                </div>
                <span className="text-[13px] num font-semibold text-fg text-right">{formatDurationShort(group.minutes)}</span>
                <span className="text-[13px] num text-fg-3 text-right">{group.amount > 0 ? formatMoney(group.amount) : ''}</span>
                <span />
              </div>
              <div>
                {group.entries.map(entry => (
                  <EntryRow
                    key={entry.id}
                    entry={entry}
                    onEdit={() => openEdit(entry)}
                    onDelete={() => setDeleting(entry)}
                    onRestart={() => handleRestart(entry)}
                    onAddTime={m => handleAddTime(entry, m)}
                    onSetBillable={b => setBillable(entry, b)}
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}

      <EntryEditor
        mode={editorMode}
        entry={editing}
        projects={projects}
        defaultDate={selectedDay || undefined}
        onClose={() => setEditorMode(null)}
        onSaved={() => { setEditorMode(null); loadData() }}
      />

      <ConfirmDialog
        isOpen={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={handleDelete}
        title="Delete this time entry?"
        message={deleting?.invoice_id
          ? <>It's on <b className="text-fg">{deleting.invoice_number}</b>. The invoice keeps its lines, but this time will no longer be linked to it.</>
          : 'This removes the entry for good.'}
      />
    </div>
  )
}

export function EntryRow({ entry, onEdit, onDelete, onRestart, onAddTime, onSetBillable, showProject = true }: {
  entry: TimeEntry
  onEdit: () => void
  onDelete: () => void
  onRestart?: () => void
  onAddTime?: (minutes: number) => void
  onSetBillable?: (billable: boolean) => void
  showProject?: boolean
}) {
  const locked = isLockedEntry(entry)
  const unbilled = entry.billing_state === 'unbilled'
  const value = entryValue(entry)

  return (
    <div
      onDoubleClick={onEdit}
      className={`list-row group grid grid-cols-[minmax(130px,1.3fr)_minmax(0,1fr)_auto_auto_64px_84px_60px] items-center gap-3 px-4 hover:bg-fg/[0.025] ${showProject ? 'h-[44px] [--inset:32px]' : 'h-[36px]'}`}
    >
      <div className="min-w-0 flex items-center gap-2.5">
        {showProject ? (
          <>
            <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: entry.project_color }} />
            <div className="min-w-0">
              <div className="text-sm text-fg truncate">{entry.project_name}</div>
              {entry.client_name && <div className="text-xs text-fg-3 truncate">{entry.client_name}</div>}
            </div>
          </>
        ) : (
          <div className="text-sm text-fg-2 num">
            {new Date(entry.start_time).toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' })}
          </div>
        )}
      </div>
      <div className="min-w-0 text-sm text-fg-2 truncate" title={entry.description}>
        {entry.description}
      </div>
      <div className="text-xs text-fg-3 num whitespace-nowrap text-right">
        {formatTime(entry.start_time)} – {entry.end_time ? formatTime(entry.end_time) : '…'}
      </div>
      <div className="flex justify-end">
        <BillingChip state={entry.billing_state} invoiceId={entry.invoice_id} invoiceNumber={entry.invoice_number} />
      </div>
      <div className="text-[13px] font-medium text-fg num text-right">{formatDurationShort(entry.duration_minutes)}</div>
      <div className={`text-[13px] num text-right ${entry.is_billable ? 'text-fg-3' : 'text-fg-4'}`}>
        {entry.is_billable ? formatMoney(value) : '—'}
      </div>
      <div className="flex items-center justify-end gap-0.5 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity">
        <button onClick={onEdit} className="btn-icon-sm" title="Edit" aria-label="Edit entry"><Pencil /></button>
        <Menu
          items={[
            onRestart && { label: 'Start timer for this', icon: RotateCcw, onClick: onRestart },
            !locked && onAddTime && 'separator',
            !locked && onAddTime && { label: 'Add 15 minutes', icon: Plus, onClick: () => onAddTime(15) },
            !locked && onAddTime && { label: 'Add 30 minutes', icon: Plus, onClick: () => onAddTime(30) },
            !locked && onAddTime && { label: 'Add 1 hour', icon: Plus, onClick: () => onAddTime(60) },
            onSetBillable && (unbilled || entry.billing_state === 'nonbillable') && 'separator',
            onSetBillable && unbilled && { label: "Don't bill this", icon: Ban, onClick: () => onSetBillable(false) },
            onSetBillable && entry.billing_state === 'nonbillable' && { label: 'Make billable', icon: CircleDollarSign, onClick: () => onSetBillable(true) },
            'separator',
            { label: 'Delete', icon: Trash2, danger: true, onClick: onDelete },
          ]}
        />
      </div>
    </div>
  )
}
