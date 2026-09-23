import { useEffect, useMemo, useState } from 'react'
import { formatDurationShort, formatMoney, formatTime, toLocalISODate } from '../utils/format'
import { elapsedSeconds } from '../hooks/useTimer'
import type { TimeEntry } from '@shared/types'

interface Block {
  key: string
  start: number // hours since midnight
  end: number
  color: string
  label: string
  running?: boolean
}

function hourLabel(h: number) {
  const hr = h % 24
  if (hr === 0) return '12 AM'
  if (hr === 12) return 'Noon'
  return hr < 12 ? `${hr} AM` : `${hr - 12} PM`
}

/**
 * Today on a ruler: when you worked, colored by project, with an amber
 * needle at the current time.
 */
export default function TodayRuler({ entries, activeEntry }: { entries: TimeEntry[]; activeEntry: TimeEntry | null }) {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 30_000)
    return () => clearInterval(id)
  }, [])

  const today = toLocalISODate(now)
  const midnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const hoursAt = (ms: number) => Math.min(24, Math.max(0, (ms - midnight) / 3_600_000))
  const nowH = hoursAt(now.getTime())

  const { blocks, minutes, earned } = useMemo(() => {
    const list: Block[] = []
    let mins = 0
    let money = 0
    for (const e of entries) {
      if (!e.end_time || toLocalISODate(new Date(e.start_time)) !== today) continue
      const start = hoursAt(new Date(e.start_time).getTime())
      const end = hoursAt(new Date(e.end_time).getTime())
      list.push({
        key: `e${e.id}`,
        start,
        end: end > start ? end : Math.min(24, start + e.duration_minutes / 60),
        color: e.project_color || '#8E8E93',
        label: `${e.project_name} · ${formatTime(e.start_time)} – ${formatTime(e.end_time)} · ${formatDurationShort(e.duration_minutes)}`,
      })
      mins += e.duration_minutes
      if (e.is_billable) money += (e.duration_minutes / 60) * (e.rate || 0)
    }
    if (activeEntry && toLocalISODate(new Date(activeEntry.start_time)) === today) {
      const start = hoursAt(new Date(activeEntry.start_time).getTime())
      const until = activeEntry.paused_at ? hoursAt(new Date(activeEntry.paused_at).getTime()) : nowH
      const secs = elapsedSeconds(activeEntry)
      list.push({
        key: 'running',
        start,
        end: Math.max(until, start + 1 / 60),
        color: activeEntry.project_color || '#8E8E93',
        label: `${activeEntry.project_name} · ${activeEntry.paused_at ? 'paused' : 'running'} since ${formatTime(activeEntry.start_time)}`,
        running: !activeEntry.paused_at,
      })
      mins += secs / 60
      if (activeEntry.is_billable !== 0) money += (secs / 3600) * (activeEntry.rate || 0)
    }
    return { blocks: list, minutes: mins, earned: money }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [entries, activeEntry, today, Math.floor(nowH * 60)])

  // Working hours by default, stretched to fit whatever was tracked
  const first = Math.min(8, ...blocks.map(b => Math.floor(b.start)))
  const last = Math.max(20, Math.ceil(nowH + 0.25), ...blocks.map(b => Math.ceil(b.end)))
  const span = last - first
  const pct = (h: number) => `${((h - first) / span) * 100}%`
  const hours = Array.from({ length: span + 1 }, (_, i) => first + i)

  return (
    <section>
      <div className="group-head">
        <h2 className="section-title">Today</h2>
        <span className="text-xs text-fg-3">
          {minutes > 0 ? (
            <>
              <span className="font-figures text-[14px] text-fg-2">{formatDurationShort(minutes)}</span>
              {earned > 0 && <> · <span className="font-figures text-[14px] text-fg-2">{formatMoney(earned)}</span> earned</>}
            </>
          ) : 'Nothing on the clock yet'}
        </span>
      </div>
      <div className="card px-4 pt-3.5 pb-2">
        <div className="relative h-[22px] rounded-[4px] bg-fg/[0.045]">
          {blocks.map(b => (
            <div
              key={b.key}
              title={b.label}
              className="absolute top-0 bottom-0 rounded-[4px]"
              style={{
                left: pct(b.start),
                width: `max(3px, ${((b.end - b.start) / span) * 100}%)`,
                backgroundColor: b.color,
                opacity: b.running ? 0.85 : 1,
              }}
            />
          ))}
          {/* Now */}
          <div className="absolute -top-[5px] -bottom-[5px] w-[1.5px] -ml-[0.75px] bg-accent" style={{ left: pct(nowH) }} title={`Now, ${formatTime(now.toISOString())}`}>
            <span className="absolute -top-[3px] left-1/2 -translate-x-1/2 w-[7px] h-[7px] rounded-full bg-accent" />
          </div>
        </div>
        {/* Ruler */}
        <div className="relative h-[20px] mt-1.5">
          {hours.map(h => (
            <span
              key={h}
              className={`absolute top-0 w-px ${h % 3 === 0 ? 'h-[6px] bg-fg/30' : 'h-[3px] bg-fg/20'}`}
              style={{ left: pct(h) }}
            />
          ))}
          {hours.slice(0, -1).map(h => (
            <span key={`half${h}`} className="absolute top-0 w-px h-[2px] bg-fg/[0.12]" style={{ left: pct(h + 0.5) }} />
          ))}
          {hours.filter(h => h % 3 === 0).map(h => (
            <span
              key={`label${h}`}
              className={`absolute top-[8px] text-[10px] leading-3 text-fg-4 whitespace-nowrap ${h === first ? '' : h === last ? '-translate-x-full' : '-translate-x-1/2'}`}
              style={{ left: pct(h) }}
            >
              {hourLabel(h)}
            </span>
          ))}
        </div>
      </div>
    </section>
  )
}
