import { useState, useEffect, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { Play, Pause, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import { BillingChip } from '../components/StatusBadge'
import EmptyState from '../components/EmptyState'
import TodayRuler from '../components/TodayRuler'
import { Mascot, IconHourglass, IconSend, IconAlert, IconTrophy, type Mood } from '../components/Illustrations'
import {
  formatMoney, formatDurationShort, formatHoursShort, formatRelative, todayISO, addDays, toLocalISODate, parseLocalDate,
} from '../utils/format'
import { onBillingChanged } from '../utils/events'
import { ATTENTION_ICON } from '../utils/attention'
import { elapsedSeconds } from '../hooks/useTimer'
import type { BillingOverview, DashboardStats, TimeEntry, Project } from '@shared/types'

interface DashboardProps {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  onPauseTimer: () => Promise<any>
  onResumeTimer: () => Promise<any>
  isTimerRunning: boolean
  isTimerPaused: boolean
  activeEntry: TimeEntry | null
}

function mondayOf(dateStr: string): string {
  const dt = parseLocalDate(dateStr)
  dt.setDate(dt.getDate() - ((dt.getDay() + 6) % 7))
  return toLocalISODate(dt)
}

interface Segment { name: string; color: string; minutes: number }

export default function Dashboard({
  onStartTimer, onPauseTimer, onResumeTimer, isTimerRunning, isTimerPaused, activeEntry,
}: DashboardProps) {
  const navigate = useNavigate()
  const [stats, setStats] = useState<DashboardStats | null>(null)
  const [billing, setBilling] = useState<BillingOverview | null>(null)
  const [entries, setEntries] = useState<TimeEntry[]>([])
  const [projects, setProjects] = useState<Project[]>([])

  const load = async () => {
    const [s, b, e, p] = await Promise.all([
      window.api.dashboard.stats(),
      window.api.billing.overview(),
      window.api.time.list(),
      window.api.projects.list(),
    ])
    setStats(s)
    setBilling(b)
    setEntries(e)
    setProjects(p.filter((x: Project) => x.status === 'active'))
  }

  useEffect(() => { load() }, [isTimerRunning, isTimerPaused])
  useEffect(() => onBillingChanged(load), [])

  const today = todayISO()

  const week = useMemo(() => {
    const start = mondayOf(today)
    const days = Array.from({ length: 7 }, (_, i) => addDays(start, i))
    const byDay = new Map<string, Map<number, Segment>>()
    for (const e of entries) {
      if (!e.end_time) continue
      const d = toLocalISODate(new Date(e.start_time))
      if (d < days[0] || d > days[6]) continue
      let day = byDay.get(d)
      if (!day) { day = new Map(); byDay.set(d, day) }
      const seg = day.get(e.project_id) || { name: e.project_name || 'Project', color: e.project_color || '#AFAFAF', minutes: 0 }
      seg.minutes += e.duration_minutes
      day.set(e.project_id, seg)
    }
    const segments = days.map(d => Array.from(byDay.get(d)?.values() || []).sort((a, b) => b.minutes - a.minutes))
    const totals = segments.map(list => list.reduce((s, x) => s + x.minutes, 0))
    return { days, segments, totals, max: Math.max(60, ...totals), total: totals.reduce((a, b) => a + b, 0) }
  }, [entries, today])

  const recent = useMemo(() => entries.filter(e => e.end_time).slice(0, 6), [entries])
  const quickProjects = useMemo(() => [...projects]
    .sort((a, b) => String(b.last_activity || b.created_at).localeCompare(String(a.last_activity || a.created_at)))
    .slice(0, 5), [projects])

  if (!stats || !billing) return null
  const { pipeline } = billing
  const attention = billing.attention.slice(0, 4)
  const dateLine = new Date().toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric' })
  const monthName = new Date().toLocaleDateString('en-US', { month: 'long' })

  // Today's hours, including a running timer
  const runningToday = activeEntry && toLocalISODate(new Date(activeEntry.start_time)) === today ? elapsedSeconds(activeEntry) / 3600 : 0
  const todayHours = (stats.hours_today || 0) + runningToday

  // What the mascot says: the most useful thing right now
  let mood: Mood = 'idle'
  let message: string
  let action: { label: string; to: string; primary?: boolean } | null = null
  if (pipeline.overdue_count > 0) {
    mood = 'worried'
    message = `${formatMoney(pipeline.overdue_amount)} is overdue. A friendly nudge usually does the trick.`
    action = { label: 'See overdue', to: '/billing?status=overdue' }
  } else if (isTimerRunning && activeEntry) {
    mood = 'happy'
    message = `You're on the clock for ${activeEntry.project_name}. Nice focus!`
  } else if (pipeline.unbilled_amount > 0 && billing.attention.some(a => a.kind === 'unbilled')) {
    mood = 'wave'
    message = `${formatMoney(pipeline.unbilled_amount)} is ready to bill whenever you are.`
    action = { label: 'Bill it', to: '/billing', primary: true }
  } else if (todayHours === 0) {
    message = "Fresh day, clean slate. Start a timer when you're ready."
  } else {
    mood = 'happy'
    message = `Nice work! ${formatHoursShort(todayHours)} tracked so far today.`
  }

  return (
    <div className="page">
      <PageHeader title="Dashboard" subtitle={dateLine} />

      {/* The mascot's take on today */}
      <div className="card flex items-center gap-4 pl-3 pr-4 py-2.5 mb-5">
        <Mascot size={58} mood={mood} motion={mood === 'happy' ? 'hop' : 'bob'} interactive />
        <p className="flex-1 min-w-0 text-[16px] leading-[22px] font-semibold text-fg">{message}</p>
        {action && (
          <button onClick={() => navigate(action!.to)} className={action.primary ? 'btn-primary' : 'btn-secondary'}>
            {action.label}
          </button>
        )}
      </div>

      <MetricStrip className="mb-6">
        <Metric
          label="Ready to bill"
          icon={<IconHourglass />}
          tint="orange"
          value={<Money animate amount={pipeline.unbilled_amount} />}
          sub={pipeline.unbilled_hours > 0 ? `${formatHoursShort(pipeline.unbilled_hours)} not on an invoice` : 'All caught up'}
          onClick={() => navigate('/billing')}
        />
        <Metric
          label="Awaiting payment"
          icon={<IconSend />}
          tint="blue"
          value={<Money animate amount={pipeline.awaiting_amount + pipeline.draft_amount} />}
          sub={pipeline.draft_count ? `${pipeline.awaiting_count} sent · ${pipeline.draft_count} draft` : `${pipeline.awaiting_count} invoice${pipeline.awaiting_count === 1 ? '' : 's'} sent`}
          onClick={() => navigate('/billing?status=sent')}
        />
        <Metric
          label="Overdue"
          icon={<IconAlert />}
          tint="red"
          value={<Money animate amount={pipeline.overdue_amount} className={pipeline.overdue_amount > 0 ? 'text-red' : ''} />}
          sub={pipeline.overdue_count ? `${pipeline.overdue_count} past due` : 'Nothing late'}
          onClick={() => navigate('/billing?status=overdue')}
        />
        <Metric
          label={`Paid in ${today.slice(0, 4)}`}
          icon={<IconTrophy />}
          tint="green"
          value={<Money animate amount={pipeline.paid_ytd_amount} />}
          sub={`${pipeline.paid_ytd_count} invoice${pipeline.paid_ytd_count === 1 ? '' : 's'}`}
          onClick={() => navigate('/billing?status=paid')}
        />
      </MetricStrip>

      <div className="mb-7">
        <TodayRuler entries={entries} activeEntry={activeEntry} />
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_310px] gap-6 items-start">
        <div className="space-y-7 min-w-0">
          {/* Needs attention */}
          <section>
            <div className="group-head">
              <h2 className="section-title">
                Needs attention
                {billing.attention.length > 0 && <span className="ml-2 align-middle inline-flex items-center justify-center min-w-[20px] h-[20px] px-1.5 rounded-full bg-red text-white text-[11.5px] font-bold">{billing.attention.length}</span>}
              </h2>
              {billing.attention.length > 0 && (
                <button onClick={() => navigate('/billing')} className="head-link">Open billing <ChevronRight /></button>
              )}
            </div>
            <div className="card overflow-hidden">
              {attention.length === 0 ? (
                <div className="flex items-center gap-4 px-4 py-3">
                  <Mascot size={52} mood="happy" interactive />
                  <span className="text-[15px] font-bold text-fg">All square! Nothing is late or waiting to be billed.</span>
                </div>
              ) : attention.map(item => {
                const Icon = ATTENTION_ICON[item.kind]
                const to = item.invoice_id ? `/invoices/${item.invoice_id}` : item.project_id ? `/projects/${item.project_id}` : '/billing'
                return (
                  <button
                    key={item.key}
                    onClick={() => navigate(to)}
                    className="list-row [--inset:64px] group wiggle-on-hover w-full flex items-center gap-3.5 px-4 h-[62px] hover:bg-fg/[0.03] text-left transition-colors"
                  >
                    <Icon className="wiggle-target w-9 h-9 shrink-0" />
                    <div className="min-w-0 flex-1">
                      <div className="text-[15px] font-bold text-fg truncate">{item.title}</div>
                      <div className="text-[12.5px] font-semibold text-fg-3 truncate">{item.detail}</div>
                    </div>
                    {item.amount != null && <span className="font-figures text-[15px] text-fg">{formatMoney(item.amount)}</span>}
                    <ChevronRight className="w-5 h-5 text-fg-4 group-hover:text-fg-3 group-hover:translate-x-1 transition-transform" strokeWidth={3} />
                  </button>
                )
              })}
              {billing.attention.length > attention.length && (
                <button onClick={() => navigate('/billing')} className="w-full px-4 h-[44px] text-[13px] font-semibold text-accent-text hover:bg-fg/[0.03] border-t border-line text-left">
                  {billing.attention.length - attention.length} more in Billing
                </button>
              )}
            </div>
          </section>

          {/* Recent time */}
          <section>
            <div className="group-head">
              <h2 className="section-title">Recent time</h2>
              <button onClick={() => navigate('/time')} className="head-link">View all <ChevronRight /></button>
            </div>
            <div className="card overflow-hidden">
              {recent.length === 0 ? (
                <EmptyState compact mood="sleepy" title="Nothing on the clock yet" description="Start a timer from the toolbar, and your time shows up here." />
              ) : recent.map(entry => (
                <div key={entry.id} className="list-row [--inset:40px] flex items-center gap-3 px-4 h-[54px]">
                  <span className="w-3 h-3 rounded-full shrink-0" style={{ backgroundColor: entry.project_color }} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[14.5px] font-bold text-fg truncate">{entry.description || entry.project_name}</div>
                    <div className="text-[12.5px] font-semibold text-fg-3 truncate">{entry.description ? `${entry.project_name} · ${entry.client_name}` : entry.client_name}</div>
                  </div>
                  <BillingChip state={entry.billing_state} invoiceId={entry.invoice_id} invoiceNumber={entry.invoice_number} />
                  <div className="text-right w-[76px] shrink-0">
                    <div className="font-figures text-[15px] text-fg">{formatDurationShort(entry.duration_minutes)}</div>
                    <div className="text-[11.5px] font-semibold text-fg-4">{formatRelative(entry.start_time)}</div>
                  </div>
                </div>
              ))}
            </div>
          </section>
        </div>

        <div className="space-y-7">
          {/* This week */}
          <section>
            <div className="group-head">
              <h2 className="section-title">This week</h2>
              <span className="font-figures text-[16px] text-fg-3">{formatDurationShort(week.total)}</span>
            </div>
            <div className="card px-4 pt-4 pb-3">
              <div className="grid grid-cols-7 gap-2 h-[108px] items-end">
                {week.days.map((d, i) => {
                  const total = week.totals[i]
                  const dayName = parseLocalDate(d).toLocaleDateString('en-US', { weekday: 'long' })
                  const tip = total
                    ? `${dayName}: ${formatDurationShort(total)}\n${week.segments[i].map(sg => `${sg.name} ${formatDurationShort(sg.minutes)}`).join('\n')}`
                    : `${dayName}: nothing tracked`
                  return (
                    <div key={d} className="h-full flex items-end justify-center" title={tip}>
                      {total > 0 ? (
                        <div
                          className="grow-up w-full max-w-[28px] flex flex-col-reverse gap-[3px] rounded-[9px] overflow-hidden"
                          style={{ height: `${Math.max(9, (total / week.max) * 100)}%`, animationDelay: `${150 + i * 55}ms` }}
                        >
                          {week.segments[i].map(sg => (
                            <div key={sg.name} style={{ flex: `${sg.minutes} 1 0px`, backgroundColor: sg.color }} />
                          ))}
                        </div>
                      ) : (
                        <div className="w-full max-w-[28px] h-[8px] rounded-full bg-line" />
                      )}
                    </div>
                  )
                })}
              </div>
              <div className="grid grid-cols-7 gap-2 mt-2">
                {week.days.map(d => (
                  <div key={d} className={`text-center text-[12px] font-bold ${d === today ? 'text-accent' : 'text-fg-4'}`}>
                    {parseLocalDate(d).toLocaleDateString('en-US', { weekday: 'narrow' })}
                  </div>
                ))}
              </div>
              <div className="grid grid-cols-2 gap-3 mt-3 pt-3 border-t-2 border-line">
                <div>
                  <div className="text-[12px] font-semibold text-fg-3">Best day</div>
                  <div className="text-[14px] font-bold text-fg">
                    {week.total > 0
                      ? `${parseLocalDate(week.days[week.totals.indexOf(Math.max(...week.totals))]).toLocaleDateString('en-US', { weekday: 'short' })} · ${formatDurationShort(Math.max(...week.totals))}`
                      : '—'}
                  </div>
                </div>
                <div>
                  <div className="text-[12px] font-semibold text-fg-3">{monthName}</div>
                  <div className="text-[14px] font-bold text-fg">{formatHoursShort(stats.hours_this_month)}</div>
                </div>
              </div>
            </div>
          </section>

          {/* Start a timer */}
          <section>
            <div className="group-head">
              <h2 className="section-title">Start a timer</h2>
              <button onClick={() => navigate('/projects')} className="head-link">Projects <ChevronRight /></button>
            </div>
            <div className="card overflow-hidden">
              {quickProjects.length === 0 ? (
                <div className="px-4 py-3 text-[14px] font-semibold text-fg-3">
                  No active projects. <button onClick={() => navigate('/projects?action=new')} className="link">Create one</button>
                </div>
              ) : quickProjects.map(project => {
                const isCurrent = (isTimerRunning || isTimerPaused) && activeEntry?.project_id === project.id
                return (
                  <div
                    key={project.id}
                    className={`list-row [--inset:40px] flex items-center gap-3 pl-4 pr-3 h-[56px] ${isCurrent ? 'bg-accent/10' : ''}`}
                  >
                    <span className="w-3 h-3 rounded-full shrink-0" style={{ backgroundColor: project.color }} />
                    <div className="min-w-0 flex-1">
                      <div className="text-[14px] font-bold text-fg truncate">{project.name}</div>
                      <div className="text-[12px] font-semibold text-fg-3 truncate">{project.client_name}</div>
                    </div>
                    <button
                      onClick={() => (isCurrent ? (isTimerPaused ? onResumeTimer() : onPauseTimer()) : onStartTimer(project.id))}
                      className="w-9 h-9 shrink-0 rounded-full bg-accent text-white flex items-center justify-center hover:scale-110 active:scale-95 transition-transform"
                      style={{ boxShadow: '0 3px 10px -3px rgb(255 140 10 / 0.6)' }}
                      title={isCurrent ? (isTimerPaused ? 'Resume' : 'Pause') : `Start a timer for ${project.name}`}
                      aria-label={isCurrent ? (isTimerPaused ? 'Resume timer' : 'Pause timer') : `Start a timer for ${project.name}`}
                    >
                      {isCurrent && !isTimerPaused
                        ? <Pause className="w-4 h-4 fill-current" />
                        : <Play className="w-4 h-4 fill-current ml-0.5" />}
                    </button>
                  </div>
                )
              })}
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}
