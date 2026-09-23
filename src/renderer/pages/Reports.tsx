import { useState, useEffect, useMemo } from 'react'
import { BarChart3 } from 'lucide-react'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, Legend,
} from 'recharts'
import PageHeader from '../components/PageHeader'
import Segmented from '../components/Segmented'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import EmptyState from '../components/EmptyState'
import { formatMoney, formatMoneyCompact, formatHoursShort, todayISO, addDays, toLocalISODate, parseLocalDate } from '../utils/format'
import toast from 'react-hot-toast'

type Preset = 'week' | 'month' | 'last-month' | 'quarter' | 'year' | 'custom'
type Tab = 'projects' | 'clients' | 'income'

const FALLBACK_COLORS = ['#F5A623', '#3E8BF7', '#30A46C', '#E5484D', '#8E4EC6', '#12A594', '#F76B15', '#D6409F']
const SERIES_COLOR: Record<string, string> = { invoiced: '#3E8BF7', paid: '#30A46C' }

function rangeFor(preset: Preset): { start: string; end: string } {
  const today = todayISO()
  const d = parseLocalDate(today)
  const y = d.getFullYear()
  const m = d.getMonth()
  switch (preset) {
    case 'week': {
      const start = addDays(today, -((d.getDay() + 6) % 7))
      return { start, end: addDays(start, 6) }
    }
    case 'month':
      return { start: toLocalISODate(new Date(y, m, 1)), end: toLocalISODate(new Date(y, m + 1, 0)) }
    case 'last-month':
      return { start: toLocalISODate(new Date(y, m - 1, 1)), end: toLocalISODate(new Date(y, m, 0)) }
    case 'quarter': {
      const q = Math.floor(m / 3) * 3
      return { start: toLocalISODate(new Date(y, q, 1)), end: toLocalISODate(new Date(y, q + 3, 0)) }
    }
    default:
      return { start: `${y}-01-01`, end: `${y}-12-31` }
  }
}

interface Props {
  isTimerRunning: boolean
  isTimerPaused: boolean
}

