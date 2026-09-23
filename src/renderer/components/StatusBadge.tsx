import type { ReactNode } from 'react'
import { useNavigate } from 'react-router-dom'
import type { Tone } from './Metric'
import type { BillingState } from '@shared/types'

const TONE_CLASSES: Record<Tone, { bg: string; text: string; dot: string }> = {
  accent: { bg: 'bg-accent/[0.14]', text: 'text-accent-text', dot: 'bg-accent' },
  green: { bg: 'bg-green/[0.13]', text: 'text-green', dot: 'bg-green' },
  blue: { bg: 'bg-blue/[0.13]', text: 'text-blue', dot: 'bg-blue' },
  red: { bg: 'bg-red/[0.13]', text: 'text-red', dot: 'bg-red' },
  amber: { bg: 'bg-amber/[0.14]', text: 'text-amber', dot: 'bg-amber' },
  violet: { bg: 'bg-violet/[0.13]', text: 'text-violet', dot: 'bg-violet' },
  gray: { bg: 'bg-fg/[0.07]', text: 'text-fg-2', dot: 'bg-fg-3' },
}

export function Badge({ tone = 'gray', dot = false, children, className = '' }: {
  tone?: Tone
  dot?: boolean
  children: ReactNode
  className?: string
}) {
  const t = TONE_CLASSES[tone]
  return (
    <span className={`badge ${t.text} ${className}`}>
      {dot && <span className={`w-1.5 h-1.5 rounded-full ${t.dot}`} />}
      {children}
    </span>
  )
}

const STATUS: Record<string, { tone: Tone; label: string }> = {
  // Invoices
  draft: { tone: 'gray', label: 'Draft' },
  sent: { tone: 'blue', label: 'Sent' },
  overdue: { tone: 'red', label: 'Overdue' },
  paid: { tone: 'green', label: 'Paid' },
  // Projects
  active: { tone: 'green', label: 'Active' },
  paused: { tone: 'amber', label: 'Paused' },
  complete: { tone: 'blue', label: 'Complete' },
  archived: { tone: 'gray', label: 'Archived' },
}

export default function StatusBadge({ status }: { status: string }) {
  const s = STATUS[status] || { tone: 'gray' as Tone, label: status }
  return <Badge tone={s.tone}>{s.label}</Badge>
}

const BILLING: Record<BillingState, { tone: Tone; label: string }> = {
  unbilled: { tone: 'amber', label: 'Unbilled' },
  nonbillable: { tone: 'gray', label: 'Non-billable' },
  draft: { tone: 'gray', label: 'Draft' },
  sent: { tone: 'blue', label: 'Sent' },
  overdue: { tone: 'red', label: 'Overdue' },
  paid: { tone: 'green', label: 'Paid' },
  invoiced: { tone: 'blue', label: 'Invoiced' },
}

const TONE_TEXT: Record<Tone, string> = {
  accent: 'text-accent-text',
  green: 'text-green',
  blue: 'text-blue',
  red: 'text-red',
  amber: 'text-amber',
  violet: 'text-violet',
  gray: 'text-fg-3',
}

/**
 * Where a time entry's money is, as quiet text. Unbilled is the normal state,
 * so it shows nothing; invoiced time names its invoice and links to it.
 */
export function BillingChip({ state, invoiceId, invoiceNumber }: {
  state?: BillingState
  invoiceId?: number | null
  invoiceNumber?: string | null
}) {
  const navigate = useNavigate()
  if (!state || state === 'unbilled') return null
  if (state === 'nonbillable') return <span className="text-xs text-fg-4 whitespace-nowrap">Not billable</span>
  const b = BILLING[state]
  const status = <span className={TONE_TEXT[b.tone]}>{b.label}</span>
  if (invoiceId) {
    return (
      <button
        onClick={e => { e.stopPropagation(); navigate(`/invoices/${invoiceId}`) }}
        className="text-xs whitespace-nowrap hover:underline underline-offset-2"
        title={`Open ${invoiceNumber}`}
      >
        {status}{invoiceNumber && <span className="text-fg-3"> · {invoiceNumber}</span>}
      </button>
    )
  }
  return <span className="text-xs whitespace-nowrap">{status}</span>
}
