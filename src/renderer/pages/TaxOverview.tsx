import { useEffect, useMemo, useState } from 'react'
import { motion } from 'framer-motion'
import {
  BarChart3, Download, Plus, Pencil, AlertCircle, ChevronDown, ChevronRight, FileText,
} from 'lucide-react'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from 'recharts'
import ExpensePanel from '../components/ExpensePanel'
import { formatMoney } from '../utils/format'
import type { Expense, ExpenseCategory, TaxSettings } from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.05 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

const CATEGORY_LABEL: Record<ExpenseCategory, string> = {
  equipment: 'Equipment / Hardware',
  software: 'Software / Subscriptions',
  home_office: 'Home Office',
  phone_internet: 'Phone & Internet',
  travel: 'Travel',
  meals: 'Meals & Entertainment',
  professional_development: 'Professional Development',
  other: 'Other',
}

const MONTHS_SHORT = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

interface OverviewData {
  tax_year: number
  total_invoiced: number
  total_paid: number
  total_outstanding: number
  gst_collected_paid: number
  gst_collected_total: number
  invoice_count: number
  paid_count: number
  expenses_by_category: Array<{ category: string; total: number; count: number }>
  total_expenses: number
  monthly_income: Array<{ month: string; paid: number; invoiced: number }>
}

