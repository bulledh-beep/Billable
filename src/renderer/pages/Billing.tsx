import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { motion, AnimatePresence } from 'framer-motion'
import { Download, FileText, Send, CreditCard, Trash2, ExternalLink, CheckCircle2, X } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import Metric, { MetricStrip } from '../components/Metric'
import Money from '../components/Money'
import StatusBadge from '../components/StatusBadge'
import Segmented from '../components/Segmented'
import SearchInput from '../components/SearchInput'
import Menu from '../components/Menu'
import EmptyState from '../components/EmptyState'
import ConfirmDialog from '../components/ConfirmDialog'
import RecordPaymentModal from '../components/RecordPaymentModal'
import MergeClientsModal from '../components/MergeClientsModal'
import { ClientAvatar } from './Clients'
import { formatMoney, formatDay, formatHoursShort, daysSince } from '../utils/format'
import { notifyBillingChanged, onBillingChanged } from '../utils/events'
import { ATTENTION_ICON } from '../utils/attention'
import { IconHourglass, IconReceipt, IconSend, IconAlert, IconTrophy } from '../components/Illustrations'
import type { AttentionItem, BillingOverview, Client, Invoice, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

type InvoiceFilter = 'all' | 'draft' | 'sent' | 'overdue' | 'paid'

export default function Billing() {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const [data, setData] = useState<BillingOverview | null>(null)
  const [clients, setClients] = useState<Client[]>([])
  const [filter, setFilter] = useState<InvoiceFilter>((searchParams.get('status') as InvoiceFilter) || 'all')
  const [year, setYear] = useState<'all' | number>('all')
  const [search, setSearch] = useState('')
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [paying, setPaying] = useState<Invoice[]>([])
  const [deleting, setDeleting] = useState<Invoice | null>(null)
  const [dontBill, setDontBill] = useState<AttentionItem | null>(null)
  const [merge, setMerge] = useState<{ source: number; target: number } | null>(null)
  const readyRef = useRef<HTMLDivElement>(null)
  const invoicesRef = useRef<HTMLDivElement>(null)

  const load = async () => {
    const [o, c] = await Promise.all([window.api.billing.overview(), window.api.clients.list()])
    setData(o)
    setClients(c)
  }

  useEffect(() => {
    load()
    return onBillingChanged(load)
  }, [])

  const refresh = () => { notifyBillingChanged() }

  const invoices = data?.invoices || []
  const years = useMemo(() => {
    const set = new Set<number>([new Date().getFullYear()])
    invoices.forEach(i => set.add(parseInt(String(i.issue_date).slice(0, 4))))
    return Array.from(set).filter(Boolean).sort((a, b) => b - a)
  }, [invoices])

  const byYear = useMemo(() => invoices.filter(i => year === 'all' || String(i.issue_date).startsWith(String(year))), [invoices, year])
  const counts = useMemo(() => ({
    all: byYear.length,
    draft: byYear.filter(i => i.status === 'draft').length,
    sent: byYear.filter(i => i.status === 'sent').length,
    overdue: byYear.filter(i => i.status === 'overdue').length,
    paid: byYear.filter(i => i.status === 'paid').length,
  }), [byYear])

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase()
    return byYear.filter(i =>
      (filter === 'all' || i.status === filter) &&
      (!q || i.invoice_number.toLowerCase().includes(q) || (i.client_name || '').toLowerCase().includes(q) || (i.project_name || '').toLowerCase().includes(q)))
  }, [byYear, filter, search])

  // Keep the selection to invoices that are still visible
  useEffect(() => {
    setSelected(prev => new Set(Array.from(prev).filter(id => visible.some(v => v.id === id))))
  }, [visible])

  if (!data) return null
  const { pipeline, ready_to_bill: ready, attention } = data
  const year0 = new Date().getFullYear()

  const selectedInvoices = visible.filter(i => selected.has(i.id))
  const selectedTotal = selectedInvoices.reduce((s, i) => s + i.total, 0)
  const canSend = selectedInvoices.some(i => i.status === 'draft')
  const canPay = selectedInvoices.some(i => i.status !== 'paid')
  const allVisibleSelected = visible.length > 0 && visible.every(i => selected.has(i.id))

  const toggle = (id: number) => setSelected(prev => {
    const next = new Set(prev)
    next.has(id) ? next.delete(id) : next.add(id)
    return next
  })

  const showInvoices = (f: InvoiceFilter) => {
    setFilter(f)
    setYear('all')
    invoicesRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  // ---- Actions ----
  const markSent = async (ids: number[]) => {
    const n = await window.api.invoices.markSent(ids)
    toast.success(n === 1 ? 'Marked as sent' : `${n} invoices marked as sent`)
    setSelected(new Set())
    refresh()
  }

  const downloadPdf = async (inv: Invoice) => {
    try {
      const path = await window.api.invoices.exportPDF(inv.id)
      if (path) toast.success(`${inv.invoice_number} saved as PDF`)
    } catch (err: any) {
      toast.error(`Couldn't create the PDF: ${err.message || err}`)
    }
  }

  const confirmDelete = async () => {
    if (!deleting) return
    const r = await window.api.invoices.delete(deleting.id)
    toast.success(r?.released_entries
      ? `${deleting.invoice_number} deleted. ${formatHoursShort(r.released_hours)} of time is unbilled again.`
      : `${deleting.invoice_number} deleted`)
    setDeleting(null)
    refresh()
  }

  const confirmDontBill = async () => {
    if (!dontBill?.project_id) return
    const entries: TimeEntry[] = await window.api.time.list(dontBill.project_id)
    // Finished time only, so a running timer on this project isn't touched
    const ids = entries.filter(e => e.end_time && e.billing_state === 'unbilled').map(e => e.id)
    await window.api.time.setBillable(ids, false)
    toast.success(`${dontBill.title}: ${ids.length} entr${ids.length === 1 ? 'y' : 'ies'} marked don't bill`)
    setDontBill(null)
    refresh()
  }

  const discardTimer = async (item: AttentionItem) => {
    if (!item.entry_id) return
    await window.api.time.delete(item.entry_id)
    toast.success('Accidental timer removed')
    refresh()
  }

  const dismiss = async (item: AttentionItem) => {
    await window.api.billing.dismiss(item.key)
    refresh()
  }

  const exportCsv = async () => {
    if (visible.length === 0) return toast.error('No invoices to export')
    const rows = visible.map(i => ({
      invoice_number: i.invoice_number,
      client: i.client_name || '',
      project: i.project_name || '',
      status: i.status,
      issue_date: i.issue_date,
      due_date: i.due_date,
      sent_date: i.sent_at || '',
      payment_date: i.payment_date || '',
      payment_method: i.payment_method || '',
      hours: (i.entry_hours || 0).toFixed(2),
      subtotal: (i.subtotal || 0).toFixed(2),
      gst_hst: i.gst_hst_applicable ? (i.gst_hst_amount || 0).toFixed(2) : '',
      total: (i.total || 0).toFixed(2),
      currency: i.currency || 'CAD',
    }))
    const result = await window.api.reports.exportCSV(rows, year === 'all' ? 'invoices.csv' : `invoices-${year}.csv`)
    if (result) toast.success('Invoices exported')
  }

  const mergeFor = (ids: number[]) => {
    const weight = (id: number) => {
      const c = clients.find(x => x.id === id)
      return (c?.invoice_count || 0) * 10 + (c?.project_count || 0)
    }
    const [a, b] = ids
    return weight(a) >= weight(b) ? { source: b, target: a } : { source: a, target: b }
  }

  const attentionActions = (item: AttentionItem) => {
    switch (item.kind) {
      case 'unbilled':
        return (
          <>
            <button onClick={() => setDontBill(item)} className="btn-ghost btn-sm">Don't bill</button>
            <button onClick={() => navigate(`/invoices/new?project_id=${item.project_id}`)} className="btn-secondary btn-sm">Create invoice</button>
          </>
        )
      case 'overdue': {
        const inv = invoices.find(i => i.id === item.invoice_id)
        return (
          <>
            <button onClick={() => navigate(`/invoices/${item.invoice_id}`)} className="btn-ghost btn-sm">Open</button>
            {inv && <button onClick={() => setPaying([inv])} className="btn-secondary btn-sm">Record payment</button>}
          </>
        )
      }
      case 'draft':
        return (
          <>
            <button onClick={() => navigate(`/invoices/${item.invoice_id}`)} className="btn-ghost btn-sm">Open</button>
            <button onClick={() => markSent([item.invoice_id!])} className="btn-secondary btn-sm">Mark as sent</button>
          </>
        )
      case 'accidental_timer':
        return (
          <>
            <button onClick={() => dismiss(item)} className="btn-ghost btn-sm">Keep it</button>
            <button onClick={() => discardTimer(item)} className="btn-secondary btn-sm">Remove</button>
          </>
        )
      case 'duplicate_clients':
        return (
          <>
            <button onClick={() => dismiss(item)} className="btn-ghost btn-sm">They're different</button>
            <button onClick={() => setMerge(mergeFor(item.client_ids!))} className="btn-secondary btn-sm">Merge</button>
          </>
        )
    }
  }

  return (
    <div className="page">
      <PageHeader
        title="Billing"
        actions={
          <>
            <button onClick={exportCsv} className="btn-secondary">Export</button>
            <button onClick={() => navigate('/invoices/new')} className="btn-secondary">New invoice</button>
          </>
        }
      />

      {/* The money pipeline */}
      <MetricStrip className="mb-6">
        <Metric
          label="Ready to bill"
          icon={<IconHourglass />}
          tint="orange"
          value={<Money animate amount={pipeline.unbilled_amount} />}
          sub={pipeline.unbilled_hours > 0 ? `${formatHoursShort(pipeline.unbilled_hours)} across ${pipeline.unbilled_projects} project${pipeline.unbilled_projects === 1 ? '' : 's'}` : 'All caught up'}
          onClick={() => readyRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' })}
        />
        <Metric
          label="Drafts"
          icon={<IconReceipt />}
          tint="purple"
          value={<Money animate amount={pipeline.draft_amount} />}
          sub={`${pipeline.draft_count} not sent yet`}
          onClick={() => showInvoices('draft')}
          active={filter === 'draft'}
        />
        <Metric
          label="Awaiting payment"
          icon={<IconSend />}
          tint="blue"
          value={<Money animate amount={pipeline.awaiting_amount} />}
          sub={`${pipeline.awaiting_count} invoice${pipeline.awaiting_count === 1 ? '' : 's'} sent`}
          onClick={() => showInvoices('sent')}
          active={filter === 'sent'}
        />
        <Metric
          label="Overdue"
          icon={<IconAlert />}
          tint="red"
          value={<Money animate amount={pipeline.overdue_amount} className={pipeline.overdue_amount > 0 ? 'text-red' : ''} />}
          sub={pipeline.overdue_count ? `${pipeline.overdue_count} past due` : 'Nothing late'}
          onClick={() => showInvoices('overdue')}
          active={filter === 'overdue'}
        />
        <Metric
          label={`Paid in ${year0}`}
          icon={<IconTrophy />}
          tint="green"
          value={<Money animate amount={pipeline.paid_ytd_amount} />}
          sub={`${pipeline.paid_ytd_count} invoice${pipeline.paid_ytd_count === 1 ? '' : 's'}`}
          onClick={() => showInvoices('paid')}
          active={filter === 'paid'}
        />
      </MetricStrip>

      {/* Needs attention */}
      {attention.length > 0 && (
        <section className="mb-6">
          <div className="group-head">
            <h2 className="section-title">
              Needs attention
              <span className="ml-2 align-middle inline-flex items-center justify-center min-w-[20px] h-[20px] px-1.5 rounded-full bg-red text-white text-[11.5px] font-bold">{attention.length}</span>
            </h2>
          </div>
          <div className="card overflow-hidden">
            {attention.map(item => {
              const Icon = ATTENTION_ICON[item.kind]
              return (
                <div key={item.key} className="list-row [--inset:60px] flex items-center gap-3.5 px-4 h-[60px]">
                  <Icon className="w-8 h-8 shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="text-[13px] font-medium text-fg truncate">{item.title}</div>
                    <div className="text-xs text-fg-3 truncate">{item.detail}</div>
                  </div>
                  {item.amount !== undefined && item.amount !== null && (
                    <div className="text-[13px] num text-fg-2 w-24 text-right">{formatMoney(item.amount)}</div>
                  )}
                  <div className="flex items-center gap-1.5 shrink-0">{attentionActions(item)}</div>
                </div>
              )
            })}
          </div>
        </section>
      )}

      {/* Ready to bill */}
      <section ref={readyRef} className="mb-6 scroll-mt-4">
        <div className="group-head justify-start">
          <h2 className="section-title">Ready to bill</h2>
          {pipeline.unbilled_amount > 0 && <span className="text-xs text-fg-3">Tracked time that isn't on an invoice yet</span>}
        </div>
        {ready.length === 0 ? (
          <div className="card">
            <EmptyState compact icon={CheckCircle2} title="Everything is billed" description="New billable time shows up here until it's on an invoice." />
          </div>
        ) : (
          <div className="card overflow-hidden">
            {ready.map(group => (
              <div key={group.client_id} className="border-b border-line last:border-b-0">
                <div className="flex items-center gap-3 px-4 h-[48px]">
                  <ClientAvatar name={group.client_name} size={26} />
                  <div className="min-w-0 flex-1">
                    <button onClick={() => navigate(`/clients/${group.client_id}`)} className="text-[13px] font-semibold text-fg hover:underline underline-offset-2">
                      {group.client_name}
                    </button>
                    <div className="text-xs text-fg-3">
                      {group.projects.length} project{group.projects.length === 1 ? '' : 's'} · {formatHoursShort(group.hours)} · oldest {formatDay(group.oldest_day)}
                    </div>
                  </div>
                  <div className="text-[13px] num font-semibold text-fg w-24 text-right">{formatMoney(group.amount)}</div>
                  <div className="w-[118px] flex justify-end shrink-0">
                    <button onClick={() => navigate(`/invoices/new?client_id=${group.client_id}`)} className="btn-secondary btn-sm">
                      Invoice all
                    </button>
                  </div>
                </div>
                <div className="pb-1.5">
                  {group.projects.map(p => {
                    const stale = daysSince(p.oldest_day) >= 30
                    return (
                      <div key={p.project_id} className="group flex items-center gap-3 pl-[55px] pr-4 h-[36px] hover:bg-fg/[0.025]">
                        <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: p.project_color }} />
                        <button onClick={() => navigate(`/projects/${p.project_id}`)} className="text-[13px] text-fg-2 hover:text-fg truncate text-left">
                          {p.project_name}
                        </button>
                        {(p.project_status === 'complete' || p.project_status === 'archived') && <StatusBadge status={p.project_status} />}
                        <span className="flex-1" />
                        <span className={`text-xs ${stale ? 'text-amber' : 'text-fg-3'}`}>since {formatDay(p.oldest_day)}</span>
                        <span className="text-xs text-fg-3 num w-14 text-right">{formatHoursShort(p.hours)}</span>
                        <span className="text-[13px] num text-fg-2 w-24 text-right">{formatMoney(p.amount)}</span>
                        <div className="w-[118px] flex justify-end shrink-0">
                          <button
                            onClick={() => navigate(`/invoices/new?project_id=${p.project_id}`)}
                            className="btn-ghost btn-sm opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                          >
                            Invoice
                          </button>
                        </div>
                      </div>
                    )
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Invoices */}
      <section ref={invoicesRef} className="scroll-mt-4">
        <div className="flex items-center gap-3 mb-2">
          <h2 className="section-title">Invoices</h2>
          <Segmented
            value={filter}
            onChange={setFilter}
            options={[
              { value: 'all', label: 'All', count: counts.all },
              { value: 'draft', label: 'Drafts', count: counts.draft },
              { value: 'sent', label: 'Awaiting', count: counts.sent },
              { value: 'overdue', label: 'Overdue', count: counts.overdue },
              { value: 'paid', label: 'Paid', count: counts.paid },
            ]}
          />
          <div className="ml-auto flex items-center gap-2">
            <select className="input w-28" value={String(year)} onChange={e => setYear(e.target.value === 'all' ? 'all' : parseInt(e.target.value))}>
              <option value="all">All years</option>
              {years.map(y => <option key={y} value={y}>{y}</option>)}
            </select>
            <SearchInput value={search} onChange={setSearch} placeholder="Search invoices" className="w-60" />
          </div>
        </div>

        {invoices.length === 0 ? (
          <div className="card">
            <EmptyState
              icon={FileText}
              title="No invoices yet"
              description="Create one from your unbilled time. The time you pick is linked to the invoice."
              action={{ label: 'New invoice', onClick: () => navigate('/invoices/new') }}
            />
          </div>
        ) : (
          <div className="card overflow-hidden">
            <table className="tbl group/table">
              <thead>
                <tr>
                  <th className="w-8">
                    <input
                      type="checkbox"
                      checked={allVisibleSelected}
                      ref={el => { if (el) el.indeterminate = selected.size > 0 && !allVisibleSelected }}
                      onChange={() => setSelected(allVisibleSelected ? new Set() : new Set(visible.map(i => i.id)))}
                      className={selected.size > 0 ? '' : 'opacity-0 group-hover/table:opacity-100 focus-visible:opacity-100'}
                      aria-label="Select all"
                    />
                  </th>
                  <th>Invoice</th>
                  <th>Client</th>
                  <th>Issued</th>
                  <th>Due / paid</th>
                  <th>Status</th>
                  <th className="text-right">Amount</th>
                  <th className="w-10"></th>
                </tr>
              </thead>
              <tbody>
                {visible.map(inv => {
                  const isSel = selected.has(inv.id)
                  return (
                    <tr key={inv.id} onClick={() => navigate(`/invoices/${inv.id}`)} className={`row-hover group ${isSel ? 'bg-accent/[0.05]' : ''}`}>
                      <td onClick={e => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          checked={isSel}
                          onChange={() => toggle(inv.id)}
                          className={isSel ? '' : 'opacity-0 group-hover:opacity-100'}
                          aria-label={`Select ${inv.invoice_number}`}
                        />
                      </td>
                      <td>
                        <div className="text-[13px] font-medium text-fg">{inv.invoice_number}</div>
                        <div className="text-xs text-fg-3 truncate max-w-[220px]">
                          {inv.project_name || (inv.project_count && inv.project_count > 1 ? `${inv.project_count} projects` : 'No project')}
                          {(inv.entry_hours || 0) > 0 && ` · ${formatHoursShort(inv.entry_hours || 0)}`}
                        </div>
                      </td>
                      <td className="text-fg-2 truncate max-w-[180px]">{inv.client_name}</td>
                      <td className="text-fg-2 whitespace-nowrap">{formatDay(inv.issue_date)}</td>
                      <td className="whitespace-nowrap">
                        {inv.status === 'paid' ? (
                          <span className="text-fg-2">Paid {formatDay(inv.payment_date || inv.issue_date)}</span>
                        ) : inv.status === 'overdue' ? (
                          <span className="text-red">{inv.days_past_due} day{inv.days_past_due === 1 ? '' : 's'} late</span>
                        ) : inv.status === 'draft' ? (
                          <span className="text-fg-3">Not sent</span>
                        ) : (
                          <span className="text-fg-2">Due {formatDay(inv.due_date)}</span>
                        )}
                      </td>
                      <td><StatusBadge status={inv.status} /></td>
                      <td className="text-right num font-medium text-fg">{formatMoney(inv.total)}</td>
                      <td onClick={e => e.stopPropagation()}>
                        <Menu
                          items={[
                            { label: 'Open', icon: ExternalLink, onClick: () => navigate(`/invoices/${inv.id}`) },
                            { label: 'Download PDF', icon: Download, onClick: () => downloadPdf(inv) },
                            inv.status === 'draft' && { label: 'Mark as sent', icon: Send, onClick: () => markSent([inv.id]) },
                            inv.status !== 'paid' && { label: 'Record payment…', icon: CreditCard, onClick: () => setPaying([inv]) },
                            'separator',
                            { label: 'Delete', icon: Trash2, danger: true, onClick: () => setDeleting(inv) },
                          ]}
                        />
                      </td>
                    </tr>
                  )
                })}
                {visible.length === 0 && (
                  <tr><td colSpan={8} className="text-center text-fg-3 !h-20">No invoices match these filters</td></tr>
                )}
              </tbody>
            </table>
          </div>
        )}
      </section>

      {/* Bulk actions */}
      <AnimatePresence>
        {selectedInvoices.length > 0 && (
          <motion.div
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 12 }}
            transition={{ duration: 0.15 }}
            className="fixed bottom-5 left-[calc(50%+110px)] -translate-x-1/2 z-40 flex items-center gap-3 pl-4 pr-2 h-11 rounded-[10px] bg-panel shadow-pop"
          >
            <span className="text-[13px] font-medium text-fg">{selectedInvoices.length} selected</span>
            <span className="text-[13px] num text-fg-3">{formatMoney(selectedTotal)}</span>
            <div className="w-px h-5 bg-line" />
            {canSend && (
              <button onClick={() => markSent(selectedInvoices.filter(i => i.status === 'draft').map(i => i.id))} className="btn-secondary btn-sm">
                Mark as sent
              </button>
            )}
            {canPay && (
              <button onClick={() => setPaying(selectedInvoices.filter(i => i.status !== 'paid'))} className="btn-primary btn-sm">
                Record payment
              </button>
            )}
            <button onClick={() => setSelected(new Set())} className="btn-icon-sm" aria-label="Clear selection"><X /></button>
          </motion.div>
        )}
      </AnimatePresence>

      <RecordPaymentModal
        invoices={paying}
        onClose={() => setPaying([])}
        onDone={() => { setPaying([]); setSelected(new Set()) }}
      />

      <ConfirmDialog
        isOpen={!!deleting}
        onClose={() => setDeleting(null)}
        onConfirm={confirmDelete}
        title={`Delete ${deleting?.invoice_number}?`}
        message={
          <>
            {deleting?.status === 'paid' && <p className="mb-2 text-red">This invoice is paid. Deleting it also removes the payment from your income totals.</p>}
            {(deleting?.entry_count || 0) > 0
              ? <p>Its {deleting?.entry_count} time entr{deleting?.entry_count === 1 ? 'y' : 'ies'} ({formatHoursShort(deleting?.entry_hours || 0)}) go back to unbilled so you can bill them again.</p>
              : <p>This can't be undone.</p>}
          </>
        }
        confirmText="Delete invoice"
      />

      <ConfirmDialog
        isOpen={!!dontBill}
        onClose={() => setDontBill(null)}
        onConfirm={confirmDontBill}
        title={`Don't bill ${dontBill?.title}?`}
        message={<>Its unbilled time ({formatMoney(dontBill?.amount || 0)}) is marked non-billable. It stays in your time log, just not as money owed. You can change entries back from the Time page.</>}
        confirmText="Don't bill it"
        danger={false}
      />

      <MergeClientsModal
        open={!!merge}
        clients={clients}
        sourceId={merge?.source ?? null}
        targetId={merge?.target ?? null}
        onClose={() => setMerge(null)}
        onMerged={() => { setMerge(null); refresh() }}
      />
    </div>
  )
}

