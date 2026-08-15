import { motion } from 'framer-motion'
import { Pause, Play, Square } from 'lucide-react'
import type { TimeEntry } from '@shared/types'

interface TimerBarProps {
  entry: TimeEntry | null
  isRunning: boolean
  isPaused: boolean
  elapsed: string
  onPause: () => Promise<unknown>
  onResume: () => Promise<unknown>
  onStop: () => Promise<unknown>
}

export default function TimerBar({
  entry,
  isRunning,
  isPaused,
  elapsed,
  onPause,
  onResume,
  onStop,
}: TimerBarProps) {
  return (
    <div className="drag-region h-[52px] flex-shrink-0 flex items-center justify-end px-6">
      {entry && (
        <motion.div
          initial={{ opacity: 0, y: -4 }}
          animate={{ opacity: 1, y: 0 }}
          className="no-drag flex items-center gap-3 rounded-lg border border-rim/[0.06] bg-surface-100/80 px-3 py-1.5 shadow-sm"
        >
          <div className="flex items-center gap-2 min-w-0">
            <span className={`w-2 h-2 rounded-full flex-shrink-0 ${isPaused ? 'bg-status-paused' : 'bg-accent'}`} />
            <span className="text-xs font-medium text-text-primary max-w-[180px] truncate">
              {entry.project_name || 'Current project'}
            </span>
            <span className={`text-[10px] uppercase tracking-wider ${isPaused ? 'text-status-paused' : 'text-accent'}`}>
              {isPaused ? 'Paused' : 'Recording'}
            </span>
            <span className="font-mono text-xs text-text-secondary tabular-nums">{elapsed}</span>
          </div>
          <div className="w-px h-4 bg-rim/[0.08]" />
          {isPaused ? (
            <button
              onClick={onResume}
              className="p-1.5 rounded-md text-accent hover:bg-accent/10 transition-colors"
              title="Resume timer"
              aria-label="Resume timer"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
            </button>
          ) : isRunning ? (
            <button
              onClick={onPause}
              className="p-1.5 rounded-md text-status-paused hover:bg-status-paused/10 transition-colors"
              title="Pause timer"
              aria-label="Pause timer"
            >
              <Pause className="w-3.5 h-3.5 fill-current" />
            </button>
          ) : null}
          <button
            onClick={onStop}
            className="p-1.5 rounded-md text-red-400 hover:bg-red-500/10 transition-colors"
            title="Stop timer"
            aria-label="Stop timer"
          >
            <Square className="w-3.5 h-3.5 fill-current" />
          </button>
        </motion.div>
      )}
    </div>
  )
}
