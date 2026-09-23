import { useEffect, useState } from 'react'
import Modal from './Modal'
import { formatMoney } from '../utils/format'
import type { Client } from '@shared/types'

export interface ClientFormValues {
  name: string
  company: string
  email: string
  address: string
  default_rate: number
  currency: string
}

export default function ClientForm({ open, client, onClose, onSubmit }: {
  open: boolean
  client?: Client | null
  onClose: () => void
  onSubmit: (values: ClientFormValues) => void
}) {
  const [form, setForm] = useState<ClientFormValues>({
    name: '', company: '', email: '', address: '', default_rate: 0, currency: 'CAD',
  })

  useEffect(() => {
    if (!open) return
    if (client) {
      setForm({
        name: client.name,
        company: client.company || '',
        email: client.email || '',
        address: client.address || '',
        default_rate: client.default_rate,
        currency: client.currency || 'CAD',
      })
    } else {
      window.api.settings.get().then((s: any) => {
        setForm({
          name: '', company: '', email: '', address: '',
          default_rate: Number(s.default_rate) || 0,
          currency: s.default_currency || 'CAD',
        })
      })
    }
  }, [open, client])

  const valid = !!form.name.trim()
  const submit = () => { if (valid) onSubmit({ ...form, name: form.name.trim() }) }
  const rateChanged = client && Number(form.default_rate) !== Number(client.default_rate)

  return (
    <Modal
      isOpen={open}
      onClose={onClose}
      title={client ? 'Edit client' : 'New client'}
      footer={
        <>
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button onClick={submit} disabled={!valid} className="btn-primary">{client ? 'Save changes' : 'Create client'}</button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label">Name</label>
            <input className="input" value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} placeholder="Jane Cooper" autoFocus />
          </div>
          <div>
            <label className="label">Company</label>
            <input className="input" value={form.company} onChange={e => setForm(f => ({ ...f, company: e.target.value }))} placeholder="Optional" />
          </div>
        </div>
        <div>
          <label className="label">Email</label>
          <input className="input" type="email" value={form.email} onChange={e => setForm(f => ({ ...f, email: e.target.value }))} placeholder="billing@company.com" />
        </div>
        <div>
          <label className="label">Billing address</label>
          <textarea className="input" rows={2} value={form.address} onChange={e => setForm(f => ({ ...f, address: e.target.value }))} placeholder="Shown on invoices" />
        </div>
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="label">Hourly rate</label>
            <div className="relative">
              <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">$</span>
              <input
                className="input pl-6 num"
                type="number"
                min={0}
                step="0.01"
                value={form.default_rate || ''}
                onChange={e => setForm(f => ({ ...f, default_rate: parseFloat(e.target.value) || 0 }))}
              />
            </div>
          </div>
          <div>
            <label className="label">Currency</label>
            <select className="input" value={form.currency} onChange={e => setForm(f => ({ ...f, currency: e.target.value }))}>
              <option value="CAD">CAD</option>
              <option value="USD">USD</option>
              <option value="EUR">EUR</option>
              <option value="GBP">GBP</option>
              <option value="AUD">AUD</option>
              <option value="JPY">JPY</option>
            </select>
          </div>
        </div>
        {rateChanged && (
          <p className="text-xs text-fg-2 bg-accent/[0.07] rounded-md px-3 py-2">
            Projects still at {formatMoney(client!.default_rate)}/hr move to the new rate, so their unbilled time is priced at it. Invoiced time keeps its original rate.
          </p>
        )}
      </div>
    </Modal>
  )
}
