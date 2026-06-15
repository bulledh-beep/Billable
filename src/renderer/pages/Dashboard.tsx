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
  CheckCircle2,
  AlertTriangle,
} from 'lucide-react'
import AnimatedNumber from '../components/AnimatedNumber'
import { formatMoney, formatHours, formatRelative, formatDuration, getAvatarColor, getInitials } from '../utils/format'
import type { DashboardStats, TimeEntry, Project } from '@shared/types'

interface DashboardProps {
  onStartTimer: (projectId: number, description?: string, isBillable?: number) => Promise<any>
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
      label: 'Outstanding Invoices',
      value: stats.outstanding_invoices,
      format: (v: number) => formatMoney(v),
      icon: DollarSign,
      color: 'text-yellow-400',
      bg: 'bg-yellow-500/10',
    },
    {
      label: 'Income This Month',
      value: stats.paid_income_this_month,
      format: (v: number) => formatMoney(v),
      icon: TrendingUp,
      color: 'text-status-complete',
      bg: 'bg-status-complete/10',
    },
    {
      label: 'Expenses This Month',
      value: stats.expenses_this_month + (stats.bills_paid_this_month_total || 0),
      format: (v: number) => formatMoney(v),
      icon: DollarSign,
      color: 'text-status-overdue',
      bg: 'bg-status-overdue/10',
    },
    {
      label: 'Safe-to-Spend Balance',
      value: stats.safe_to_spend,
      format: (v: number) => formatMoney(v),
      icon: CheckCircle2,
      color: 'text-accent',
      bg: 'bg-accent/10',
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
      <motion.div variants={item} className="mb-6 flex justify-between items-center">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Dashboard</h1>
          <p className="text-sm text-text-secondary mt-1">
            Welcome back. Here&apos;s your financial overview.
          </p>
        </div>
        <button
          onClick={() => navigate('/billing')}
          className="btn-primary text-xs flex items-center gap-2"
        >
          Open Billing
        </button>
      </motion.div>

      {/* Safe-to-Spend Banner */}
      {stats && (
        <motion.div variants={item} className="glass-panel p-6 mb-6 bg-gradient-to-r from-accent/[0.04] to-transparent border border-accent/10">
          <div className="flex flex-col xl:flex-row justify-between items-start xl:items-center gap-4">
            <div>
              <span className="text-[10px] uppercase font-bold tracking-wider text-accent">Safe-To-Spend Balance</span>
              <div className="text-3xl font-bold font-mono text-text-primary mt-1">
                {formatMoney(stats.safe_to_spend)}
              </div>
              <p className="text-xs text-text-tertiary mt-1">
                Formula: Realized Income - Paid Expenses - Pending Bills (30d) - Estimated Tax Set-Aside ({stats.tax_bracket_rate}%)
              </p>
            </div>
            
            {/* The Formula Breakdown */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 p-3.5 rounded-xl bg-surface-200 border border-rim/6 text-xs w-full xl:w-auto">
              <div>
                <div className="text-text-tertiary text-[10px]">Realized Income</div>
                <div className="font-semibold font-mono text-green-400 mt-0.5">{formatMoney(stats.paid_income_this_month)}</div>
              </div>
              <div>
                <div className="text-text-tertiary text-[10px]">Paid Expenses</div>
                <div className="font-semibold font-mono text-red-400 mt-0.5">-{formatMoney(stats.expenses_this_month)}</div>
              </div>
              <div>
                <div className="text-text-tertiary text-[10px]">Bills (30d)</div>
                <div className="font-semibold font-mono text-yellow-400 mt-0.5">-{formatMoney(stats.bills_due_in_next_30_days)}</div>
              </div>
              <div>
                <div className="text-text-tertiary text-[10px]">Tax Set-Aside ({stats.tax_bracket_rate}%)</div>
                <div className="font-semibold font-mono text-purple-400 mt-0.5">-{formatMoney(stats.estimated_tax_set_aside)}</div>
              </div>
            </div>
          </div>
        </motion.div>
      )}

      {/* Stat Cards */}
      <div className="grid grid-cols-5 gap-4 mb-6">
        {statCards.map((card, i) => (
          <motion.div
            key={card.label}
            variants={item}
            className="glass-panel p-4 relative overflow-hidden"
          >
            <div className="flex items-center justify-between mb-3">
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
        {/* Left Side: Recent Activity */}
        <div className="col-span-2 space-y-6">
          {/* Recent Activity */}
          <motion.div variants={item} className="glass-panel p-5">
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
                    className="flex items-center gap-3 py-2 px-3 rounded-lg hover:bg-surface-200/50 transition-colors"
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
        </div>

        {/* Right Side: Quick Start & Monthly Expenses */}
        <div className="space-y-6">
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
                {projects.slice(0, 5).map((project: any) => {
                  const isActive = isTimerRunning && activeEntry?.project_id === project.id
                  return (
                    <button
                      key={project.id}
                      onClick={() => isActive ? handleQuickStop() : handleQuickStart(project.id)}
                      className={`w-full flex items-center gap-3 py-2 px-3 rounded-lg transition-colors text-left group ${
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

          {/* Monthly Expenses by Category */}
          <motion.div variants={item} className="glass-panel p-5">
            <h2 className="text-sm font-semibold text-text-primary mb-4">Expenses This Month</h2>

            {!stats?.expenses_by_category || stats.expenses_by_category.length === 0 ? (
              <div className="text-center py-8">
                <DollarSign className="w-8 h-8 text-text-tertiary mx-auto mb-2" />
                <p className="text-xs text-text-secondary">No expenses logged this month.</p>
              </div>
            ) : (
              <div className="space-y-3">
                {stats.expenses_by_category.map((cat: any) => (
                  <div key={cat.category}>
                    <div className="flex justify-between text-xs mb-1">
                      <span className="capitalize text-text-secondary">{cat.category.replace('_', ' ')}</span>
                      <span className="font-semibold font-mono text-text-primary">{formatMoney(cat.total)}</span>
                    </div>
                    <div className="w-full bg-surface-200 h-1 rounded-full overflow-hidden">
                      <div
                        className="bg-accent h-full rounded-full"
                        style={{
                          width: `${Math.min(100, (cat.total / (stats.expenses_this_month || 1)) * 100)}%`,
                        }}
                      />
                    </div>
                  </div>
                ))}
                <div className="pt-2 border-t border-rim/6 flex justify-between items-center text-xs">
                  <span className="font-medium text-text-secondary">Total Monthly Expenses</span>
                  <span className="font-bold font-mono text-red-400">{formatMoney(stats.expenses_this_month)}</span>
                </div>
              </div>
            )}
          </motion.div>
        </div>
      </div>
    </motion.div>
  )
}
