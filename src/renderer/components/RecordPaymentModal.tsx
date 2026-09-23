import { useEffect, useState } from 'react'
import Modal from './Modal'
import { formatMoney, todayISO, formatDay } from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import type { Invoice, PaymentMethod } from '@shared/types'
import toast from 'react-hot-toast'

const EXTRA_METHODS = ['e-Transfer', 'Cheque', 'Cash', 'Credit card', 'Bank transfer', 'PayPal', 'Other']

/** Record payment for one or several invoices with one date and method. */
export default function RecordPaymentModal({ invoices, onClose, onDone }: {
  invoices: Invoice[]
  onClose: () => void
  onDone: () => void
}) {
  const open = invoices.length > 0
  const [date, setDate] = useState(todayISO())
  const [method, setMethod] = useState('')
  const [methods, setMethods] = useState<string[]>([])
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!open) return
    setDate(todayISO())
    window.api.settings.get().then((s: any) => {
      let saved: PaymentMethod[] = []
      try { saved = JSON.parse(s.payment_methods || '[]') } catch { /* ignore */ }
      const names = Array.from(new Set([...saved.map(m => m.name).filter(Boolean), ...EXTRA_METHODS]))
      setMethods(names)
      setMethod(s.default_payment_method || names[0] || '')
    })
  }, [open])

  const total = invoices.reduce((s, i) => s + (Number(i.total) || 0), 0)
  const single = invoices.length === 1 ? invoices[0] : null

  const save = async () => {
    setSaving(true)
    try {
      const n = await window.api.invoices.markPaid(invoices.map(i => i.id), date, method || null)
      toast.success(single ? `${single.invoice_number} marked as paid` : `${n} invoices marked as paid`)
      notifyBillingChanged()
      onDone()
    } catch (err: any) {
      toast.error(`Couldn't record payment: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      isOpen={open}
      onClose={onClose}
      title={single ? `Record payment for ${single.invoice_number}` : `Record payment for ${invoices.length} invoices`}
      size="sm"
      footer={
        <>
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button onClick={save} disabled={saving || !date} className="btn-primary">{saving ? 'Saving…' : `Mark ${formatMoney(total)} paid`}</button>
        </>
      }
    >
      <div className="space-y-4">
        {!single && (
          <div className="rounded-[8px] border border-line divide-y divide-line max-h-40 overflow-y-auto">
            {invoices.map(i => (
              <div key={i.id} className="flex items-center justify-between px-3 h-[32px] text-[13px]">
                <span className="text-fg">{i.invoice_number} <span className="text-fg-3">· {i.client_name}</span></span>
                <span className="num text-fg-2">{formatMoney(i.total)}</span>
              </div>
            ))}
          </div>
        )}
        {single && (
          <p className="text-sm text-fg-2">
            {single.client_name} · issued {formatDay(single.issue_date)} · <span className="num font-medium text-fg">{formatMoney(single.total)}</span>
          </p>
        )}
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label">Date received</label>
            <input className="input" type="date" value={date} onChange={e => setDate(e.target.value)} />
          </div>
          <div>
            <label className="label">Method</label>
            <select className="input" value={method} onChange={e => setMethod(e.target.value)}>
              {methods.map(m => <option key={m} value={m}>{m}</option>)}
            </select>
          </div>
        </div>
        <p className="hint !mt-0">Use the day the money arrived. Tax Overview counts income in that month.</p>
      </div>
    </Modal>
  )
}
