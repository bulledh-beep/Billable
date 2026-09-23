import { useState, useEffect, useMemo } from 'react'
import { useParams, useNavigate, Link } from 'react-router-dom'
import { Trash2, Pencil, Undo2, AlertTriangle, CheckCircle2, Circle, Mail, MapPin } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import StatusBadge from '../components/StatusBadge'
import Stamp from '../components/Stamp'
import ConfirmDialog from '../components/ConfirmDialog'
import RecordPaymentModal from '../components/RecordPaymentModal'
import Menu from '../components/Menu'
import { formatMoney, formatDate, formatDay, formatHoursShort, formatDurationShort, toLocalISODate } from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import type { Invoice, Settings, TaxSettings } from '@shared/types'
import toast from 'react-hot-toast'

/** What the time-based lines should add up to, grouped the way the builder groups them. */
function expectedHoursForLines(entries: Invoice['entries'], style: Invoice['line_style']): number | null {
  if (!entries || !style || style === 'manual') return null
  const groups = new Map<string, number>()
  for (const e of entries) {
    const day = toLocalISODate(new Date(e.start_time))
    const key = style === 'entry' ? `e${e.id}` : style === 'project' ? `p${e.project_id}` : `d${day}:${e.project_id}`
    groups.set(key, (groups.get(key) || 0) + e.duration_minutes)
  }
  let total = 0
  for (const mins of groups.values()) total += Math.round((mins / 60) * 100) / 100
  return Math.round(total * 100) / 100
}

