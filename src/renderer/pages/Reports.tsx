import { useState, useEffect, useMemo } from 'react'
import { motion } from 'framer-motion'
import {
  BarChart3, Download, Calendar, DollarSign, CreditCard, TrendingUp, Percent, Info,
} from 'lucide-react'
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, Legend, ResponsiveContainer, Cell,
} from 'recharts'
import { formatMoney, formatHours, formatDate } from '../utils/format'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

const CHART_COLORS = ['#F5A623', '#3498DB', '#2ECC71', '#E74C3C', '#9B59B6', '#1ABC9C', '#E67E22', '#EC407A']

const CATEGORY_LABEL: Record<string, string> = {
  equipment: 'Equipment / Hardware',
  software: 'Software & SaaS',
  home_office: 'Home Office',
  phone_internet: 'Phone & Internet',
  travel: 'Travel',
  meals: 'Meals & Entertainment',
  professional_development: 'Professional Dev',
  other: 'Other',
}

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
  
  const [bills, setBills] = useState<any[]>([])
  const [subscriptions, setSubscriptions] = useState<any[]>([])
  const [expenses, setExpenses] = useState<any[]>([])
  const [invoices, setInvoices] = useState<any[]>([])
  const [taxSettings, setTaxSettings] = useState<any>(null)
  
  const [activeTab, setActiveTab] = useState<'projects' | 'clients' | 'earnings' | 'bills' | 'subscriptions' | 'expenses' | 'cashflow' | 'taxes'>('projects')

  useEffect(() => { loadReports() }, [dateRange])
  useEffect(() => { loadReports() }, [isTimerRunning])

  useEffect(() => {
    const onFocus = () => loadReports()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [dateRange])

  const loadReports = async () => {
    try {
      const [byProject, byClient, byMonth, b, s, exp, inv, ts] = await Promise.all([
        window.api.reports.hoursByProject(dateRange.start, dateRange.end),
        window.api.reports.hoursByClient(dateRange.start, dateRange.end),
        window.api.reports.earningsByMonth(dateRange.start, dateRange.end),
        window.api.bills.list(),
        window.api.subscriptions.list(),
        window.api.expenses.list(),
        window.api.invoices.list(),
        window.api.tax.getSettings(),
      ])
      setHoursByProject(byProject)
      setHoursByClient(byClient)
      setEarningsByMonth(byMonth)
      setBills(b)
      setSubscriptions(s)
      setExpenses(exp)
      setInvoices(inv)
      setTaxSettings(ts)
    } catch (err: any) {
      toast.error(`Error loading report data: ${err.message || err}`)
    }
  }

  // --- Monthly Bill Calculations ---
  const billsReportData = useMemo(() => {
    const filtered = bills.filter(b => {
      if (!b.due_date) return false
      return b.due_date >= dateRange.start && b.due_date <= dateRange.end
    })

    const monthlyMap: Record<string, { month: string; paid: number; unpaid: number; total: number }> = {}
    filtered.forEach(b => {
      const month = b.due_date.slice(0, 7) // 'YYYY-MM'
      if (!monthlyMap[month]) {
        monthlyMap[month] = { month, paid: 0, unpaid: 0, total: 0 }
      }
      const amt = b.amount || 0
      if (b.status === 'paid') {
        monthlyMap[month].paid += amt
      } else {
        monthlyMap[month].unpaid += amt
      }
      monthlyMap[month].total += amt
    })

    const chartData = Object.values(monthlyMap).sort((a, b) => a.month.localeCompare(b.month))

    const categoryMap: Record<string, { category: string; amount: number; count: number }> = {}
    filtered.forEach(b => {
      const cat = b.category || 'other'
      if (!categoryMap[cat]) {
        categoryMap[cat] = { category: cat, amount: 0, count: 0 }
      }
      categoryMap[cat].amount += b.amount || 0
      categoryMap[cat].count += 1
    })

    const categoriesList = Object.values(categoryMap).sort((a, b) => b.amount - a.amount)
    const totalAmount = filtered.reduce((s, b) => s + (b.amount || 0), 0)
    const totalPaid = filtered.filter(b => b.status === 'paid').reduce((s, b) => s + (b.amount || 0), 0)

    return {
      filtered,
      chartData,
      categoriesList,
      totalAmount,
      totalPaid,
      totalUnpaid: totalAmount - totalPaid
    }
  }, [bills, dateRange])

  // --- Subscription Calculations ---
  const subscriptionsReportData = useMemo(() => {
    const activeSubs = subscriptions.filter(s => s.status === 'active')
    
    let monthlyTotal = 0
    let annualTotal = 0

    activeSubs.forEach(s => {
      const amt = s.amount || 0
      const cycle = s.billing_cycle || 'monthly'
      if (cycle === 'monthly') {
        monthlyTotal += amt
        annualTotal += amt * 12
      } else if (cycle === 'yearly') {
        monthlyTotal += amt / 12
        annualTotal += amt
      } else if (cycle === 'weekly') {
        monthlyTotal += amt * 4.33
        annualTotal += amt * 52
      } else if (cycle === 'quarterly') {
        monthlyTotal += amt / 3
        annualTotal += amt * 4
      }
    })

    const categoryMap: Record<string, { category: string; amount: number; count: number }> = {}
    activeSubs.forEach(s => {
      const cat = s.category || 'software'
      if (!categoryMap[cat]) {
        categoryMap[cat] = { category: cat, amount: 0, count: 0 }
      }
      const amt = s.amount || 0
      const cycle = s.billing_cycle || 'monthly'
      let monthlyAmt = amt
      if (cycle === 'yearly') monthlyAmt = amt / 12
      else if (cycle === 'weekly') monthlyAmt = amt * 4.33
      else if (cycle === 'quarterly') monthlyAmt = amt / 3

      categoryMap[cat].amount += monthlyAmt
      categoryMap[cat].count += 1
    })

    const categoriesList = Object.values(categoryMap).sort((a, b) => b.amount - a.amount)

    return {
      activeSubs,
      monthlyTotal,
      annualTotal,
      categoriesList
    }
  }, [subscriptions])

  // --- Expense Calculations ---
  const expensesReportData = useMemo(() => {
    const filtered = expenses.filter(e => e.date >= dateRange.start && e.date <= dateRange.end)

    const categoryMap: Record<string, { category: string; total: number; deductible: number; count: number }> = {}
    filtered.forEach(e => {
      const cat = e.category || 'other'
      if (!categoryMap[cat]) {
        categoryMap[cat] = { category: cat, total: 0, deductible: 0, count: 0 }
      }
      categoryMap[cat].total += e.amount || 0
      if (e.is_deductible !== 0) {
        categoryMap[cat].deductible += e.amount || 0
      }
      categoryMap[cat].count += 1
    })

    const categoriesList = Object.values(categoryMap).sort((a, b) => b.total - a.total)
    const totalAmount = filtered.reduce((s, e) => s + (e.amount || 0), 0)
    const totalDeductible = filtered.reduce((s, e) => s + (e.is_deductible !== 0 ? (e.amount || 0) : 0), 0)

    return {
      filtered,
      categoriesList,
      totalAmount,
      totalDeductible
    }
  }, [expenses, dateRange])

  // --- Cash Flow Calculations ---
  const cashFlowReportData = useMemo(() => {
    const startYear = parseInt(dateRange.start.slice(0, 4))
    const endYear = parseInt(dateRange.end.slice(0, 4))
    const startMonth = parseInt(dateRange.start.slice(5, 7))
    const endMonth = parseInt(dateRange.end.slice(5, 7))

    const months: string[] = []
    let currYear = startYear
    let currMonth = startMonth
    while (currYear < endYear || (currYear === endYear && currMonth <= endMonth)) {
      months.push(`${currYear}-${String(currMonth).padStart(2, '0')}`)
      currMonth++
      if (currMonth > 12) {
        currMonth = 1
        currYear++
      }
    }

    const monthlyMap: Record<string, { month: string; income: number; expenses: number; bills: number; outflow: number; net: number }> = {}
    months.forEach(m => {
      monthlyMap[m] = { month: m, income: 0, expenses: 0, bills: 0, outflow: 0, net: 0 }
    })

    invoices.forEach(inv => {
      if (inv.status === 'paid' && inv.payment_date) {
        const month = inv.payment_date.slice(0, 7)
        if (monthlyMap[month]) {
          monthlyMap[month].income += inv.total || 0
        }
      }
    })

    expenses.forEach(e => {
      const month = e.date.slice(0, 7)
      if (monthlyMap[month]) {
        monthlyMap[month].expenses += e.amount || 0
      }
    })

    bills.forEach(b => {
      if (b.status === 'paid' && b.due_date) {
        const month = b.due_date.slice(0, 7)
        if (monthlyMap[month]) {
          monthlyMap[month].bills += b.amount || 0
        }
      }
    })

    Object.keys(monthlyMap).forEach(m => {
      const row = monthlyMap[m]
      row.outflow = row.expenses + row.bills
      row.net = row.income - row.outflow
    })

    const chartData = Object.values(monthlyMap).sort((a, b) => a.month.localeCompare(b.month))
    const totalIncome = chartData.reduce((s, r) => s + r.income, 0)
    const totalOutflow = chartData.reduce((s, r) => s + r.outflow, 0)

    return {
      chartData,
      totalIncome,
      totalOutflow,
      netCashFlow: totalIncome - totalOutflow
    }
  }, [invoices, expenses, bills, dateRange])

  // --- Tax Estimate Calculations ---
  const taxEstimateReportData = useMemo(() => {
    const rangeInvoices = invoices.filter(i => i.issue_date >= dateRange.start && i.issue_date <= dateRange.end)
    const totalInvoiced = rangeInvoices.reduce((s, i) => s + (i.total || 0), 0)
    const totalPaid = rangeInvoices.filter(i => i.status === 'paid').reduce((s, i) => s + (i.total || 0), 0)
    const totalOutstanding = rangeInvoices.filter(i => ['sent', 'overdue'].includes(i.status)).reduce((s, i) => s + (i.total || 0), 0)

    const gstCollectedTotal = rangeInvoices.reduce((s, i) => s + (i.gst_hst_amount || 0), 0)
    const gstCollectedPaid = rangeInvoices.filter(i => i.status === 'paid').reduce((s, i) => s + (i.gst_hst_amount || 0), 0)

    const rangeExpenses = expenses.filter(e => e.date >= dateRange.start && e.date <= dateRange.end)
    const totalDeductible = rangeExpenses.filter(e => e.is_deductible !== 0).reduce((s, e) => s + (e.amount || 0), 0)

    const realizedTaxableIncome = Math.max(0, (totalPaid - gstCollectedPaid) - totalDeductible)
    const incomeTaxRate = taxSettings?.income_tax_bracket || 25
    const estimatedTax = realizedTaxableIncome * (incomeTaxRate / 100)
    const totalSetAside = gstCollectedPaid + estimatedTax

    const projectedTaxableIncome = Math.max(0, (totalInvoiced - gstCollectedTotal) - totalDeductible)
    const projectedIncomeTax = projectedTaxableIncome * (incomeTaxRate / 100)
    const projectedTotalSetAside = gstCollectedTotal + projectedIncomeTax

    return {
      totalInvoiced,
      totalPaid,
      totalOutstanding,
      gstCollectedTotal,
      gstCollectedPaid,
      totalDeductible,
      realizedTaxableIncome,
      estimatedTax,
      totalSetAside,
      projectedTaxableIncome,
      projectedIncomeTax,
      projectedTotalSetAside,
      incomeTaxRate
    }
  }, [invoices, expenses, taxSettings, dateRange])

  const handleExport = async () => {
    let data: any[]
    let filename: string
    if (activeTab === 'projects') {
      data = hoursByProject
      filename = 'hours-by-project.csv'
    } else if (activeTab === 'clients') {
      data = hoursByClient
      filename = 'hours-by-client.csv'
    } else if (activeTab === 'earnings') {
      data = earningsByMonth
      filename = 'earnings-by-month.csv'
    } else if (activeTab === 'bills') {
      data = billsReportData.filtered.map(b => ({
        vendor: b.vendor,
        amount: b.amount,
        currency: b.currency,
        due_date: b.due_date,
        category: b.category,
        status: b.status,
        recurring: b.recurring ? 'Yes' : 'No',
        frequency: b.frequency,
      }))
      filename = 'bills-report.csv'
    } else if (activeTab === 'subscriptions') {
      data = subscriptionsReportData.activeSubs.map(s => ({
        name: s.name,
        vendor: s.vendor,
        amount: s.amount,
        currency: s.currency,
        billing_cycle: s.billing_cycle,
        category: s.category,
        payment_method: s.payment_method,
        status: s.status,
      }))
      filename = 'subscriptions-report.csv'
    } else if (activeTab === 'expenses') {
      data = expensesReportData.filtered.map(e => ({
        date: e.date,
        category: e.category,
        vendor: e.vendor,
        description: e.description,
        amount: e.amount,
        currency: e.currency,
        is_deductible: e.is_deductible !== 0 ? 'Yes' : 'No',
        receipt_note: e.receipt_note,
      }))
      filename = 'expenses-report.csv'
    } else if (activeTab === 'cashflow') {
      data = cashFlowReportData.chartData.map(r => ({
        month: r.month,
        income: r.income,
        outflow: r.outflow,
        net: r.net,
      }))
      filename = 'cash-flow-report.csv'
    } else if (activeTab === 'taxes') {
      const t = taxEstimateReportData
      data = [
        { metric: 'Gross Paid Income', amount: t.totalPaid },
        { metric: 'Less: GST/HST Collected on Paid', amount: -t.gstCollectedPaid },
        { metric: 'Less: Deductible Expenses', amount: -t.totalDeductible },
        { metric: 'Taxable Income (Realized)', amount: t.realizedTaxableIncome },
        { metric: 'Estimated Personal Tax Bracket (%)', amount: t.incomeTaxRate },
        { metric: 'Estimated Income Tax', amount: t.estimatedTax },
        { metric: 'GST/HST to Remit', amount: t.gstCollectedPaid },
        { metric: 'Total Set-Aside Estimate', amount: t.totalSetAside },
      ]
      filename = 'tax-estimate-report.csv'
    } else {
      return
    }

    if (data.length === 0) return toast.error('No data to export')
    const result = await window.api.reports.exportCSV(data, filename)
    if (result) toast.success('CSV exported')
  }

  const totalHours = hoursByProject.reduce((sum, p) => sum + p.hours, 0)
  const currency = taxSettings?.currency || 'CAD'

  const customTooltip = ({ active, payload, label }: any) => {
    if (!active || !payload?.length) return null
    return (
      <div className="bg-surface-200 border border-rim/[0.06] rounded-lg px-3 py-2 shadow-lg">
        <p className="text-xs text-text-secondary mb-1">{label || payload[0]?.payload?.name || payload[0]?.name}</p>
        {payload.map((p: any, idx: number) => (
          <p key={idx} className="text-sm font-mono text-text-primary font-medium">
            {p.name ? `${p.name}: ` : ''}
            {activeTab === 'projects' || activeTab === 'clients' ? `${formatHours(p.value)} hours` : formatMoney(p.value, currency)}
          </p>
        ))}
      </div>
    )
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Reports</h1>
          <p className="text-sm text-text-secondary mt-1 font-mono">
            {dateRange.start} to {dateRange.end}
          </p>
        </div>
        <button onClick={handleExport} className="btn-secondary flex items-center gap-2">
          <Download className="w-4 h-4" /> Export CSV
        </button>
      </motion.div>

      {/* Date Range Selectors */}
      <motion.div variants={item} className="flex flex-wrap gap-4 items-center mb-6">
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

      {/* Navigation Tabs */}
      <motion.div variants={item} className="flex flex-wrap gap-1 bg-surface-100 rounded-lg p-0.5 border border-rim/[0.04] w-fit mb-6">
        {[
          { key: 'projects', label: 'Hours by Project' },
          { key: 'clients', label: 'Hours by Client' },
          { key: 'earnings', label: 'Earnings' },
          { key: 'bills', label: 'Bills' },
          { key: 'subscriptions', label: 'Subscriptions' },
          { key: 'expenses', label: 'Expenses' },
          { key: 'cashflow', label: 'Cash Flow' },
          { key: 'taxes', label: 'Tax Estimate' },
        ].map(tab => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key as any)}
            className={`px-3 py-1.5 rounded-md text-xs font-medium transition-colors ${
              activeTab === tab.key ? 'bg-surface-300 text-text-primary' : 'text-text-tertiary hover:text-text-secondary'
            }`}
          >
            {tab.label}
          </button>
        ))}
      </motion.div>

      {/* Chart Section */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        {activeTab === 'projects' && (
          hoursByProject.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No hours tracked in this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={hoursByProject} layout="vertical" margin={{ left: 120 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis type="number" stroke="#6B6A67" fontSize={11} tickFormatter={v => `${v}h`} />
                <YAxis type="category" dataKey="name" stroke="#6B6A67" fontSize={11} width={110} />
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
              <p className="text-sm text-text-secondary">No client work in this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={hoursByClient} layout="vertical" margin={{ left: 120 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis type="number" stroke="#6B6A67" fontSize={11} tickFormatter={v => `${v}h`} />
                <YAxis type="category" dataKey="name" stroke="#6B6A67" fontSize={11} width={110} />
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
              <p className="text-sm text-text-secondary">No earnings logged in this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={earningsByMonth}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis dataKey="month" stroke="#6B6A67" fontSize={11} />
                <YAxis stroke="#6B6A67" fontSize={11} tickFormatter={v => `$${v}`} />
                <Tooltip content={customTooltip} />
                <Bar dataKey="earnings" fill="#F5A623" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          )
        )}

        {activeTab === 'bills' && (
          billsReportData.chartData.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No bills due in this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={billsReportData.chartData}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis dataKey="month" stroke="#6B6A67" fontSize={11} />
                <YAxis stroke="#6B6A67" fontSize={11} tickFormatter={v => `$${v}`} />
                <Tooltip content={customTooltip} />
                <Legend />
                <Bar dataKey="paid" stackId="a" fill="#2ECC71" radius={[0, 0, 0, 0]} name="Paid Bills" />
                <Bar dataKey="unpaid" stackId="a" fill="#E74C3C" radius={[4, 4, 0, 0]} name="Unpaid/Due Bills" />
              </BarChart>
            </ResponsiveContainer>
          )
        )}

        {activeTab === 'subscriptions' && (
          subscriptionsReportData.categoriesList.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No active subscriptions found</p>
            </div>
          ) : (
            <div className="space-y-4">
              <h3 className="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-2">Monthly Cost by Category</h3>
              <ResponsiveContainer width="100%" height={260}>
                <BarChart data={subscriptionsReportData.categoriesList} layout="vertical" margin={{ left: 120 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                  <XAxis type="number" stroke="#6B6A67" fontSize={11} tickFormatter={v => `$${v}`} />
                  <YAxis type="category" dataKey="category" stroke="#6B6A67" fontSize={11} width={110} tickFormatter={v => CATEGORY_LABEL[v] || v} />
                  <Tooltip content={customTooltip} />
                  <Bar dataKey="amount" fill="#3498DB" radius={[0, 4, 4, 0]} name="Monthly Cost" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          )
        )}

        {activeTab === 'expenses' && (
          expensesReportData.categoriesList.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No expenses logged in this period</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={expensesReportData.categoriesList} layout="vertical" margin={{ left: 120 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis type="number" stroke="#6B6A67" fontSize={11} tickFormatter={v => `$${v}`} />
                <YAxis type="category" dataKey="category" stroke="#6B6A67" fontSize={11} width={110} tickFormatter={v => CATEGORY_LABEL[v] || v} />
                <Tooltip content={customTooltip} />
                <Legend />
                <Bar dataKey="deductible" stackId="a" fill="#2ECC71" radius={[0, 0, 0, 0]} name="Deductible" />
                <Bar dataKey="total" stackId="b" fill="#E74C3C" radius={[0, 4, 4, 0]} name="Total Expense" />
              </BarChart>
            </ResponsiveContainer>
          )
        )}

        {activeTab === 'cashflow' && (
          cashFlowReportData.chartData.length === 0 ? (
            <div className="text-center py-16">
              <BarChart3 className="w-10 h-10 text-text-tertiary mx-auto mb-2" />
              <p className="text-sm text-text-secondary">No income or expense data found</p>
            </div>
          ) : (
            <ResponsiveContainer width="100%" height={320}>
              <BarChart data={cashFlowReportData.chartData}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgb(var(--rim) / 0.08)" />
                <XAxis dataKey="month" stroke="#6B6A67" fontSize={11} />
                <YAxis stroke="#6B6A67" fontSize={11} tickFormatter={v => `$${v}`} />
                <Tooltip content={customTooltip} />
                <Legend />
                <Bar dataKey="income" fill="#2ECC71" radius={[4, 4, 0, 0]} name="Income (Paid Invoices)" />
                <Bar dataKey="outflow" fill="#E74C3C" radius={[4, 4, 0, 0]} name="Cash Out (Expenses + Paid Bills)" />
              </BarChart>
            </ResponsiveContainer>
          )
        )}

        {activeTab === 'taxes' && (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
            <div>
              <h3 className="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-3">Realized (Paid Invoices)</h3>
              <div className="space-y-2.5">
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm">
                  <span className="text-text-secondary">Gross Cash Collected</span>
                  <span className="font-mono font-medium text-text-primary">{formatMoney(taxEstimateReportData.totalPaid, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm text-status-overdue">
                  <span className="text-text-secondary">Less: GST/HST Portion</span>
                  <span className="font-mono">-{formatMoney(taxEstimateReportData.gstCollectedPaid, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm text-status-overdue">
                  <span className="text-text-secondary">Less: Deductible Expenses</span>
                  <span className="font-mono">-{formatMoney(taxEstimateReportData.totalDeductible, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-2 border-b-2 border-text-secondary font-medium text-sm">
                  <span className="text-text-primary">Realized Taxable Income</span>
                  <span className="font-mono text-accent">{formatMoney(taxEstimateReportData.realizedTaxableIncome, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm text-text-tertiary">
                  <span>Tax Bracket Rate</span>
                  <span>{taxEstimateReportData.incomeTaxRate}%</span>
                </div>
                <div className="flex justify-between items-center py-2 text-base font-bold text-accent">
                  <span>Estimated Tax Owed</span>
                  <span className="font-mono">{formatMoney(taxEstimateReportData.estimatedTax, currency)}</span>
                </div>
              </div>
            </div>

            <div>
              <h3 className="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-3">Projected (Total Invoiced)</h3>
              <div className="space-y-2.5">
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm">
                  <span className="text-text-secondary">Gross Revenue Invoiced</span>
                  <span className="font-mono font-medium text-text-primary">{formatMoney(taxEstimateReportData.totalInvoiced, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm text-status-overdue">
                  <span className="text-text-secondary">Less: Projected GST/HST</span>
                  <span className="font-mono">-{formatMoney(taxEstimateReportData.gstCollectedTotal, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm text-status-overdue">
                  <span className="text-text-secondary">Less: Deductible Expenses</span>
                  <span className="font-mono">-{formatMoney(taxEstimateReportData.totalDeductible, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-2 border-b-2 border-text-secondary font-medium text-sm">
                  <span className="text-text-primary">Projected Taxable Income</span>
                  <span className="font-mono text-text-primary">{formatMoney(taxEstimateReportData.projectedTaxableIncome, currency)}</span>
                </div>
                <div className="flex justify-between items-center py-1.5 border-b border-rim/[0.04] text-sm text-text-tertiary">
                  <span>Tax Bracket Rate</span>
                  <span>{taxEstimateReportData.incomeTaxRate}%</span>
                </div>
                <div className="flex justify-between items-center py-2 text-base font-bold text-text-primary">
                  <span>Projected Income Tax</span>
                  <span className="font-mono">{formatMoney(taxEstimateReportData.projectedIncomeTax, currency)}</span>
                </div>
              </div>
            </div>
          </div>
        )}
      </motion.div>

      {/* Summary Stats Row */}
      <motion.div variants={item} className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        {activeTab === 'projects' && (
          <>
            <SummaryCard label="Total Tracked Hours" value={`${formatHours(totalHours)}h`} />
            <SummaryCard label="Unique Projects" value={hoursByProject.length} />
            <SummaryCard label="Avg Hours / Project" value={hoursByProject.length ? `${formatHours(totalHours / hoursByProject.length)}h` : '0h'} />
            <SummaryCard label="Date Range Span" value={`${Math.round((new Date(dateRange.end).getTime() - new Date(dateRange.start).getTime()) / (1000 * 60 * 60 * 24))} days`} />
          </>
        )}
        {activeTab === 'clients' && (
          <>
            <SummaryCard label="Total Tracked Hours" value={`${formatHours(totalHours)}h`} />
            <SummaryCard label="Active Clients" value={hoursByClient.length} />
            <SummaryCard label="Avg Hours / Client" value={hoursByClient.length ? `${formatHours(totalHours / hoursByClient.length)}h` : '0h'} />
            <SummaryCard label="Date Range Span" value={`${Math.round((new Date(dateRange.end).getTime() - new Date(dateRange.start).getTime()) / (1000 * 60 * 60 * 24))} days`} />
          </>
        )}
        {activeTab === 'earnings' && (
          <>
            <SummaryCard label="Total Paid Earnings" value={formatMoney(earningsByMonth.reduce((s, r) => s + r.earnings, 0), currency)} tone="paid" />
            <SummaryCard label="Active Months" value={earningsByMonth.length} />
            <SummaryCard label="Average Monthly Earnings" value={earningsByMonth.length ? formatMoney(earningsByMonth.reduce((s, r) => s + r.earnings, 0) / earningsByMonth.length, currency) : '$0'} />
            <SummaryCard label="Outstanding Collectible" value={formatMoney(invoices.filter(i => ['sent', 'overdue'].includes(i.status)).reduce((s, i) => s + (i.total || 0), 0), currency)} tone="outstanding" />
          </>
        )}
        {activeTab === 'bills' && (
          <>
            <SummaryCard label="Total Bills Due" value={formatMoney(billsReportData.totalAmount, currency)} />
            <SummaryCard label="Paid Bills" value={formatMoney(billsReportData.totalPaid, currency)} tone="paid" />
            <SummaryCard label="Unpaid Bills" value={formatMoney(billsReportData.totalUnpaid, currency)} tone="outstanding" />
            <SummaryCard label="Total Count" value={billsReportData.filtered.length} />
          </>
        )}
        {activeTab === 'subscriptions' && (
          <>
            <SummaryCard label="Active Subscriptions" value={subscriptionsReportData.activeSubs.length} />
            <SummaryCard label="Total Monthly Cost" value={formatMoney(subscriptionsReportData.monthlyTotal, currency)} accent />
            <SummaryCard label="Annualized Commitment" value={formatMoney(subscriptionsReportData.annualTotal, currency)} bold />
            <SummaryCard label="Avg Cost / Sub" value={subscriptionsReportData.activeSubs.length ? formatMoney(subscriptionsReportData.monthlyTotal / subscriptionsReportData.activeSubs.length, currency) : '$0'} />
          </>
        )}
        {activeTab === 'expenses' && (
          <>
            <SummaryCard label="Total Logged" value={formatMoney(expensesReportData.totalAmount, currency)} />
            <SummaryCard label="Deductible Portion" value={formatMoney(expensesReportData.totalDeductible, currency)} tone="paid" />
            <SummaryCard label="Non-deductible Portion" value={formatMoney(expensesReportData.totalAmount - expensesReportData.totalDeductible, currency)} tone="outstanding" />
            <SummaryCard label="Expense Entries Count" value={expensesReportData.filtered.length} />
          </>
        )}
        {activeTab === 'cashflow' && (
          <>
            <SummaryCard label="Total Income In" value={formatMoney(cashFlowReportData.totalIncome, currency)} tone="paid" />
            <SummaryCard label="Total Cash Out" value={formatMoney(cashFlowReportData.totalOutflow, currency)} tone="outstanding" />
            <SummaryCard label="Net Cash Flow" value={formatMoney(cashFlowReportData.netCashFlow, currency)} bold accent />
            <SummaryCard label="Months Tracked" value={cashFlowReportData.chartData.length} />
          </>
        )}
        {activeTab === 'taxes' && (
          <>
            <SummaryCard label="Taxable Income (Realized)" value={formatMoney(taxEstimateReportData.realizedTaxableIncome, currency)} />
            <SummaryCard label="GST/HST Collected (Paid)" value={formatMoney(taxEstimateReportData.gstCollectedPaid, currency)} tone="outstanding" />
            <SummaryCard label="Income Tax Estimate" value={formatMoney(taxEstimateReportData.estimatedTax, currency)} tone="outstanding" />
            <SummaryCard label="Total Set-Aside Estimate" value={formatMoney(taxEstimateReportData.totalSetAside, currency)} bold accent />
          </>
        )}
      </motion.div>

      {/* Details Table Section */}
      {activeTab === 'projects' && hoursByProject.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04] bg-surface-200/20">
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Project</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Hours</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">% of Total</th>
              </tr>
            </thead>
            <tbody>
              {hoursByProject.map((p, i) => (
                <tr key={i} className="border-b border-rim/[0.02] hover:bg-surface-200/10">
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

      {activeTab === 'clients' && hoursByClient.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04] bg-surface-200/20">
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Client</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Hours</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">% of Total</th>
              </tr>
            </thead>
            <tbody>
              {hoursByClient.map((c, i) => (
                <tr key={i} className="border-b border-rim/[0.02] hover:bg-surface-200/10">
                  <td className="px-4 py-3 text-sm text-text-primary flex items-center gap-2">
                    <div className="w-2.5 h-2.5 rounded-md flex items-center justify-center text-[10px] font-semibold text-white bg-accent">
                      {c.name.slice(0, 1)}
                    </div>
                    {c.name}
                  </td>
                  <td className="px-4 py-3 text-sm font-mono text-text-primary text-right">{formatHours(c.hours)}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-secondary text-right">
                    {totalHours > 0 ? ((c.hours / totalHours) * 100).toFixed(1) : 0}%
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </motion.div>
      )}

      {activeTab === 'bills' && billsReportData.categoriesList.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04] bg-surface-200/20">
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Category</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Count</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Total Amount</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">% of Total</th>
              </tr>
            </thead>
            <tbody>
              {billsReportData.categoriesList.map((c, i) => (
                <tr key={i} className="border-b border-rim/[0.02] hover:bg-surface-200/10">
                  <td className="px-4 py-3 text-sm text-text-primary capitalize">{CATEGORY_LABEL[c.category] || c.category}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-secondary text-right">{c.count}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-primary text-right">{formatMoney(c.amount, currency)}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-secondary text-right">
                    {billsReportData.totalAmount > 0 ? ((c.amount / billsReportData.totalAmount) * 100).toFixed(1) : 0}%
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </motion.div>
      )}

      {activeTab === 'subscriptions' && subscriptionsReportData.activeSubs.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04] bg-surface-200/20">
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Name / Vendor</th>
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Billing Cycle</th>
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Payment Method</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Rate</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Monthly Equivalent</th>
              </tr>
            </thead>
            <tbody>
              {subscriptionsReportData.activeSubs.map((s, i) => {
                const amt = s.amount || 0
                const cycle = s.billing_cycle || 'monthly'
                let monthlyEquivalent = amt
                if (cycle === 'yearly') monthlyEquivalent = amt / 12
                else if (cycle === 'weekly') monthlyEquivalent = amt * 4.33
                else if (cycle === 'quarterly') monthlyEquivalent = amt / 3

                return (
                  <tr key={i} className="border-b border-rim/[0.02] hover:bg-surface-200/10">
                    <td className="px-4 py-3 text-sm text-text-primary">
                      <div className="font-medium">{s.name}</div>
                      <div className="text-xs text-text-tertiary">{s.vendor}</div>
                    </td>
                    <td className="px-4 py-3 text-sm text-text-secondary capitalize">{s.billing_cycle}</td>
                    <td className="px-4 py-3 text-sm text-text-secondary font-mono">{s.payment_method || '—'}</td>
                    <td className="px-4 py-3 text-sm font-mono text-text-primary text-right">{formatMoney(s.amount, s.currency)}</td>
                    <td className="px-4 py-3 text-sm font-mono text-accent text-right">{formatMoney(monthlyEquivalent, s.currency)}</td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </motion.div>
      )}

      {activeTab === 'expenses' && expensesReportData.categoriesList.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04] bg-surface-200/20">
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Category</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Count</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Deductible</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Total Logged</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">% of Total</th>
              </tr>
            </thead>
            <tbody>
              {expensesReportData.categoriesList.map((c, i) => (
                <tr key={i} className="border-b border-rim/[0.02] hover:bg-surface-200/10">
                  <td className="px-4 py-3 text-sm text-text-primary capitalize">{CATEGORY_LABEL[c.category] || c.category}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-secondary text-right">{c.count}</td>
                  <td className="px-4 py-3 text-sm font-mono text-status-paid text-right">{formatMoney(c.deductible, currency)}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-primary text-right">{formatMoney(c.total, currency)}</td>
                  <td className="px-4 py-3 text-sm font-mono text-text-secondary text-right">
                    {expensesReportData.totalAmount > 0 ? ((c.total / expensesReportData.totalAmount) * 100).toFixed(1) : 0}%
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </motion.div>
      )}

      {activeTab === 'cashflow' && cashFlowReportData.chartData.length > 0 && (
        <motion.div variants={item} className="glass-panel overflow-hidden mt-6">
          <table className="w-full">
            <thead>
              <tr className="border-b border-rim/[0.04] bg-surface-200/20">
                <th className="text-left px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Month</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Cash In (Income)</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Cash Out (Expenses)</th>
                <th className="text-right px-4 py-2.5 text-xs font-semibold text-text-tertiary uppercase tracking-wider">Net Cash Flow</th>
              </tr>
            </thead>
            <tbody>
              {cashFlowReportData.chartData.map((r, i) => (
                <tr key={i} className="border-b border-rim/[0.02] hover:bg-surface-200/10">
                  <td className="px-4 py-3 text-sm text-text-primary font-mono">{r.month}</td>
                  <td className="px-4 py-3 text-sm font-mono text-status-paid text-right">{formatMoney(r.income, currency)}</td>
                  <td className="px-4 py-3 text-sm font-mono text-status-overdue text-right">-{formatMoney(r.outflow, currency)}</td>
                  <td className={`px-4 py-3 text-sm font-mono font-medium text-right ${r.net >= 0 ? 'text-accent' : 'text-status-overdue'}`}>
                    {r.net >= 0 ? '+' : ''}{formatMoney(r.net, currency)}
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

interface SummaryCardProps {
  label: string
  value: string | number
  tone?: 'paid' | 'outstanding'
  accent?: boolean
  bold?: boolean
}

function SummaryCard({ label, value, tone, accent, bold }: SummaryCardProps) {
  return (
    <div className="glass-panel p-4">
      <div className="text-[10px] uppercase tracking-wider text-text-tertiary mb-1">{label}</div>
      <div className={`font-mono text-lg font-bold ${
        tone === 'paid' ? 'text-status-paid' :
        tone === 'outstanding' ? 'text-status-overdue' :
        accent ? 'text-accent' :
        bold ? 'text-text-primary' : 'text-text-primary'
      }`}>
        {value}
      </div>
    </div>
  )
}
