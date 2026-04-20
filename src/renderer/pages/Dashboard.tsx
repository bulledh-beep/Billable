import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import {
  Clock,
  DollarSign,
  TrendingUp,
  AlertCircle,
  Play,
  Square,
  ChevronRight,
} from 'lucide-react'
import AnimatedNumber from '../components/AnimatedNumber'
import { formatMoney, formatHours, formatRelative, formatDuration, getAvatarColor, getInitials } from '../utils/format'
import type { DashboardStats, TimeEntry, Project } from '@shared/types'

interface DashboardProps {
  onStartTimer: (projectId: number, description?: string) => Promise<any>
  onStopTimer: () => Promise<any>
  isTimerRunning: boolean
  activeEntry: TimeEntry | null
}

const container = {
  hidden: { opacity: 0 },
  show: { opacity: 1, transition: { staggerChildren: 0.06 } },
}

const item = {
  hidden: { opacity: 0, y: 15 },
  show: { opacity: 1, y: 0, transition: { duration: 0.35, ease: 'easeOut' } },
}

export default function Dashboard({ onStartTimer, onStopTimer, isTimerRunning, activeEntry }: DashboardProps) {
  const navigate = useNavigate()
  const [stats, setStats] = useState<DashboardStats | null>(null)
  const [recent, setRecent] = useState<TimeEntry[]>([])
  const [projects, setProjects] = useState<Project[]>([])

  useEffect(() => {
    loadData()
  }, [])

  // Reload when timer starts/stops so stats and recent activity stay current
  useEffect(() => { loadData() }, [isTimerRunning])

  const loadData = async () => {
    const [s, r, p] = await Promise.all([
      window.api.dashboard.stats(),
      window.api.dashboard.recent(),
      window.api.projects.list(),
    ])
    setStats(s)
    setRecent(r)
    setProjects(p.filter((p: any) => p.status === 'active'))
  }

  const handleQuickStart = async (projectId: number) => {
    await onStartTimer(projectId)
    loadData()
  }

  const handleQuickStop = async () => {
    await onStopTimer()
    loadData()
  }

  const statCards = stats ? [
    {
      label: 'Hours This Week',
      value: stats.hours_this_week,
      format: formatHours,
      icon: Clock,
      color: 'text-accent',
      bg: 'bg-accent/10',
    },
    {
      label: 'Hours This Month',
      value: stats.hours_this_month,
      format: formatHours,
      icon: TrendingUp,
      color: 'text-status-complete',
      bg: 'bg-status-complete/10',
    },
    {
      label: 'Unbilled Hours',
      value: stats.unbilled_hours,
      format: formatHours,
      icon: AlertCircle,
      color: 'text-status-paused',
      bg: 'bg-status-paused/10',
    },
    {
      label: 'Outstanding',
      value: stats.outstanding_total,
      format: (v: number) => formatMoney(v),
      icon: DollarSign,
      color: 'text-status-overdue',
      bg: 'bg-status-overdue/10',
    },
    {
      label: 'Paid Total',
      value: stats.paid_total,
      format: (v: number) => formatMoney(v),
      icon: DollarSign,
      color: 'text-status-paid',
      bg: 'bg-status-paid/10',
    },
  ] : []

  return (
    <motion.div
      variants={container}
      initial="hidden"
      animate="show"
      className="p-8"
    >
      {/* Header */}
      <motion.div variants={item} className="mb-8">
        <h1 className="text-2xl font-bold text-text-primary">Dashboard</h1>
        <p className="text-sm text-text-secondary mt-1">
          Welcome back. Here&apos;s your overview.
        </p>
      </motion.div>

      {/* Stat Cards */}
      <div className="grid grid-cols-5 gap-4 mb-8">
        {statCards.map((card, i) => (
          <motion.div
            key={card.label}
            variants={item}
            className="glass-panel p-4"
          >
            <div className="flex items-center gap-2 mb-3">
              <div className={`w-8 h-8 rounded-lg ${card.bg} flex items-center justify-center`}>
                <card.icon className={`w-4 h-4 ${card.color}`} />
              </div>
            </div>
            <div className="font-mono text-xl font-semibold text-text-primary">
              <AnimatedNumber value={card.value} format={card.format} />
            </div>
            <div className="text-xs text-text-tertiary mt-1">{card.label}</div>
          </motion.div>
        ))}
      </div>

      <div className="grid grid-cols-3 gap-6">
        {/* Recent Activity */}
        <motion.div variants={item} className="col-span-2 glass-panel p-5">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-sm font-semibold text-text-primary">Recent Activity</h2>
            <button
              onClick={() => navigate('/time')}
              className="text-xs text-accent hover:text-accent-light transition-colors"
            >
              View all
            </button>
          </div>

          {recent.length === 0 ? (
            <div className="text-center py-8">
              <Clock className="w-8 h-8 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No time entries yet</p>
              <p className="text-xs text-text-tertiary mt-1">Start tracking to see your activity here</p>
            </div>
          ) : (
            <div className="space-y-1">
              {recent.map((entry: any) => (
                <div
                  key={entry.id}
                  className="flex items-center gap-3 py-2.5 px-3 rounded-lg hover:bg-surface-200/50 transition-colors"
                >
                  <div
                    className="w-2 h-2 rounded-full flex-shrink-0"
                    style={{ backgroundColor: entry.project_color }}
                  />
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-text-primary truncate">
                      {entry.description || entry.project_name}
                    </div>
                    <div className="text-xs text-text-tertiary">
                      {entry.project_name} · {entry.client_name}
                    </div>
                  </div>
                  <div className="text-right flex-shrink-0">
                    <div className="font-mono text-sm text-text-primary">
                      {formatDuration(entry.duration_minutes)}
                    </div>
                    <div className="text-xs text-text-tertiary">
                      {formatRelative(entry.start_time)}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </motion.div>

        {/* Quick Start */}
        <motion.div variants={item} className="glass-panel p-5">
          <h2 className="text-sm font-semibold text-text-primary mb-4">Quick Start</h2>

          {projects.length === 0 ? (
            <div className="text-center py-8">
              <Play className="w-8 h-8 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No active projects</p>
              <button
                onClick={() => navigate('/projects')}
                className="text-xs text-accent hover:text-accent-light transition-colors mt-2"
              >
                Create a project
              </button>
            </div>
          ) : (
            <div className="space-y-1">
              {projects.slice(0, 8).map((project: any) => {
                const isActive = isTimerRunning && activeEntry?.project_id === project.id
                return (
                  <button
                    key={project.id}
                    onClick={() => isActive ? handleQuickStop() : handleQuickStart(project.id)}
                    className={`w-full flex items-center gap-3 py-2.5 px-3 rounded-lg transition-colors text-left group ${
                      isActive ? 'bg-accent/10 hover:bg-red-500/10' : 'hover:bg-surface-200/50'
                    }`}
                  >
                    <div
                      className="w-3 h-3 rounded-full flex-shrink-0"
                      style={{ backgroundColor: project.color }}
                    />
                    <div className="flex-1 min-w-0">
                      <div className="text-sm text-text-primary truncate">{project.name}</div>
                      <div className="text-xs text-text-tertiary truncate">{project.client_name}</div>
                    </div>
                    {isActive ? (
                      <Square className="w-3.5 h-3.5 text-red-400 fill-current" />
                    ) : (
                      <Play className="w-3.5 h-3.5 text-text-tertiary group-hover:text-accent transition-colors" />
                    )}
                  </button>
                )
              })}
            </div>
          )}
        </motion.div>
      </div>
    </motion.div>
  )
}
