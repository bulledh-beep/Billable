import { useEffect, useMemo, useState } from 'react'
import { BarChart3, AlertCircle, ChevronDown, ChevronRight } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import EmptyState from '../components/EmptyState'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from 'recharts'
import ExpensePanel from '../components/ExpensePanel'
import { formatMoney, formatMoneyCompact, formatDay } from '../utils/format'
import type { Expense, ExpenseCategory, TaxSettings } from '@shared/types'
import toast from 'react-hot-toast'


const CATEGORY_LABEL: Record<ExpenseCategory, string> = {
  equipment: 'Equipment',
  software: 'Software',
  home_office: 'Home office',
  phone_internet: 'Phone and internet',
  travel: 'Travel',
  meals: 'Meals',
  professional_development: 'Professional development',
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
    <div className="page">
      <PageHeader
        title="Tax overview"
        actions={
          <>
            <select className="input w-28" value={year} onChange={e => setYear(parseInt(e.target.value))} aria-label="Tax year">
              {availableYears.map(y => <option key={y} value={y}>{y}</option>)}
            </select>
            <button onClick={() => handleExport('pdf')} disabled={exporting === 'pdf'} className="btn-secondary">
              {exporting === 'pdf' ? 'Creating…' : 'Tax summary PDF'}
            </button>
          </>
        }
      />

      {/* Set-aside estimate */}
      <div className="card mb-5 overflow-hidden">
        <div className="grid grid-cols-[minmax(0,1fr)_300px]">
          <div className="p-5">
            <div className="flex items-center gap-2">
              <span className="text-xs text-fg-3">Estimated to set aside for {year}</span>
              <span className="badge text-fg-3">Estimate</span>
            </div>
            <div className="font-figures text-[34px] leading-[38px] text-fg mt-1.5">{money(totalSetAside)}</div>
            <p className="text-sm text-fg-3 mt-1.5 max-w-lg">
              Based on invoices you've been paid for. Record payments on the Billing page to keep this current.
            </p>
            {hasUnpaidInvoiced && (
              <p className="text-sm text-fg-2 mt-3">
                If everything invoiced gets paid: <span className="num font-semibold text-fg">{money(projectedTotalSetAside)}</span>
                <span className="text-fg-3"> · {money(overview.total_invoiced - overview.total_paid)} still to collect</span>
              </p>
            )}
            <button
              onClick={() => setShowHowCalculated(v => !v)}
              className="btn-ghost btn-sm mt-3 -ml-2"
            >
              {showHowCalculated ? <ChevronDown /> : <ChevronRight />} How this is worked out
            </button>
            {showHowCalculated && (
              <div className="mt-2 p-3.5 rounded-lg bg-fg/[0.03] border border-line text-xs text-fg-2 space-y-1.5 leading-5">
                <p><b className="text-fg">Net business income</b> is paid invoices, minus the GST/HST collected on them, minus deductible expenses.</p>
                <p><b className="text-fg">Income tax</b> is net business income times your marginal rate from Tax settings.</p>
                <p><b className="text-fg">To set aside</b> is income tax plus the GST/HST you collected, since that belongs to the CRA.</p>
                <p className="text-fg-3 pt-1.5 border-t border-line">A planning aid only. Your actual tax depends on CPP, credits, and your other income, so check with an accountant before filing.</p>
              </div>
            )}
          </div>
          <div className="border-l border-line bg-panel-2/40 p-5 text-sm space-y-2.5">
            <div className="flex justify-between gap-4"><span className="text-fg-3">Income tax at {incomeTaxRate}%</span><span className="num text-fg">{money(incomeTaxEstimate)}</span></div>
            <div className="flex justify-between gap-4"><span className="text-fg-3">GST/HST to remit</span><span className="num text-fg">{money(overview.gst_collected_paid)}</span></div>
            <div className="flex justify-between gap-4 pt-2.5 border-t border-line font-semibold"><span className="text-fg">Total</span><span className="num text-fg">{money(totalSetAside)}</span></div>
            {hasUnpaidInvoiced && (
              <div className="pt-2.5 border-t border-line space-y-1.5 text-xs">
                <div className="flex justify-between gap-4"><span className="text-fg-3">Projected income tax</span><span className="num text-fg-2">{money(projectedIncomeTax)}</span></div>
                <div className="flex justify-between gap-4"><span className="text-fg-3">Projected GST/HST</span><span className="num text-fg-2">{money(overview.gst_collected_total)}</span></div>
              </div>
            )}
          </div>
        </div>
      </div>

      <MetricStrip className="mb-5">
        <Metric label="Invoiced" value={<Money amount={overview.total_invoiced} currency={taxSettings.currency || 'CAD'} />} sub={`${overview.invoice_count} invoice${overview.invoice_count === 1 ? '' : 's'}`} />
        <Metric label="Paid" value={<Money amount={overview.total_paid} currency={taxSettings.currency || 'CAD'} />} sub={`${overview.paid_count} paid`} />
        <Metric label="Outstanding" value={<Money amount={overview.total_outstanding} currency={taxSettings.currency || 'CAD'} />} sub="Sent, not yet paid" />
        <Metric label="Expenses" value={<Money amount={overview.total_expenses} currency={taxSettings.currency || 'CAD'} />} sub={`${expenses.length} logged`} />
        <Metric label="Net business income" value={<Money amount={netIncome} currency={taxSettings.currency || 'CAD'} />} sub="Paid, less GST and expenses" />
      </MetricStrip>

      {/* Monthly chart */}
      <div className="card p-4 mb-5">
        <div className="flex items-center justify-between mb-3">
          <span className="card-title">Paid income by month</span>
          <span className="text-xs text-fg-3">Counted in the month the payment arrived</span>
        </div>
        <ResponsiveContainer width="100%" height={220}>
          <BarChart data={chartData} margin={{ left: 0, right: 8, top: 4, bottom: 0 }}>
            <CartesianGrid vertical={false} stroke="rgb(var(--line))" />
            <XAxis dataKey="month" stroke="rgb(var(--fg-4))" fontSize={11} tickLine={false} axisLine={false} />
            <YAxis stroke="rgb(var(--fg-4))" fontSize={11} tickLine={false} axisLine={false} width={56} tickFormatter={v => formatMoneyCompact(v)} />
            <Tooltip
              cursor={{ fill: 'rgb(var(--fg) / 0.04)' }}
              content={({ active, payload, label }: any) => {
                if (!active || !payload?.length) return null
                return (
                  <div className="rounded-md border border-line bg-panel shadow-pop px-3 py-2 text-xs">
                    <div className="text-fg-3 mb-0.5">{label} {year}</div>
                    <div className="num font-medium text-fg">{money(payload[0].value)}</div>
                  </div>
                )
              }}
            />
            <Bar dataKey="paid" fill="rgb(var(--green))" radius={[3, 3, 0, 0]} maxBarSize={32} />
          </BarChart>
        </ResponsiveContainer>
      </div>

      {/* Expenses */}
      <div className="card mb-5">
        <div className="card-header">
          <div className="flex items-baseline gap-2">
            <span className="card-title">Expenses</span>
            <span className="text-xs text-fg-3">{money(overview.total_expenses)} deductible</span>
          </div>
          <div className="flex items-center gap-2">
            <button onClick={() => handleExport('expenses')} disabled={exporting === 'expenses'} className="btn-ghost btn-sm">
              CSV
            </button>
            <button onClick={handleNewExpense} className="btn-secondary btn-sm">Add expense</button>
          </div>
        </div>

        {overview.expenses_by_category.length > 0 && (
          <div className="flex flex-wrap gap-2 px-4 py-3 border-b border-line">
            {overview.expenses_by_category.map(c => (
              <div key={c.category} className="flex items-center gap-2 h-7 pl-2.5 pr-2 rounded-md bg-fg/[0.04] border border-line text-xs">
                <span className="text-fg-2">{CATEGORY_LABEL[c.category as ExpenseCategory] || c.category}</span>
                <span className="num font-medium text-fg">{money(c.total)}</span>
              </div>
            ))}
          </div>
        )}

        {expenses.length === 0 ? (
          <EmptyState compact icon={BarChart3} title={`No expenses for ${year}`} description="Log deductible costs to lower your taxable income." />
        ) : (
          <table className="tbl tbl-compact">
            <thead>
              <tr>
                <th className="w-28">Date</th>
                <th>Category</th>
                <th>Description</th>
                <th className="text-right">Amount</th>
              </tr>
            </thead>
            <tbody>
              {expenses.map(e => (
                <tr key={e.id} onClick={() => handleEditExpense(e)} className="row-hover">
                  <td className="text-fg-2 num whitespace-nowrap">{formatDay(e.date)}</td>
                  <td className="text-fg-2">{CATEGORY_LABEL[e.category] || e.category}</td>
                  <td className="text-fg truncate max-w-[320px]">{e.description || <span className="text-fg-4">No description</span>}</td>
                  <td className="text-right num text-fg">{money(e.amount)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Exports */}
      <div className="card px-4 py-3.5 flex items-center justify-between">
        <div>
          <div className="text-sm font-medium text-fg">Year-end files</div>
          <div className="text-xs text-fg-3">For your bookkeeper or accountant.</div>
        </div>
        <div className="flex gap-2">
          <button onClick={() => handleExport('invoices')} disabled={exporting === 'invoices'} className="btn-secondary btn-sm">Invoices CSV</button>
          <button onClick={() => handleExport('expenses')} disabled={exporting === 'expenses'} className="btn-secondary btn-sm">Expenses CSV</button>
          <button onClick={() => handleExport('pdf')} disabled={exporting === 'pdf'} className="btn-secondary btn-sm">Summary PDF</button>
        </div>
      </div>

      <p className="flex items-start gap-2 text-xs text-fg-3 mt-5 leading-5">
        <AlertCircle className="w-3.5 h-3.5 shrink-0 mt-0.5" />
        These numbers are estimates from your Billable records, not tax, legal, or financial advice.
      </p>

      <ExpensePanel
        open={panelOpen}
        expense={editingExpense}
        defaultTaxYear={year}
        onClose={() => setPanelOpen(false)}
        onSaved={handlePanelSaved}
        onDeleted={handlePanelSaved}
      />
    </div>
  )
}