export default function InvoiceDetail() {
  const { id } = useParams()
  const navigate = useNavigate()
  const invoiceId = id ? parseInt(id) : 0
  const [invoice, setInvoice] = useState<Invoice | null>(null)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [taxSettings, setTaxSettings] = useState<TaxSettings | null>(null)
  const [showDelete, setShowDelete] = useState(false)
  const [paying, setPaying] = useState(false)
  const [exporting, setExporting] = useState(false)

  useEffect(() => { if (invoiceId) load() }, [invoiceId])

  const load = async () => {
    const [inv, s, tax] = await Promise.all([
      window.api.invoices.get(invoiceId),
      window.api.settings.get(),
      window.api.tax.getSettings(),
    ])
    if (!inv) { navigate('/billing'); return }
    setInvoice(inv)
    setSettings(s)
    setTaxSettings(tax)
  }

  const entriesByProject = useMemo(() => {
    const map = new Map<string, { name: string; color: string; minutes: number; count: number }>()
    for (const e of invoice?.entries || []) {
      const key = String(e.project_id)
      const g = map.get(key) || { name: e.project_name || 'Project', color: e.project_color || '#888', minutes: 0, count: 0 }
      g.minutes += e.duration_minutes
      g.count += 1
      map.set(key, g)
    }
    return Array.from(map.values())
  }, [invoice])

  if (!invoice || !settings) return null

  const currency = invoice.currency || taxSettings?.currency || 'CAD'
  const money = (n: number) => formatMoney(n, currency)
  const items = invoice.items || []
  const entries = invoice.entries || []
  const linkedHours = entries.reduce((s, e) => s + e.duration_minutes, 0) / 60
  const lineHours = items.filter(i => i.source_key).reduce((s, i) => s + i.quantity, 0)
  const timeLines = items.some(i => i.source_key)
  // Lines round each group to two decimals, so compare like with like
  const expectedLineHours = expectedHoursForLines(entries, invoice.line_style)
  const mismatch = timeLines && expectedLineHours !== null && Math.abs(expectedLineHours - lineHours) > 0.001
  const gstAmount = invoice.gst_hst_applicable ? invoice.subtotal * ((invoice.gst_hst_rate || 0) / 100) : 0
  const otherTax = invoice.subtotal * ((invoice.tax_rate || 0) / 100)
  const businessName = settings.business_name || taxSettings?.business_name || 'Your business'
  const businessAddress = settings.business_address || taxSettings?.business_address || ''

  const markSent = async () => {
    await window.api.invoices.markSent([invoice.id])
    toast.success(`${invoice.invoice_number} marked as sent`)
    notifyBillingChanged()
    load()
  }

  const markUnpaid = async () => {
    await window.api.invoices.markUnpaid(invoice.id)
    toast.success('Payment removed. The invoice is back to awaiting payment.')
    notifyBillingChanged()
    load()
  }

  const exportPdf = async () => {
    setExporting(true)
    try {
      const path = await window.api.invoices.exportPDF(invoice.id)
      if (path) toast.success('PDF saved')
    } catch (err: any) {
      toast.error(`Couldn't create the PDF: ${err.message || err}`)
    }
    setExporting(false)
  }

  const handleDelete = async () => {
    const r = await window.api.invoices.delete(invoice.id)
    toast.success(r?.released_entries
      ? `${invoice.invoice_number} deleted. ${formatHoursShort(r.released_hours)} of time is unbilled again.`
      : `${invoice.invoice_number} deleted`)
    notifyBillingChanged()
    navigate('/billing')
  }

  const created = invoice.created_at ? toLocalISODate(new Date(String(invoice.created_at).replace(' ', 'T') + 'Z')) : invoice.issue_date
  const timeline: Array<{ label: string; date?: string | null; done: boolean; tone?: 'red' | 'green' }> = [
    { label: 'Created', date: created, done: true },
    { label: 'Sent', date: invoice.sent_at, done: invoice.status !== 'draft' },
    invoice.status === 'overdue'
      ? { label: `Overdue by ${invoice.days_past_due} day${invoice.days_past_due === 1 ? '' : 's'}`, date: invoice.due_date, done: true, tone: 'red' }
      : { label: 'Due', date: invoice.due_date, done: invoice.status === 'paid' },
    { label: invoice.payment_method ? `Paid by ${invoice.payment_method}` : 'Paid', date: invoice.payment_date, done: invoice.status === 'paid', tone: 'green' },
  ]

  const stampDate = (d?: string | null) => (d ? formatDate(d).toUpperCase().replace(',', '') : undefined)
  const stamp = invoice.status === 'paid'
    ? { label: 'Paid', detail: stampDate(invoice.payment_date), tone: 'green' as const }
    : invoice.status === 'overdue'
      ? { label: 'Overdue', detail: `${invoice.days_past_due} day${invoice.days_past_due === 1 ? '' : 's'} late`, tone: 'red' as const }
      : invoice.status === 'draft'
        ? { label: 'Draft', detail: 'Not sent', tone: 'gray' as const }
        : null

  return (
    <div className="page">
      <PageHeader
        crumbs={[{ label: 'Billing', to: '/billing' }]}
        title={invoice.invoice_number}
        subtitle={invoice.client_name}
        meta={<StatusBadge status={invoice.status} />}
        actions={
          <>
            <button onClick={exportPdf} disabled={exporting} className="btn-secondary">{exporting ? 'Saving…' : 'Save PDF'}</button>
            {invoice.status === 'draft' && <button onClick={() => navigate(`/invoices/${invoice.id}/edit`)} className="btn-secondary">Edit</button>}
            <Menu
              items={[
                invoice.status !== 'draft' && { label: 'Edit invoice', icon: Pencil, onClick: () => navigate(`/invoices/${invoice.id}/edit`) },
                invoice.status === 'paid' && { label: 'Mark as unpaid', icon: Undo2, onClick: markUnpaid },
                'separator',
                { label: 'Delete invoice', icon: Trash2, danger: true, onClick: () => setShowDelete(true) },
              ]}
            />
          </>
        }
      />

      <div className="grid grid-cols-[minmax(0,1fr)_300px] gap-6 items-start">
        {/* Paper preview, matching the PDF */}
        <div className="relative rounded-[3px] overflow-hidden bg-white text-[#18181b] shadow-[0_0_0_0.5px_rgb(0_0_0/0.12),0_2px_4px_rgb(0_0_0/0.06),0_12px_32px_-12px_rgb(0_0_0/0.25)]">
          {stamp && <Stamp {...stamp} className="absolute top-[34px] right-[190px]" />}
          <div className="px-10 py-9">
            <div className="flex justify-between gap-8">
              <div className="min-w-0">
                <div className="text-lg font-semibold">{businessName}</div>
                {businessAddress && <div className="text-sm text-[#52525b] whitespace-pre-line mt-1">{businessAddress}</div>}
                {settings.business_email && <div className="text-sm text-[#52525b]">{settings.business_email}</div>}
                {invoice.gst_hst_applicable && invoice.gst_hst_number ? <div className="text-sm text-[#52525b]">GST/HST {invoice.gst_hst_number}</div> : null}
              </div>
              <div className="text-right shrink-0">
                <div className="text-2xl font-semibold tracking-tight">Invoice</div>
                <div className="text-sm text-[#52525b] num mt-0.5">{invoice.invoice_number}</div>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-8 mt-9">
              <div>
                <div className="text-2xs font-semibold uppercase tracking-[0.08em] text-[#a1a1aa] mb-1.5">Bill to</div>
                <div className="text-sm font-semibold">{invoice.client_name}</div>
                {invoice.client_company && invoice.client_company !== invoice.client_name && <div className="text-sm text-[#52525b]">{invoice.client_company}</div>}
                {invoice.client_address && <div className="text-sm text-[#52525b] whitespace-pre-line">{invoice.client_address}</div>}
                {invoice.client_email && <div className="text-sm text-[#52525b]">{invoice.client_email}</div>}
              </div>
              <div className="text-sm space-y-1 justify-self-end min-w-[220px]">
                <div className="flex justify-between gap-6"><span className="text-[#71717a]">Issue date</span><span className="num">{formatDate(invoice.issue_date)}</span></div>
                <div className="flex justify-between gap-6"><span className="text-[#71717a]">Due date</span><span className="num">{formatDate(invoice.due_date)}</span></div>
                <div className="flex justify-between gap-6 pt-2 mt-1 border-t border-[#e4e4e7]">
                  <span className="font-semibold">{invoice.status === 'paid' ? 'Amount paid' : 'Amount due'}</span>
                  <span className="num font-semibold">{money(invoice.total)}</span>
                </div>
              </div>
            </div>

            <table className="w-full mt-9 text-sm">
              <thead>
                <tr className="border-b border-[#18181b]">
                  <th className="text-left pb-2 text-2xs font-semibold uppercase tracking-[0.08em] text-[#71717a]">Description</th>
                  <th className="text-right pb-2 text-2xs font-semibold uppercase tracking-[0.08em] text-[#71717a] w-20">Hours</th>
                  <th className="text-right pb-2 text-2xs font-semibold uppercase tracking-[0.08em] text-[#71717a] w-24">Rate</th>
                  <th className="text-right pb-2 text-2xs font-semibold uppercase tracking-[0.08em] text-[#71717a] w-28">Amount</th>
                </tr>
              </thead>
              <tbody>
                {items.map((item, i) => (
                  <tr key={i} className="border-b border-[#f0f0f2]">
                    <td className="py-2.5 pr-4">{item.description}</td>
                    <td className="py-2.5 text-right num text-[#52525b]">{Number(item.quantity).toFixed(2)}</td>
                    <td className="py-2.5 text-right num text-[#52525b]">{money(item.unit_price)}</td>
                    <td className="py-2.5 text-right num">{money(item.total)}</td>
                  </tr>
                ))}
              </tbody>
            </table>

            <div className="flex justify-end mt-5">
              <div className="w-72 text-sm space-y-1.5">
                <div className="flex justify-between"><span className="text-[#71717a]">Subtotal</span><span className="num">{money(invoice.subtotal)}</span></div>
                {invoice.gst_hst_applicable && (invoice.gst_hst_rate || 0) > 0 ? (
                  <div className="flex justify-between"><span className="text-[#71717a]">GST/HST ({invoice.gst_hst_rate}%)</span><span className="num">{money(gstAmount)}</span></div>
                ) : null}
                {invoice.tax_rate > 0 && (
                  <div className="flex justify-between"><span className="text-[#71717a]">Tax ({invoice.tax_rate}%)</span><span className="num">{money(otherTax)}</span></div>
                )}
                <div className="flex justify-between pt-2.5 mt-1 border-t border-[#18181b] text-base font-semibold">
                  <span>Total</span><span className="num">{money(invoice.total)}</span>
                </div>
                {invoice.status === 'paid' && invoice.payment_date && (
                  <div className="flex justify-between text-[#15803d] text-xs pt-1">
                    <span>Paid {formatDate(invoice.payment_date)}{invoice.payment_method ? ` by ${invoice.payment_method}` : ''}</span>
                    <span className="num">{money(invoice.total)}</span>
                  </div>
                )}
              </div>
            </div>

            {invoice.notes && (
              <div className="mt-9 pt-4 border-t border-[#e4e4e7]">
                <div className="text-2xs font-semibold uppercase tracking-[0.08em] text-[#a1a1aa] mb-1.5">Notes</div>
                <p className="text-sm text-[#52525b] whitespace-pre-line">{invoice.notes}</p>
              </div>
            )}
          </div>
        </div>

        {/* Side panel */}
        <div className="space-y-4">
          <div className="card p-4">
            <div className="text-xs text-fg-3">{invoice.status === 'paid' ? 'Paid' : 'Amount due'}</div>
            <div className="font-figures text-[30px] leading-[34px] text-fg mt-1.5">{money(invoice.total)}</div>
            {invoice.status === 'draft' && (
              <button onClick={markSent} className="btn-primary w-full mt-3">Mark as sent</button>
            )}
            {(invoice.status === 'sent' || invoice.status === 'overdue') && (
              <button onClick={() => setPaying(true)} className="btn-primary w-full mt-3">Record payment</button>
            )}
            <div className="mt-4 space-y-3">
              {timeline.map((step, i) => (
                <div key={i} className="flex items-start gap-2.5">
                  {step.done
                    ? <CheckCircle2 className={`w-4 h-4 mt-px shrink-0 ${step.tone === 'red' ? 'text-red' : step.tone === 'green' ? 'text-green' : 'text-fg-2'}`} />
                    : <Circle className="w-4 h-4 mt-px shrink-0 text-fg-4" />}
                  <div className="min-w-0">
                    <div className={`text-sm ${step.done ? (step.tone === 'red' ? 'text-red' : 'text-fg') : 'text-fg-3'}`}>{step.label}</div>
                    {step.date && <div className="text-xs text-fg-3">{formatDay(step.date)}</div>}
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="card overflow-hidden">
            <div className="card-header">
              <span className="card-title">Time on this invoice</span>
              {entries.length > 0 && <span className="text-xs text-fg-3 num">{formatHoursShort(linkedHours)}</span>}
            </div>
            {entries.length === 0 ? (
              <p className="px-4 py-4 text-sm text-fg-3">
                {invoice.line_style ? 'No time is linked.' : 'This invoice was written by hand, so no time is linked to it.'}
              </p>
            ) : (
              <div className="py-1.5">
                {entriesByProject.map(g => (
                  <div key={g.name} className="flex items-center gap-2.5 px-4 h-[32px]">
                    <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: g.color }} />
                    <span className="text-[13px] text-fg-2 truncate flex-1">{g.name}</span>
                    <span className="text-xs text-fg-3 num">{g.count} · {formatDurationShort(g.minutes)}</span>
                  </div>
                ))}
                <div className="px-4 pt-1.5 pb-1 text-xs text-fg-3">
                  {formatDay(entries[0].start_time)}{entries.length > 1 ? ` – ${formatDay(entries[entries.length - 1].start_time)}` : ''}
                </div>
              </div>
            )}
            {mismatch && (
              <div className="mx-3 mb-3 p-2.5 rounded-md bg-amber/10 text-xs text-fg-2 flex gap-2">
                <AlertTriangle className="w-3.5 h-3.5 text-amber shrink-0 mt-px" />
                <div>
                  The linked time adds up to {formatHoursShort(linkedHours)}, but the lines say {formatHoursShort(lineHours)}.
                  {invoice.status === 'draft' && (
                    <> <Link to={`/invoices/${invoice.id}/edit`} className="link">Open the draft</Link> to update the lines.</>
                  )}
                </div>
              </div>
            )}
          </div>

          <div className="card p-4">
            <div className="text-xs text-fg-3 mb-1.5">Client</div>
            <Link to={`/clients/${invoice.client_id}`} className="text-sm font-semibold text-fg hover:underline underline-offset-2">{invoice.client_name}</Link>
            {invoice.client_email && <div className="flex items-center gap-1.5 text-xs text-fg-3 mt-1.5"><Mail className="w-3 h-3" />{invoice.client_email}</div>}
            {invoice.client_address && <div className="flex items-start gap-1.5 text-xs text-fg-3 mt-1"><MapPin className="w-3 h-3 mt-0.5 shrink-0" /><span className="whitespace-pre-line">{invoice.client_address}</span></div>}
            {!invoice.client_address && <div className="flex items-center gap-1.5 text-xs text-amber mt-1.5"><AlertTriangle className="w-3 h-3" />No billing address on file</div>}
          </div>
        </div>
      </div>

      <RecordPaymentModal
        invoices={paying ? [invoice] : []}
        onClose={() => setPaying(false)}
        onDone={() => { setPaying(false); load() }}
      />

      <ConfirmDialog
        isOpen={showDelete}
        onClose={() => setShowDelete(false)}
        onConfirm={handleDelete}
        title={`Delete ${invoice.invoice_number}?`}
        message={
          <>
            {invoice.status === 'paid' && <p className="mb-2 text-red">This invoice is paid. Deleting it also removes the payment from your income totals.</p>}
            {entries.length > 0
              ? <p>Its {entries.length} time entr{entries.length === 1 ? 'y' : 'ies'} ({formatHoursShort(linkedHours)}) go back to unbilled so you can bill them again.</p>
              : <p>This can't be undone.</p>}
          </>
        }
        confirmText="Delete invoice"
      />
    </div>
  )
}
