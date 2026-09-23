import { useState, useEffect, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { Play, Pause, ChevronRight, Clock, CheckCircle2 } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import { BillingChip } from '../components/StatusBadge'
import EmptyState from '../components/EmptyState'
import {
  formatMoney, formatDurationShort, formatHoursShort, formatRelative, todayISO, addDays, toLocalISODate, parseLocalDate,
} from '../utils/format'
import { onBillingChanged } from '../utils/events'
import { ATTENTION_ICON, attentionColor } from '../utils/attention'
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

  // This week's time per day, split by project so each bar shows where it went
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
      const seg = day.get(e.project_id) || { name: e.project_name || 'Project', color: e.project_color || '#8E8E93', minutes: 0 }
      seg.minutes += e.duration_minutes
      day.set(e.project_id, seg)
    }
    const segments = days.map(d => Array.from(byDay.get(d)?.values() || []).sort((a, b) => b.minutes - a.minutes))
    const totals = segments.map(list => list.reduce((s, x) => s + x.minutes, 0))
    return { days, segments, totals, max: Math.max(60, ...totals), total: totals.reduce((a, b) => a + b, 0) }
  }, [entries, today])

  const recent = useMemo(() => entries.filter(e => e.end_time).slice(0, 7), [entries])
  const quickProjects = useMemo(() => [...projects]
    .sort((a, b) => String(b.last_activity || b.created_at).localeCompare(String(a.last_activity || a.created_at)))
    .slice(0, 6), [projects])

  if (!stats || !billing) return null
  const { pipeline } = billing
  const attention = billing.attention.slice(0, 4)
  const dateLine = new Date().toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric' })
  const monthName = new Date().toLocaleDateString('en-US', { month: 'long' })

  return (
    <div className="page">
      <PageHeader title="Dashboard" subtitle={dateLine} />

      <MetricStrip className="mb-7">
        <Metric
          label="Ready to bill"
          value={<Money amount={pipeline.unbilled_amount} />}
          sub={pipeline.unbilled_hours > 0 ? `${formatHoursShort(pipeline.unbilled_hours)} not on an invoice` : 'All caught up'}
          onClick={() => navigate('/billing')}
        />
        <Metric
          label="Awaiting payment"
          value={<Money amount={pipeline.awaiting_amount + pipeline.draft_amount} />}
          sub={pipeline.draft_count ? `${pipeline.awaiting_count} sent · ${pipeline.draft_count} draft` : `${pipeline.awaiting_count} invoice${pipeline.awaiting_count === 1 ? '' : 's'} sent`}
          onClick={() => navigate('/billing?status=sent')}
        />
        <Metric
          label="Overdue"
          value={<Money amount={pipeline.overdue_amount} className={pipeline.overdue_amount > 0 ? 'text-red' : ''} />}
          sub={pipeline.overdue_count ? `${pipeline.overdue_count} past due` : 'Nothing late'}
          onClick={() => navigate('/billing?status=overdue')}
        />
        <Metric
          label={`Paid in ${today.slice(0, 4)}`}
          value={<Money amount={pipeline.paid_ytd_amount} />}
          sub={`${pipeline.paid_ytd_count} invoice${pipeline.paid_ytd_count === 1 ? '' : 's'}`}
          onClick={() => navigate('/billing?status=paid')}
        />
      </MetricStrip>

      <div className="grid grid-cols-[minmax(0,1fr)_300px] gap-6 items-start">
        <div className="space-y-6 min-w-0">
          {/* Needs attention */}
          <section>
            <div className="group-head">
              <h2 className="section-title">
                Needs attention
                {billing.attention.length > 0 && <span className="ml-1.5 font-normal text-fg-3 num">{billing.attention.length}</span>}
              </h2>
              {billing.attention.length > 0 && (
                <button onClick={() => navigate('/billing')} className="head-link">Open Billing <ChevronRight /></button>
              )}
            </div>
            <div className="card overflow-hidden">
              {attention.length === 0 ? (
                <div className="flex items-center gap-2.5 px-4 h-[46px]">
                  <CheckCircle2 className="w-4 h-4 text-green" strokeWidth={1.75} />
                  <span className="text-[13px] text-fg-2">Nothing is overdue or waiting to be billed.</span>
                </div>
              ) : attention.map(item => {
                const Icon = ATTENTION_ICON[item.kind]
                const to = item.invoice_id ? `/invoices/${item.invoice_id}` : item.project_id ? `/projects/${item.project_id}` : '/billing'
                return (
                  <button
                    key={item.key}
                    onClick={() => navigate(to)}
                    className="list-row [--inset:44px] group w-full flex items-center gap-3 px-4 h-[48px] hover:bg-fg/[0.025] text-left"
                  >
                    <Icon className={`w-4 h-4 shrink-0 ${attentionColor(item.tone)}`} strokeWidth={1.75} />
                    <div className="min-w-0 flex-1">
                      <div className="text-[13px] font-medium text-fg truncate">{item.title}</div>
                      <div className="text-xs text-fg-3 truncate">{item.detail}</div>
                    </div>
                    {item.amount != null && <span className="text-[13px] num text-fg-2">{formatMoney(item.amount)}</span>}
                    <ChevronRight className="w-3.5 h-3.5 text-fg-4 group-hover:text-fg-3" />
                  </button>
                )
              })}
              {billing.attention.length > attention.length && (
                <button onClick={() => navigate('/billing')} className="w-full px-4 h-[34px] text-xs text-fg-3 hover:text-fg border-t border-line text-left">
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
                <EmptyState compact icon={Clock} title="No time tracked yet" description="Start a timer from the toolbar to begin." />
              ) : recent.map(entry => (
                <div key={entry.id} className="list-row [--inset:36px] flex items-center gap-3 px-4 h-[44px]">
                  <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: entry.project_color }} />
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] text-fg truncate">{entry.description || entry.project_name}</div>
                    <div className="text-xs text-fg-3 truncate">{entry.description ? `${entry.project_name} · ${entry.client_name}` : entry.client_name}</div>
                  </div>
                  <BillingChip state={entry.billing_state} invoiceId={entry.invoice_id} invoiceNumber={entry.invoice_number} />
                  <div className="text-right w-[72px] shrink-0">
                    <div className="text-[13px] num font-medium text-fg">{formatDurationShort(entry.duration_minutes)}</div>
                    <div className="text-2xs text-fg-3">{formatRelative(entry.start_time)}</div>
                  </div>
                </div>
              ))}
            </div>
          </section>
        </div>

        <div className="space-y-6">
          {/* This week */}
          <section>
            <div className="group-head">
              <h2 className="section-title">This week</h2>
              <span className="text-xs font-medium text-fg-2 num">{formatDurationShort(week.total)}</span>
            </div>
            <div className="card px-4 pt-4 pb-3">
              <div className="grid grid-cols-7 gap-2 h-[92px] items-end">
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
                          className="w-full max-w-[24px] flex flex-col-reverse gap-px rounded-[4px] overflow-hidden"
                          style={{ height: `${Math.max(7, (total / week.max) * 100)}%` }}
                        >
                          {week.segments[i].map(sg => (
                            <div key={sg.name} style={{ flex: `${sg.minutes} 1 0px`, backgroundColor: sg.color }} />
                          ))}
                        </div>
                      ) : (
                        <div className="w-full max-w-[24px] h-[3px] rounded-full bg-fg/[0.08]" />
                      )}
                    </div>
                  )
                })}
              </div>
              <div className="grid grid-cols-7 gap-2 mt-2">
                {week.days.map(d => (
                  <div key={d} className={`text-center text-2xs ${d === today ? 'text-fg font-semibold' : 'text-fg-4'}`}>
                    {parseLocalDate(d).toLocaleDateString('en-US', { weekday: 'narrow' })}
                  </div>
                ))}
              </div>
              <div className="grid grid-cols-2 gap-3 mt-3 pt-3 border-t border-line">
                <div>
                  <div className="text-2xs text-fg-3">Today</div>
                  <div className="text-[13px] font-semibold text-fg num">{formatHoursShort(stats.hours_today)}</div>
                </div>
                <div>
                  <div className="text-2xs text-fg-3">{monthName}</div>
                  <div className="text-[13px] font-semibold text-fg num">{formatHoursShort(stats.hours_this_month)}</div>
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
                <div className="px-4 py-3 text-[13px] text-fg-3">
                  No active projects. <button onClick={() => navigate('/projects?action=new')} className="link">Create one</button>
                </div>
              ) : quickProjects.map(project => {
                const isCurrent = (isTimerRunning || isTimerPaused) && activeEntry?.project_id === project.id
                return (
                  <button
                    key={project.id}
                    onClick={() => (isCurrent ? (isTimerPaused ? onResumeTimer() : onPauseTimer()) : onStartTimer(project.id))}
                    className={`list-row [--inset:34px] group w-full flex items-center gap-2.5 px-4 h-[44px] text-left transition-colors ${
                      isCurrent ? 'bg-accent/[0.1]' : 'hover:bg-fg/[0.025]'
                    }`}
                    title={isCurrent ? (isTimerPaused ? 'Resume' : 'Pause') : `Start a timer for ${project.name}`}
                  >
                    <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: project.color }} />
                    <div className="min-w-0 flex-1">
                      <div className="text-[13px] text-fg truncate">{project.name}</div>
                      <div className="text-2xs text-fg-3 truncate">{project.client_name}</div>
                    </div>
                    {isCurrent ? (
                      isTimerPaused
                        ? <Play className="w-3 h-3 text-accent-text fill-current" />
                        : <Pause className="w-3 h-3 text-accent-text fill-current" />
                    ) : (
                      <Play className="w-3 h-3 text-accent fill-current opacity-0 group-hover:opacity-100 transition-opacity" />
                    )}
                  </button>
                )
              })}
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}
