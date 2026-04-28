import { useState, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowLeft, Download, Send, Check, AlertTriangle, Trash2, Pencil } from 'lucide-react'
import StatusBadge from '../components/StatusBadge'
import ConfirmDialog from '../components/ConfirmDialog'
import { formatMoney, formatDate } from '../utils/format'
import type { Invoice, Settings } from '@shared/types'
import toast from 'react-hot-toast'

export default function InvoiceDetail() {
  const { id } = useParams()
  const navigate = useNavigate()
  const [invoice, setInvoice] = useState<Invoice | null>(null)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [showDelete, setShowDelete] = useState(false)
  const [exporting, setExporting] = useState(false)

  useEffect(() => {
    if (id) loadInvoice(parseInt(id))
    loadSettings()
  }, [id])

  const loadInvoice = async (invoiceId: number) => {
    const inv = await window.api.invoices.get(invoiceId)
    setInvoice(inv)
  }

  const loadSettings = async () => {
    const s = await window.api.settings.get()
    setSettings(s)
  }

  const updateStatus = async (status: string) => {
    if (!invoice) return
    await window.api.invoices.update(invoice.id, { status })
    toast.success(`Invoice marked as ${status}`)
    loadInvoice(invoice.id)
  }

  const handleExportPDF = async () => {
    if (!invoice) return
    setExporting(true)
    try {
      const pdfPath = await window.api.invoices.exportPDF(invoice.id)
      if (pdfPath) {
        toast.success('PDF exported successfully')
      }
    } catch (err: any) {
      console.error('PDF export failed:', err)
      toast.error(`Failed to export PDF: ${err.message || 'Unknown error'}`)
    }
    setExporting(false)
  }

  const handleDelete = async () => {
    if (!invoice) return
    await window.api.invoices.delete(invoice.id)
    toast.success('Invoice deleted')
    navigate('/invoices')
  }

  if (!invoice) return null

  const items = invoice.items || []
  const taxAmount = invoice.subtotal * (invoice.tax_rate / 100)

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="p-8">
      <button
        onClick={() => navigate('/invoices')}
        className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary transition-colors mb-6"
      >
        <ArrowLeft className="w-4 h-4" /> Invoices
      </button>

      <div className="flex items-start justify-between mb-8">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-bold text-text-primary">{invoice.invoice_number}</h1>
            <StatusBadge status={invoice.status} />
          </div>
          <p className="text-sm text-text-secondary mt-1">
            {invoice.client_name} · {invoice.project_name || 'Multiple projects'}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {invoice.status === 'draft' && (
            <button onClick={() => updateStatus('sent')} className="btn-secondary flex items-center gap-2">
              <Send className="w-4 h-4" /> Mark Sent
            </button>
          )}
          {(invoice.status === 'sent' || invoice.status === 'overdue') && (
            <button onClick={() => updateStatus('paid')} className="btn-primary flex items-center gap-2">
              <Check className="w-4 h-4" /> Mark Paid
            </button>
          )}
          {invoice.status === 'sent' && (
            <button onClick={() => updateStatus('overdue')} className="btn-ghost flex items-center gap-2 text-status-overdue">
              <AlertTriangle className="w-4 h-4" /> Mark Overdue
            </button>
          )}
          <button
            onClick={handleExportPDF}
            disabled={exporting}
            className="btn-secondary flex items-center gap-2"
          >
            <Download className="w-4 h-4" /> {exporting ? 'Exporting...' : 'Export PDF'}
          </button>
          <button
            onClick={() => navigate(`/invoices/${invoice.id}/edit`)}
            className="btn-secondary flex items-center gap-2"
            title="Edit invoice"
          >
            <Pencil className="w-4 h-4" /> Edit
          </button>
          <button onClick={() => setShowDelete(true)} className="btn-danger p-2">
            <Trash2 className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Invoice Preview */}
      <div className="glass-panel overflow-hidden">
        {/* Accent Bar */}
        <div className="h-1 bg-accent" />

        <div className="p-8">
          {/* Header */}
          <div className="flex justify-between mb-10">
            <div>
              {settings && (
                <>
                  <h2 className="text-lg font-semibold text-text-primary mb-1">{settings.business_name || 'Your Business'}</h2>
                  {settings.business_address && <p className="text-sm text-text-tertiary whitespace-pre-line">{settings.business_address}</p>}
                  {settings.business_email && <p className="text-sm text-text-tertiary">{settings.business_email}</p>}
                  {settings.tax_id && <p className="text-sm text-text-tertiary">Tax ID: {settings.tax_id}</p>}
                </>
              )}
            </div>
            <div className="text-right">
              <h2 className="text-3xl font-bold text-accent tracking-wider mb-2">INVOICE</h2>
              <p className="text-sm text-text-secondary">{invoice.invoice_number}</p>
              <div className="mt-4 space-y-1 text-sm">
                <div><span className="text-text-tertiary">Issued:</span> <span className="text-text-primary">{formatDate(invoice.issue_date)}</span></div>
                <div><span className="text-text-tertiary">Due:</span> <span className="text-text-primary">{formatDate(invoice.due_date)}</span></div>
              </div>
            </div>
          </div>

          {/* Bill To */}
          <div className="mb-8">
            <h3 className="text-[11px] uppercase tracking-wider text-text-tertiary mb-2">Bill To</h3>
            <p className="text-sm font-semibold text-text-primary">{invoice.client_name}</p>
            {invoice.client_company && <p className="text-sm text-text-secondary">{invoice.client_company}</p>}
            {invoice.client_address && <p className="text-sm text-text-tertiary whitespace-pre-line">{invoice.client_address}</p>}
            {invoice.client_email && <p className="text-sm text-text-tertiary">{invoice.client_email}</p>}
          </div>

          {/* Line Items */}
          <table className="w-full mb-8">
            <thead>
              <tr className="border-b border-white/[0.06]">
                <th className="text-left py-3 text-xs font-medium text-text-tertiary uppercase tracking-wider">Description</th>
                <th className="text-center py-3 text-xs font-medium text-text-tertiary uppercase tracking-wider">Hours</th>
                <th className="text-right py-3 text-xs font-medium text-text-tertiary uppercase tracking-wider">Rate</th>
                <th className="text-right py-3 text-xs font-medium text-text-tertiary uppercase tracking-wider">Amount</th>
              </tr>
            </thead>
            <tbody>
              {items.map((item: any, i: number) => (
                <tr key={i} className="border-b border-white/[0.02]">
                  <td className="py-3 text-sm text-text-primary">{item.description}</td>
                  <td className="py-3 text-sm text-text-secondary font-mono text-center">{item.quantity.toFixed(2)}</td>
                  <td className="py-3 text-sm text-text-secondary font-mono text-right">{formatMoney(item.unit_price)}</td>
                  <td className="py-3 text-sm text-text-primary font-mono text-right font-medium">{formatMoney(item.total)}</td>
                </tr>
              ))}
            </tbody>
          </table>

          {/* Totals */}
          <div className="flex justify-end">
            <div className="w-72 space-y-2">
              <div className="flex justify-between text-sm">
                <span className="text-text-secondary">Subtotal</span>
                <span className="font-mono text-text-primary">{formatMoney(invoice.subtotal)}</span>
              </div>
              {invoice.tax_rate > 0 && (
                <div className="flex justify-between text-sm">
                  <span className="text-text-secondary">Tax ({invoice.tax_rate}%)</span>
                  <span className="font-mono text-text-primary">{formatMoney(taxAmount)}</span>
                </div>
              )}
              <div className="flex justify-between text-xl font-bold pt-3 border-t border-white/[0.06]">
                <span className="text-text-primary">Total</span>
                <span className="font-mono text-accent">{formatMoney(invoice.total)}</span>
              </div>
            </div>
          </div>

          {/* Notes */}
          {invoice.notes && (
            <div className="mt-8 p-4 bg-surface-200/50 rounded-lg">
              <h3 className="text-xs font-medium text-text-tertiary uppercase tracking-wider mb-2">Notes</h3>
              <p className="text-sm text-text-secondary whitespace-pre-line">{invoice.notes}</p>
            </div>
          )}
        </div>
      </div>

      <ConfirmDialog
        isOpen={showDelete}
        onClose={() => setShowDelete(false)}
        onConfirm={handleDelete}
        title="Delete Invoice"
        message="Are you sure you want to delete this invoice?"
      />
    </motion.div>
  )
}
