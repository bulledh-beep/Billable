import { useEffect, useState } from 'react'
import { ArrowRight } from 'lucide-react'
import Modal from './Modal'
import { notifyBillingChanged } from '../utils/events'
import type { Client } from '@shared/types'
import toast from 'react-hot-toast'

/**
 * Fold one client into another. Projects and invoices move over and the
 * duplicate record is removed.
 */
export default function MergeClientsModal({ open, clients, sourceId, targetId, onClose, onMerged }: {
  open: boolean
  clients: Client[]
  sourceId: number | null
  targetId?: number | null
  onClose: () => void
  onMerged: (targetId: number) => void
}) {
  const [from, setFrom] = useState<number>(0)
  const [into, setInto] = useState<number>(0)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!open) return
    setFrom(sourceId || 0)
    setInto(targetId || clients.find(c => c.id !== sourceId)?.id || 0)
  }, [open, sourceId, targetId, clients])

  const source = clients.find(c => c.id === from)
  const target = clients.find(c => c.id === into)
  const valid = !!source && !!target && from !== into

  const merge = async () => {
    if (!valid) return
    setSaving(true)
    try {
      const r = await window.api.clients.merge(from, into)
      toast.success(`Merged into ${target!.name}: ${r.moved_projects} project${r.moved_projects === 1 ? '' : 's'}, ${r.moved_invoices} invoice${r.moved_invoices === 1 ? '' : 's'} moved`)
      notifyBillingChanged()
      onMerged(into)
    } catch (err: any) {
      toast.error(err.message || String(err))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      isOpen={open}
      onClose={onClose}
      title="Merge clients"
      description="Use this when the same client was added twice."
      footer={
        <>
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button onClick={merge} disabled={!valid || saving} className="btn-primary">{saving ? 'Merging…' : 'Merge clients'}</button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-[1fr_auto_1fr] items-end gap-3">
          <div>
            <label className="label">Merge this client</label>
            <select className="input" value={from} onChange={e => setFrom(parseInt(e.target.value))}>
              <option value={0}>Choose…</option>
              {clients.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </div>
          <ArrowRight className="w-4 h-4 text-fg-3 mb-2" />
          <div>
            <label className="label">Into this one</label>
            <select className="input" value={into} onChange={e => setInto(parseInt(e.target.value))}>
              <option value={0}>Choose…</option>
              {clients.filter(c => c.id !== from).map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </div>
        </div>
        {valid && (
          <div className="rounded-lg border border-line bg-panel-2/60 p-3 text-sm text-fg-2 space-y-1">
            <p>
              <b className="text-fg">{source!.project_count ?? 0}</b> project{source!.project_count === 1 ? '' : 's'} and{' '}
              <b className="text-fg">{source!.invoice_count ?? 0}</b> invoice{source!.invoice_count === 1 ? '' : 's'} move to <b className="text-fg">{target!.name}</b>.
            </p>
            <p>{source!.name} is then removed. {target!.name}'s contact details and rate stay as they are.</p>
          </div>
        )}
      </div>
    </Modal>
  )
}
