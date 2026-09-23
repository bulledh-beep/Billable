import { FileText, Ban, Clock } from 'lucide-react'
import Modal from './Modal'
import { formatMoney, formatDay, formatHoursShort } from '../utils/format'
import type { Project } from '@shared/types'

export type CloseOutChoice = 'invoice' | 'dont-bill' | 'keep'

/**
 * Shown when a project with unbilled time is marked complete or archived,
 * so leftover hours don't get forgotten.
 */
export default function CloseOutModal({ project, nextStatus, onChoose, onClose }: {
  project: Project | null
  nextStatus: Project['status']
  onChoose: (choice: CloseOutChoice) => void
  onClose: () => void
}) {
  if (!project) return null
  const options: Array<{ key: CloseOutChoice; icon: typeof FileText; title: string; body: string; primary?: boolean }> = [
    {
      key: 'invoice',
      icon: FileText,
      title: 'Create an invoice for it',
      body: `Marks the project ${nextStatus} and opens a new invoice with this time.`,
      primary: true,
    },
    {
      key: 'keep',
      icon: Clock,
      title: 'Keep it unbilled for now',
      body: 'It stays on your Billing page until you invoice it.',
    },
    {
      key: 'dont-bill',
      icon: Ban,
      title: "Don't bill it",
      body: 'Marks the time non-billable so it stops showing as money owed.',
    },
  ]

  return (
    <Modal
      isOpen={!!project}
      onClose={onClose}
      title={`${project.name} has unbilled time`}
      description={
        <>
          <span className="num font-medium text-fg">{formatMoney(project.unbilled_amount || 0)}</span>
          {' '}across {formatHoursShort(project.unbilled_hours || 0)}
          {project.oldest_unbilled ? `, going back to ${formatDay(project.oldest_unbilled)}` : ''}.
        </>
      }
      size="sm"
    >
      <div className="space-y-2">
        {options.map(o => (
          <button
            key={o.key}
            onClick={() => onChoose(o.key)}
            className={`w-full flex items-start gap-3 p-3 rounded-lg border text-left transition-colors ${
              o.primary ? 'border-accent/50 bg-accent/[0.06] hover:bg-accent/10' : 'border-line hover:bg-fg/[0.03] hover:border-line-strong'
            }`}
          >
            <o.icon className={`w-4 h-4 mt-0.5 shrink-0 ${o.primary ? 'text-accent-text' : 'text-fg-3'}`} />
            <div>
              <div className="text-sm font-medium text-fg">{o.title}</div>
              <div className="text-xs text-fg-3 mt-0.5">{o.body}</div>
            </div>
          </button>
        ))}
      </div>
    </Modal>
  )
}
