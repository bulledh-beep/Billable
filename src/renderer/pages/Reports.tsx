import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { BarChart3, Download } from 'lucide-react'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Cell,
} from 'recharts'
import { formatMoney, formatHours } from '../utils/format'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.06 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

const CHART_COLORS = ['#F5A623', '#3498DB', '#2ECC71', '#E74C3C', '#9B59B6', '#1ABC9C', '#E67E22', '#EC407A']

interface ReportsProps {
  isTimerRunning: boolean
}

export default function Reports({ isTimerRunning }: ReportsProps) {
  const [dateRange, setDateRange] = useState({
    start: new Date(new Date().getFullYear(), 0, 1).toISOString().split('T')[0],
    end: new Date().toISOString().split('T')[0],
  })
  const [hoursByProject, setHoursByProject] = useState<any[]>([])
  const [hoursByClient, setHoursByClient] = useState<any[]>([])
  const [earningsByMonth, setEarningsByMonth] = useState<any[]>([])
  const [activeTab, setActiveTab] = useState<'projects' | 'clients' | 'earnings'>('projects')

  useEffect(() => { loadReports() }, [dateRange])

  // Refresh whenever the timer starts/stops so new entries & projects appear in totals
  useEffect(() => { loadReports() }, [isTimerRunning])

  // Refresh on window focus so navigating back from another app / page picks up changes
  useEffect(() => {
    const onFocus = () => loadReports()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [dateRange])

  const loadReports = async () => {
    const [byProject, byClient, byMonth] = await Promise.all([
      window.api.reports.hoursByProject(dateRange.start, dateRange.end),
      window.api.reports.hoursByClient(dateRange.start, dateRange.end),
      window.api.reports.earningsByMonth(dateRange.start, dateRange.end),
    ])
    setHoursByProject(byProject)
    setHoursByClient(byClient)
    setEarningsByMonth(byMonth)
  }

  const handleExport = async () => {
    let data: any[]
    let filename: string
    if (activeTab === 'projects') {
      data = hoursByProject
      filename = 'hours-by-project.csv'
    } else if (activeTab === 'clients') {
      data = hoursByClient
      filename = 'hours-by-client.csv'
    } else {
      data = earningsByMonth
      filename = 'earnings-by-month.csv'
    }

    if (data.length === 0) return toast.error('No data to export')
    const result = await window.api.reports.exportCSV(data, filename)
    if (result) toast.success('CSV exported')
  }

  const totalHours = hoursByProject.reduce((sum, p) => sum + p.hours, 0)

  const customTooltip = ({ active, payload, label }: any) => {
    if (!active || !payload?.length) return null
    return (
      <div className="bg-surface-200 border border-rim/[0.06] rounded-lg px-3 py-2 shadow-lg">
        <p className="text-xs text-text-secondary mb-1">{label || payload[0]?.payload?.name}</p>
        <p className="text-sm font-mono text-text-primary font-medium">
          {activeTab === 'earnings' ? formatMoney(payload[0].value) : `${formatHours(payload[0].value)} hours`}
        </p>
      </div>
    )
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Reports</h1>
          <p className="text-sm text-text-secondary mt-1">
            {formatHours(totalHours)} total hours tracked
          </p>
        </div>
        <button onClick={handleExport} className="btn-secondary flex items-center gap-2">
          <Download className="w-4 h-4" /> Export CSV
        </button>
      </motion.div>

      {/* Date Range */}
      <motion.div variants={item} className="flex gap-4 items-center mb-6">
        <div className="flex items-center gap-2">
          <label className="text-xs text-text-tertiary">From</label>
          <input
            type="date"
            value={dateRange.start}
            onChange={e => setDateRange(d => ({ ...d, start: e.target.value }))}
            className="input-field w-40"
          />
        </div>
        <div className="flex items-center gap-2">
          <label className="text-xs text-text-tertiary">To</label>
          <input
            type="date"
            value={dateRange.end}
            onChange={e => setDateRange(d => ({ ...d, end: e.target.value }))}
            className="input-field w-40"
          />
        </div>
      </motion.div>

      {/* Tabs */}
      <motion.div variants={item} className="flex gap-1 bg-surface-100 rounded-lg p-0.5 border border-rim/[0.04] w-fit mb-6">
        {[
          { key: 'projects', label: 'Hours by Project' },
          { key: 'clients', label: 'Hours by Client' },
          { key: 'earnings', label: 'Earnings by Month' },
        ].map(tab => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key as any)}
            className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${
              activeTab === tab.key ? 'bg-surface-300 text-text-primary' : 'text-text-tertiary hover:text-text-secondary'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </motion.div>

      {/* Chart */}
      <motion.div variants={item} className="glass-panel p-6">
        {activeTab === 'projects' && (
          hoursByProject.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No data for this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={400}>
              <BarChart data={hoursByProject} layout="vertical" margin={{ left: 120 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis type="number" stroke="#6B6A67" fontSize={12} tickFormatter={v => `${v}h`} />
                <YAxis type="category" dataKey="name" stroke="#6B6A67" fontSize={12} width={110} />
                <Tooltip content={customTooltip} />
                <Bar dataKey="hours" radius={[0, 4, 4, 0]}>
                  {hoursByProject.map((entry, i) => (
                    <Cell key={i} fill={entry.color || CHART_COLORS[i % CHART_COLORS.length]} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          )
        )}

        {activeTab === 'clients' && (
          hoursByClient.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No data for this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={400}>
              <BarChart data={hoursByClient} layout="vertical" margin={{ left: 120 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis type="number" stroke="#6B6A67" fontSize={12} tickFormatter={v => `${v}h`} />
                <YAxis type="category" dataKey="name" stroke="#6B6A67" fontSize={12} width={110} />
                <Tooltip content={customTooltip} />
                <Bar dataKey="hours" radius={[0, 4, 4, 0]}>
                  {hoursByClient.map((_, i) => (
                    <Cell key={i} fill={CHART_COLORS[i % CHART_COLORS.length]} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          )
        )}

        {activeTab === 'earnings' && (
          earningsByMonth.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No data for this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={400}>
              <BarChart data={earningsByMonth}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis dataKey="month" stroke="#6B6A67" fontSize={12} />
                <YAxis stroke="#6B6A67" fontSize={12} tickFormatter={v => `$${v}`} />
                <Tooltip content={customTooltip} />
                <Bar dataKey="earnings" fill="#F5A623" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          )
        )}
      </motion.div>

      {/* Summary Table */}
      {activeTab === 'projects' && hoursByProject.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04]">
                <th className="text-left px-4 py-3 text-xs font-medium text-text-tertiary">Project</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">Hours</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-text-tertiary">% of Total</th>
              </tr>
            </thead>
            <tbody>
              {hoursByProject.map((p, i) => (
                <tr key={i} className="border-b border-rim/[0.02]">
                  <td className="px-4 py-3 text-sm text-text-primary flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full" style={{ backgroundColor: p.color || CHART_COLORS[i % CHART_COLORS.length] }} />
                    {p.name}
                  </td>
                  <td className="px-4 py-3 text-sm font-mono text-text-primary text-right">{formatHours(p.hours)}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-secondary text-right">
                    {totalHours > 0 ? ((p.hours / totalHours) * 100).toFixed(1) : 0}%
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </motion.div>
      )}
    </motion.div>
  )
}
