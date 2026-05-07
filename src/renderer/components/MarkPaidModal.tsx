import { useEffect, useState } from 'react'
import { CreditCard } from 'lucide-react'
import Modal from './Modal'
import type { PaymentMethod } from '@shared/types'
import { todayISO } from '../utils/format'
import toast from 'react-hot-toast'

interface MarkPaidModalProps {
  invoiceId: number | null // null = closed
  invoiceNumber?: string
  onClose: () => void
  onMarked: () => void
}

export default function MarkPaidModal({
  invoiceId,
  invoiceNumber,
  onClose,
  onMarked,
}: MarkPaidModalProps) {
  const [paymentDate, setPaymentDate] = useState<string>(todayISO())
  const [method, setMethod] = useState('')
  const [methods, setMethods] = useState<PaymentMethod[]>([])
  const [saving, setSaving] = useState(false)

  // Reset and load defaults whenever the modal opens for a new invoice
  useEffect(() => {
    if (invoiceId === null) return
    setPaymentDate(todayISO())
    loadDefaults()
  }, [invoiceId])

  const loadDefaults = async () => {
    try {
      const settings = await window.api.settings.get()
      const parsed: PaymentMethod[] = JSON.parse(settings.payment_methods || '[]')
      setMethods(parsed)
      const def = parsed.find(m => m.name === settings.default_payment_method) || parsed[0]
      setMethod(def?.name || '')
    } catch {
      setMethods([])
    }
  }

  const handleConfirm = async () => {
    if (invoiceId === null) return
    setSaving(true)
    try {
      await window.api.invoices.update(invoiceId, {
        status: 'paid',
        payment_date: paymentDate,
        payment_method: method || null,
      })
      toast.success(invoiceNumber ? `${invoiceNumber} marked as paid` : 'Invoice marked as paid')
      onMarked()
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      isOpen={invoiceId !== null}
      onClose={onClose}
      title={invoiceNumber ? `Mark ${invoiceNumber} as Paid` : 'Mark Invoice as Paid'}
      size="sm"
    >
      <div className="space-y-4">
        <div>
          <label className="text-xs font-medium text-text-secondary mb-1.5 block">Payment Date</label>
          <input
            className="input-field"
            type="date"
            value={paymentDate}
            onChange={e => setPaymentDate(e.target.value)}
          />
        </div>
        <div>
          <label className="text-xs font-medium text-text-secondary mb-1.5 block">Payment Method</label>
          {methods.length > 0 ? (
            <select
              className="input-field"
              value={method}
              onChange={e => setMethod(e.target.value)}
            >
              <option value="">Select method…</option>
              {methods.map((m, i) => (
                <option key={i} value={m.name}>{m.name}</option>
              ))}
              <option value="Cheque">Cheque</option>
              <option value="Cash">Cash</option>
              <option value="Credit Card">Credit Card</option>
              <option value="Other">Other</option>
            </select>
          ) : (
            <input
              className="input-field"
              value={method}
              onChange={e => setMethod(e.target.value)}
              placeholder="e.g. e-Transfer, Cheque, PayPal"
            />
          )}
          <p className="text-[10px] text-text-tertiary mt-1">
            Configure default methods in Settings → Payment Methods.
          </p>
        </div>
        <div className="flex justify-end gap-3 pt-2">
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button
            onClick={handleConfirm}
            disabled={saving}
            className={`btn-primary flex items-center gap-2 ${saving ? 'opacity-50' : ''}`}
          >
            <CreditCard className="w-4 h-4" /> {saving ? 'Saving…' : 'Confirm Paid'}
          </button>
        </div>
      </div>
    </Modal>
  )
}