export default function TaxOverview() {
  const currentYear = new Date().getFullYear()
  const [year, setYear] = useState(currentYear)
  const [overview, setOverview] = useState<OverviewData | null>(null)
  const [taxSettings, setTaxSettings] = useState<TaxSettings | null>(null)
  const [expenses, setExpenses] = useState<Expense[]>([])
  const [showHowCalculated, setShowHowCalculated] = useState(false)
  const [panelOpen, setPanelOpen] = useState(false)
  const [editingExpense, setEditingExpense] = useState<Expense | null>(null)
  const [exporting, setExporting] = useState<string | null>(null)

  const availableYears = useMemo(() => {
    const years = new Set<number>([currentYear])
    for (let y = currentYear; y >= currentYear - 5; y--) years.add(y)
    return Array.from(years).sort((a, b) => b - a)
  }, [currentYear])

  useEffect(() => { loadAll() }, [year])

  const loadAll = async () => {
    try {
      const [ov, ts, exp] = await Promise.all([
        window.api.tax.getOverview(year),
        window.api.tax.getSettings(),
        window.api.expenses.list(year),
      ])
      setOverview(ov)
      setTaxSettings(ts)
      setExpenses(exp)
    } catch (err: any) {
      toast.error(`Failed to load: ${err.message || err}`)
    }
  }

  const handleNewExpense = () => {
    setEditingExpense(null)
    setPanelOpen(true)
  }

  const handleEditExpense = (e: Expense) => {
    setEditingExpense(e)
    setPanelOpen(true)
  }

  const handlePanelSaved = async () => {
    setPanelOpen(false)
    await loadAll()
  }

  const handleExport = async (kind: 'pdf' | 'invoices' | 'expenses') => {
    setExporting(kind)
    try {
      let result: string | null = null
      if (kind === 'pdf') result = await window.api.tax.exportSummaryPDF(year)
      else if (kind === 'invoices') result = await window.api.tax.exportInvoicesCSV(year)
      else result = await window.api.tax.exportExpensesCSV(year)

      if (result) toast.success('Exported')
      else if (kind !== 'pdf') toast.error('No data to export')
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    } finally {
      setExporting(null)
    }
  }

  if (!overview || !taxSettings) return null

  // Realized: net business income from PAID invoices only
  const netIncome = overview.total_paid - overview.gst_collected_paid - overview.total_expenses
  const incomeTaxRate = taxSettings.income_tax_bracket || 0
  const incomeTaxEstimate = Math.max(0, netIncome) * (incomeTaxRate / 100)
  const totalSetAside = overview.gst_collected_paid + incomeTaxEstimate

  // Projected: what to expect if everything currently invoiced (paid + outstanding) gets paid
  const projectedNetIncome =
    overview.total_invoiced - overview.gst_collected_total - overview.total_expenses
  const projectedIncomeTax = Math.max(0, projectedNetIncome) * (incomeTaxRate / 100)
  const projectedTotalSetAside = overview.gst_collected_total + projectedIncomeTax
  const hasUnpaidInvoiced = overview.total_invoiced > overview.total_paid

  const money = (amount: number) => formatMoney(amount, taxSettings.currency || 'CAD')

  // Build a 12-month chart series
  const chartData = MONTHS_SHORT.map((label, i) => {
    const key = `${year}-${String(i + 1).padStart(2, '0')}`
    const found = overview.monthly_income.find(m => m.month === key)
    return { month: label, paid: found?.paid || 0 }
  })

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      {/* Header */}
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Tax Overview</h1>
          <p className="text-sm text-text-secondary mt-1">
            {overview.invoice_count} invoice{overview.invoice_count === 1 ? '' : 's'}, {expenses.length} expense{expenses.length === 1 ? '' : 's'} for {year}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <select
            className="input-field w-28"
            value={year}
            onChange={e => setYear(parseInt(e.target.value))}
          >
            {availableYears.map(y => <option key={y} value={y}>{y}</option>)}
          </select>
          <button
            onClick={() => handleExport('pdf')}
            disabled={exporting === 'pdf'}
            className="btn-primary flex items-center gap-2"
          >
            <FileText className="w-4 h-4" />
            {exporting === 'pdf' ? 'Generating…' : 'Tax Summary PDF'}
          </button>
        </div>
      </motion.div>

      {/* Estimated Tax Owing — prominent chip */}
      <motion.div variants={item} className="glass-panel border border-accent/30 bg-accent/[0.04] p-6 mb-6">
        <div className="flex items-start justify-between">
          <div>
            <div className="flex items-center gap-2">
              <span className="text-[11px] uppercase tracking-wider text-accent/80 font-semibold">Estimated to Set Aside</span>
              <span className="text-[10px] text-text-tertiary px-1.5 py-0.5 rounded bg-surface-200" title="Estimate — not professional tax advice.">
                estimate only
              </span>
            </div>
            <div className="font-mono text-4xl font-bold text-accent mt-2 tracking-tight">
              {money(totalSetAside)}
            </div>
            <p className="text-xs text-text-tertiary mt-2 max-w-md">
              Based on invoices you've already been paid for. Mark invoices as paid (Invoices page) to grow this number.
            </p>
            {hasUnpaidInvoiced && (
              <div className="mt-3 pt-3 border-t border-accent/15 flex items-center gap-2">
                <span className="text-[10px] uppercase tracking-wider text-text-tertiary">Projected if all paid</span>
                <span className="font-mono text-base font-semibold text-text-primary">
                  {money(projectedTotalSetAside)}
                </span>
                <span className="text-[10px] text-text-tertiary">
                  ({money(overview.total_invoiced - overview.total_paid)} still uncollected)
                </span>
              </div>
            )}
          </div>
          <div className="text-right text-xs space-y-1">
            <div className="flex justify-between gap-6">
              <span className="text-text-tertiary">Income tax @ {incomeTaxRate}%</span>
              <span className="font-mono text-text-primary">{money(incomeTaxEstimate)}</span>
            </div>
            <div className="flex justify-between gap-6">
              <span className="text-text-tertiary">GST/HST to remit</span>
              <span className="font-mono text-text-primary">{money(overview.gst_collected_paid)}</span>
            </div>
            {hasUnpaidInvoiced && (
              <div className="pt-2 mt-2 border-t border-rim/[0.04]">
                <div className="flex justify-between gap-6">
                  <span className="text-text-tertiary">Projected income tax</span>
                  <span className="font-mono text-text-secondary">{money(projectedIncomeTax)}</span>
                </div>
                <div className="flex justify-between gap-6">
                  <span className="text-text-tertiary">Projected GST</span>
                  <span className="font-mono text-text-secondary">{money(overview.gst_collected_total)}</span>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* How calculated expander */}
        <button
          onClick={() => setShowHowCalculated(v => !v)}
          className="mt-4 flex items-center gap-1.5 text-xs text-text-secondary hover:text-text-primary transition-colors"
        >
          {showHowCalculated ? <ChevronDown className="w-3.5 h-3.5" /> : <ChevronRight className="w-3.5 h-3.5" />}
          How is this calculated?
        </button>
        {showHowCalculated && (
          <div className="mt-3 p-4 rounded-lg bg-surface-200/50 text-xs text-text-secondary space-y-2 leading-relaxed">
            <p><span className="font-mono text-text-primary">Net business income</span> = paid invoices − GST/HST collected on those invoices − deductible expenses</p>
            <p><span className="font-mono text-text-primary">Income tax estimate</span> = max(0, net business income) × your income tax bracket %</p>
            <p><span className="font-mono text-text-primary">Total to set aside</span> = income tax estimate + GST/HST collected (since GST belongs to the CRA, not you)</p>
            {hasUnpaidInvoiced && (
              <p className="pt-2 border-t border-rim/[0.04]">
                <span className="font-mono text-text-primary">Projected if all paid</span> uses the same formula but counts every invoice (draft, sent, overdue, paid) — useful for planning ahead.
              </p>
            )}
            <p className="text-text-tertiary pt-2 border-t border-rim/[0.04]">
              This is a planning aid only. Actual taxes depend on your full return — CPP contributions, deductions, credits, and other income sources. Consult a CPA before filing.
            </p>
          </div>
        )}
      </motion.div>

      {/* Income Summary */}
      <motion.div variants={item} className="grid grid-cols-5 gap-3 mb-6">
        <SummaryCard label="Total Invoiced" value={money(overview.total_invoiced)} />
        <SummaryCard label="Paid" value={money(overview.total_paid)} tone="paid" />
        <SummaryCard label="Outstanding" value={money(overview.total_outstanding)} tone="outstanding" />
        <SummaryCard label="GST/HST Collected" value={money(overview.gst_collected_paid)} accent />
        <SummaryCard label="Net Business Income" value={money(netIncome)} bold />
      </motion.div>

      {/* Monthly Chart */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold text-text-primary">Paid Income by Month — {year}</h2>
        </div>
        <ResponsiveContainer width="100%" height={260}>
          <BarChart data={chartData}>
            <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
            <XAxis dataKey="month" stroke="#6B6A67" fontSize={11} />
            <YAxis stroke="#6B6A67" fontSize={11} tickFormatter={v => `$${v}`} />
            <Tooltip
              cursor={{ fill: 'rgba(245,166,35,0.05)' }}
              content={({ active, payload, label }: any) => {
                if (!active || !payload?.length) return null
                return (
                  <div className="bg-surface-200 border border-rim/[0.06] rounded-lg px-3 py-2 shadow-lg">
                    <p className="text-xs text-text-secondary mb-1">{label} {year}</p>
                    <p className="text-sm font-mono text-text-primary font-medium">
                      {money(payload[0].value)}
                    </p>
                  </div>
                )
              }}
            />
            <Bar dataKey="paid" fill="#F5A623" radius={[4, 4, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </motion.div>

      {/* Expenses */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-sm font-semibold text-text-primary">Expenses</h2>
            <p className="text-[11px] text-text-tertiary mt-0.5">
              Total deductible: <span className="font-mono text-text-primary">{money(overview.total_expenses)}</span>
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => handleExport('expenses')}
              disabled={exporting === 'expenses'}
              className="btn-ghost flex items-center gap-1.5 text-xs"
              title="Export expenses CSV"
            >
              <Download className="w-3.5 h-3.5" /> CSV
            </button>
            <button onClick={handleNewExpense} className="btn-primary flex items-center gap-2 text-xs py-1.5">
              <Plus className="w-3.5 h-3.5" /> Add Expense
            </button>
          </div>
        </div>

        {/* Category breakdown bar */}
        {overview.expenses_by_category.length > 0 && (
          <div className="grid grid-cols-2 gap-2 mb-4">
            {overview.expenses_by_category.map(c => (
              <div key={c.category} className="flex items-center justify-between p-2.5 rounded-lg bg-surface-200/40">
                <div>
                  <div className="text-xs font-medium text-text-primary">
                    {CATEGORY_LABEL[c.category as ExpenseCategory] || c.category}
                  </div>
                  <div className="text-[10px] text-text-tertiary">{c.count} expense{c.count === 1 ? '' : 's'}</div>
                </div>
                <div className="font-mono text-sm text-text-primary">{money(c.total)}</div>
              </div>
            ))}
          </div>
        )}

        {expenses.length === 0 ? (
          <div className="text-center py-10">
            <BarChart3 className="w-8 h-8 text-text-tertiary mx-auto mb-2" />
            <p className="text-sm text-text-secondary">No expenses logged for {year}</p>
            <p className="text-xs text-text-tertiary mt-1">Track deductibles to lower your taxable income.</p>
          </div>
        ) : (
          <div className="overflow-hidden border border-rim/[0.04] rounded-lg">
            <table className="w-full">
              <thead>
                <tr className="border-b border-rim/[0.04] bg-surface-200/30">
                  <th className="text-left px-3 py-2.5 text-[10px] font-medium text-text-tertiary uppercase tracking-wider">Date</th>
                  <th className="text-left px-3 py-2.5 text-[10px] font-medium text-text-tertiary uppercase tracking-wider">Category</th>
                  <th className="text-left px-3 py-2.5 text-[10px] font-medium text-text-tertiary uppercase tracking-wider">Description</th>
                  <th className="text-right px-3 py-2.5 text-[10px] font-medium text-text-tertiary uppercase tracking-wider">Amount</th>
                  <th className="w-8"></th>
                </tr>
              </thead>
              <tbody>
                {expenses.map(e => (
                  <tr
                    key={e.id}
                    onClick={() => handleEditExpense(e)}
                    className="border-b border-rim/[0.02] hover:bg-surface-200/30 transition-colors cursor-pointer"
                  >
                    <td className="px-3 py-2.5 text-xs text-text-secondary font-mono">
                      {e.date.slice(5, 10)}
                    </td>
                    <td className="px-3 py-2.5 text-xs text-text-primary">
                      {CATEGORY_LABEL[e.category] || e.category}
                    </td>
                    <td className="px-3 py-2.5 text-xs text-text-secondary truncate max-w-[300px]">
                      {e.description || '—'}
                    </td>
                    <td className="px-3 py-2.5 text-xs font-mono text-text-primary text-right">
                      {money(e.amount)}
                    </td>
                    <td className="px-2">
                      <Pencil className="w-3 h-3 text-text-tertiary opacity-0 group-hover:opacity-100" />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </motion.div>

      {/* Exports row */}
      <motion.div variants={item} className="glass-panel p-4 flex items-center justify-between">
        <div>
          <h3 className="text-xs font-semibold text-text-primary">Year-end exports</h3>
          <p className="text-[10px] text-text-tertiary mt-0.5">
            Hand these to your bookkeeper or accountant.
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => handleExport('invoices')}
            disabled={exporting === 'invoices'}
            className="btn-secondary flex items-center gap-2 text-xs"
          >
            <Download className="w-3.5 h-3.5" /> Invoice CSV
          </button>
          <button
            onClick={() => handleExport('expenses')}
            disabled={exporting === 'expenses'}
            className="btn-secondary flex items-center gap-2 text-xs"
          >
            <Download className="w-3.5 h-3.5" /> Expense CSV
          </button>
          <button
            onClick={() => handleExport('pdf')}
            disabled={exporting === 'pdf'}
            className="btn-primary flex items-center gap-2 text-xs"
          >
            <FileText className="w-3.5 h-3.5" /> Tax Summary PDF
          </button>
        </div>
      </motion.div>

      {/* Disclaimer */}
      <motion.div variants={item} className="mt-6 flex items-start gap-3 p-4 rounded-lg bg-surface-200/40 border border-rim/[0.04]">
        <AlertCircle className="w-4 h-4 text-text-tertiary flex-shrink-0 mt-0.5" />
        <p className="text-xs text-text-secondary leading-relaxed">
          All numbers shown are estimates derived from your records in Billable. They do not constitute tax, legal, or financial advice. Please consult a CPA or qualified tax professional before filing or remitting.
        </p>
      </motion.div>

      <ExpensePanel
        open={panelOpen}
        expense={editingExpense}
        defaultTaxYear={year}
        onClose={() => setPanelOpen(false)}
        onSaved={handlePanelSaved}
        onDeleted={handlePanelSaved}
      />
    </motion.div>
  )
}

function SummaryCard({
  label,
  value,
  accent,
  tone,
  bold,
}: {
  label: string
  value: string
  accent?: boolean
  tone?: 'paid' | 'outstanding'
  bold?: boolean
}) {
  const valueColor = accent
    ? 'text-accent'
    : tone === 'paid'
      ? 'text-status-paid'
      : tone === 'outstanding'
        ? 'text-status-overdue'
        : 'text-text-primary'

  return (
    <div className="glass-panel p-4">
      <div className="text-[10px] uppercase tracking-wider text-text-tertiary mb-1.5">{label}</div>
      <div className={`font-mono ${bold ? 'text-xl font-bold' : 'text-lg font-semibold'} ${valueColor}`}>
        {value}
      </div>
    </div>
  )
}