export default function Reports({ isTimerRunning, isTimerPaused }: Props) {
  const [preset, setPreset] = useState<Preset>('year')
  const [range, setRange] = useState(rangeFor('year'))
  const [tab, setTab] = useState<Tab>('projects')
  const [byProject, setByProject] = useState<any[]>([])
  const [byClient, setByClient] = useState<any[]>([])
  const [byMonth, setByMonth] = useState<any[]>([])

  useEffect(() => { load() }, [range, isTimerRunning, isTimerPaused])
  useEffect(() => {
    const onFocus = () => load()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [range])

  const load = async () => {
    if (!range.start || !range.end || range.start > range.end) return
    const [p, c, m] = await Promise.all([
      window.api.reports.hoursByProject(range.start, range.end),
      window.api.reports.hoursByClient(range.start, range.end),
      window.api.reports.earningsByMonth(range.start, range.end),
    ])
    setByProject(p)
    setByClient(c)
    setByMonth(m)
  }

  const choosePreset = (p: Preset) => {
    setPreset(p)
    if (p !== 'custom') setRange(rangeFor(p))
  }

  const totals = useMemo(() => ({
    hours: byProject.reduce((s, r) => s + r.hours, 0),
    value: byProject.reduce((s, r) => s + (r.value || 0), 0),
    invoiced: byMonth.reduce((s, r) => s + (r.invoiced || 0), 0),
    paid: byMonth.reduce((s, r) => s + (r.paid || 0), 0),
  }), [byProject, byMonth])

  const monthSeries = useMemo(() => byMonth.map(r => ({
    ...r,
    label: parseLocalDate(`${r.month}-01`).toLocaleDateString('en-US', { month: 'short', year: '2-digit' }),
  })), [byMonth])

  const handleExport = async () => {
    const data = tab === 'projects'
      ? byProject.map(r => ({ project: r.name, client: r.client_name, hours: r.hours.toFixed(2), billable_value: (r.value || 0).toFixed(2) }))
      : tab === 'clients'
        ? byClient.map(r => ({ client: r.name, hours: r.hours.toFixed(2), billable_value: (r.value || 0).toFixed(2) }))
        : byMonth.map(r => ({ month: r.month, invoiced: (r.invoiced || 0).toFixed(2), paid: (r.paid || 0).toFixed(2) }))
    if (data.length === 0) return toast.error('Nothing to export for this range')
    const file = `${tab === 'income' ? 'income-by-month' : `hours-by-${tab === 'projects' ? 'project' : 'client'}`}-${range.start}-to-${range.end}.csv`
    const result = await window.api.reports.exportCSV(data, file)
    if (result) toast.success('CSV exported')
  }

  const axis = { stroke: 'rgb(var(--fg-4))', fontSize: 11, tickLine: false, axisLine: false }
  const tooltip = (fmt: (v: number) => string) => ({ active, payload, label }: any) => {
    if (!active || !payload?.length) return null
    return (
      <div className="rounded-[8px] bg-panel shadow-pop px-3 py-2 text-xs">
        <div className="text-fg-3 mb-1">{label || payload[0]?.payload?.name}</div>
        {payload.map((p: any) => (
          <div key={p.dataKey} className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full" style={{ backgroundColor: SERIES_COLOR[p.dataKey] || p.payload?.color || FALLBACK_COLORS[0] }} />
            <span className="text-fg-2">{p.name}</span>
            <span className="num font-medium text-fg ml-auto pl-3">{fmt(p.value)}</span>
          </div>
        ))}
      </div>
    )
  }

  const rows = tab === 'projects' ? byProject : byClient

  return (
    <div className="page">
      <PageHeader
        title="Reports"
        actions={<button onClick={handleExport} className="btn-secondary">Export CSV</button>}
      />

      <div className="flex items-center gap-3 mb-5 flex-wrap">
        <Segmented
          value={preset}
          onChange={choosePreset}
          options={[
            { value: 'week', label: 'This week' },
            { value: 'month', label: 'This month' },
            { value: 'last-month', label: 'Last month' },
            { value: 'quarter', label: 'This quarter' },
            { value: 'year', label: 'This year' },
            { value: 'custom', label: 'Custom' },
          ]}
        />
        <div className="flex items-center gap-2 ml-auto">
          <input
            type="date"
            className="input w-[150px]"
            value={range.start}
            onChange={e => { setPreset('custom'); setRange(r => ({ ...r, start: e.target.value })) }}
          />
          <span className="text-fg-4">–</span>
          <input
            type="date"
            className="input w-[150px]"
            value={range.end}
            onChange={e => { setPreset('custom'); setRange(r => ({ ...r, end: e.target.value })) }}
          />
        </div>
      </div>

      <MetricStrip className="mb-5">
        <Metric label="Hours tracked" value={formatHoursShort(totals.hours)} sub={`${byProject.length} project${byProject.length === 1 ? '' : 's'}`} />
        <Metric label="Billable value" value={<Money amount={totals.value} />} sub="Tracked time at project rates" />
        <Metric label="Invoiced" value={<Money amount={totals.invoiced} />} sub="Issued in this range" />
        <Metric label="Paid" value={<Money amount={totals.paid} />} sub="Received in this range" />
      </MetricStrip>

      <div className="flex items-center mb-3">
        <Segmented
          value={tab}
          onChange={setTab}
          options={[
            { value: 'projects', label: 'Time by project' },
            { value: 'clients', label: 'Time by client' },
            { value: 'income', label: 'Income by month' },
          ]}
        />
      </div>

      {tab !== 'income' ? (
        rows.length === 0 ? (
          <div className="card"><EmptyState icon={BarChart3} title="No time in this range" description="Pick a different range or track some time." /></div>
        ) : (
          <div className="card overflow-hidden">
            <table className="tbl">
              <thead>
                <tr>
                  <th>{tab === 'projects' ? 'Project' : 'Client'}</th>
                  <th className="text-right w-24">Hours</th>
                  <th className="w-[220px]">Share of time</th>
                  <th className="text-right w-28">Value</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r, i) => {
                  const share = totals.hours > 0 ? r.hours / totals.hours : 0
                  const topHours = rows[0]?.hours || 1
                  return (
                    <tr key={r.id ?? i}>
                      <td>
                        <div className="flex items-center gap-2.5 min-w-0">
                          <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: r.color || FALLBACK_COLORS[i % FALLBACK_COLORS.length] }} />
                          <div className="min-w-0">
                            <div className="text-[13px] text-fg truncate max-w-[360px]">{r.name}</div>
                            {r.client_name && <div className="text-xs text-fg-3 truncate">{r.client_name}</div>}
                          </div>
                        </div>
                      </td>
                      <td className="text-right num text-fg">{formatHoursShort(r.hours)}</td>
                      <td>
                        <div className="flex items-center gap-2.5">
                          <div className="flex-1 h-[6px] rounded-full bg-fg/[0.07] overflow-hidden">
                            <div className="h-full rounded-full bg-accent" style={{ width: `${Math.max(2, (r.hours / topHours) * 100)}%` }} />
                          </div>
                          <span className="w-9 text-right text-xs num text-fg-3">{`${Math.round(share * 100)}%`}</span>
                        </div>
                      </td>
                      <td className="text-right num text-fg-2">{formatMoney(r.value || 0)}</td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )
      ) : byMonth.length === 0 ? (
        <div className="card"><EmptyState icon={BarChart3} title="No invoices in this range" description="Income shows up here once invoices are sent or paid." /></div>
      ) : (
        <div className="card p-4">
          <ResponsiveContainer width="100%" height={300}>
            <BarChart data={monthSeries} margin={{ left: 0, right: 8, top: 8, bottom: 0 }} barGap={4}>
              <CartesianGrid vertical={false} stroke="rgb(var(--line))" />
              <XAxis dataKey="label" {...axis} />
              <YAxis {...axis} width={56} tickFormatter={v => formatMoneyCompact(v)} />
              <Tooltip cursor={{ fill: 'rgb(var(--fg) / 0.04)' }} content={tooltip(v => formatMoney(v))} />
              <Legend iconType="circle" iconSize={8} wrapperStyle={{ fontSize: 12, color: 'rgb(var(--fg-2))' }} />
              <Bar dataKey="invoiced" name="Invoiced" fill="rgb(var(--blue))" radius={[3, 3, 0, 0]} maxBarSize={22} />
              <Bar dataKey="paid" name="Paid" fill="rgb(var(--green))" radius={[3, 3, 0, 0]} maxBarSize={22} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  )
}
