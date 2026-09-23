import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Pause, Play, Square, Search } from 'lucide-react'
import { Popover } from './Menu'
import { useElapsed } from '../hooks/useTimer'
import type { Project, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

interface TimerControlProps {
  entry: TimeEntry | null
  isRunning: boolean
  isPaused: boolean
  onPause: () => Promise<unknown>
  onResume: () => Promise<unknown>
  onStop: () => Promise<any>
  onStart: (projectId: number, description?: string) => Promise<unknown>
}

/** Timer that lives in the window toolbar on every page. */
export default function TimerControl({ entry, isRunning, isPaused, onPause, onResume, onStop, onStart }: TimerControlProps) {
  const navigate = useNavigate()
  const elapsed = useElapsed(entry)
  const [open, setOpen] = useState(false)
  const [anchor, setAnchor] = useState<HTMLElement | null>(null)

  const stop = async () => {
    const result = await onStop()
    if (result?.discarded) toast('Timer ran under a minute, so it was discarded')
    else if (result) toast.success('Timer stopped')
  }

  if (entry) {
    return (
      <div className="no-drag flex items-center h-[28px] pl-2.5 pr-0.5 gap-2 rounded-[7px] border border-line-strong bg-panel dark:bg-fg/[0.07] dark:border-transparent">
        <span
          className={`w-[7px] h-[7px] rounded-full shrink-0 ${isPaused ? 'bg-fg-4' : 'bg-accent animate-[timer-breathe_2s_ease-in-out_infinite]'}`}
          aria-hidden="true"
        />
        <button
          onClick={() => navigate('/time')}
          className="text-[13px] text-fg-2 hover:text-fg max-w-[180px] truncate"
          title="Open Time"
        >
          {entry.project_name || 'Timer'}
        </button>
        <span className={`num text-[13px] font-semibold tabular-nums ${isPaused ? 'text-fg-3' : 'text-fg'}`}>{elapsed}</span>
        <div className="w-px h-3.5 bg-line-strong mx-0.5" />
        {isPaused ? (
          <button onClick={onResume} className="btn-icon-sm text-accent-text" title="Resume (⌘⇧P)" aria-label="Resume timer">
            <Play className="fill-current !w-3 !h-3" />
          </button>
        ) : isRunning ? (
          <button onClick={onPause} className="btn-icon-sm" title="Pause (⌘⇧P)" aria-label="Pause timer">
            <Pause className="fill-current !w-3 !h-3" />
          </button>
        ) : null}
        <button onClick={stop} className="btn-icon-sm hover:!text-red" title="Stop (⌘⇧S)" aria-label="Stop timer">
          <Square className="fill-current !w-[11px] !h-[11px]" />
        </button>
      </div>
    )
  }

  return (
    <>
      <button
        ref={setAnchor}
        onClick={() => setOpen(o => !o)}
        className={`no-drag btn-secondary ${open ? '!bg-panel-2 dark:!bg-fg/[0.13]' : ''}`}
        title="Start a timer"
      >
        <Play className="!w-3 !h-3 fill-current text-accent" />
        Start timer
      </button>
      <Popover open={open} anchor={anchor} onClose={() => setOpen(false)} width={320}>
        {open && (
          <QuickStart
            onStart={async (pid, desc) => {
              setOpen(false)
              await onStart(pid, desc)
            }}
          />
        )}
      </Popover>
    </>
  )
}

function QuickStart({ onStart }: { onStart: (projectId: number, description: string) => void }) {
  const [projects, setProjects] = useState<Project[]>([])
  const [query, setQuery] = useState('')
  const [description, setDescription] = useState('')
  const [index, setIndex] = useState(0)
  const searchRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    window.api.projects.list().then((list: Project[]) => {
      const active = list.filter(p => p.status === 'active')
      active.sort((a, b) => String(b.last_activity || b.created_at).localeCompare(String(a.last_activity || a.created_at)))
      setProjects(active)
    })
    setTimeout(() => searchRef.current?.focus(), 30)
  }, [])

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase()
    return q
      ? projects.filter(p => p.name.toLowerCase().includes(q) || (p.client_name || '').toLowerCase().includes(q))
      : projects
  }, [projects, query])

  useEffect(() => { setIndex(0) }, [query])

  const choose = (p?: Project) => { if (p) onStart(p.id, description.trim()) }

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') { e.preventDefault(); setIndex(i => Math.min(i + 1, filtered.length - 1)) }
    if (e.key === 'ArrowUp') { e.preventDefault(); setIndex(i => Math.max(i - 1, 0)) }
    if (e.key === 'Enter') { e.preventDefault(); choose(filtered[index]) }
  }

  return (
    <div onKeyDown={onKey}>
      <div className="p-2 border-b border-line space-y-1.5">
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-[13px] h-[13px] text-fg-4" />
          <input
            ref={searchRef}
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Find a project"
            className="input pl-8"
          />
        </div>
        <input
          value={description}
          onChange={e => setDescription(e.target.value)}
          placeholder="What are you working on? (optional)"
          className="input"
        />
      </div>
      <div className="max-h-[280px] overflow-y-auto p-[5px]">
        {filtered.length === 0 && (
          <p className="px-3 py-6 text-center text-sm text-fg-3">
            {projects.length === 0 ? 'No active projects yet' : 'No projects match'}
          </p>
        )}
        {filtered.map((p, i) => (
          <button
            key={p.id}
            onMouseEnter={() => setIndex(i)}
            onClick={() => choose(p)}
            className={`w-full flex items-center gap-2.5 px-2 h-[30px] rounded-[5px] text-left ${i === index ? 'bg-fg/[0.07]' : ''}`}
          >
            <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: p.color }} />
            <span className="text-[13px] text-fg truncate flex-1">{p.name}</span>
            <span className="text-xs text-fg-3 truncate max-w-[110px]">{p.client_name}</span>
          </button>
        ))}
      </div>
      <div className="px-3 h-[30px] flex items-center gap-1.5 border-t border-line text-2xs text-fg-3">
        <span className="kbd">↑↓</span> choose <span className="kbd ml-1.5">↵</span> start
      </div>
    </div>
  )
}
