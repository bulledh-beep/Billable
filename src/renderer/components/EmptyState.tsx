import type { ElementType } from 'react'
import { Mascot, type Mood } from './Illustrations'

interface EmptyStateProps {
  /** Older call sites pass an icon; the mascot stands in for all of them now. */
  icon?: ElementType
  mood?: Mood
  title: string
  description: string
  action?: {
    label: string
    onClick: () => void
  }
  compact?: boolean
}

export default function EmptyState({ mood = 'idle', title, description, action, compact }: EmptyStateProps) {
  return (
    <div className={`flex flex-col items-center justify-center text-center px-6 ${compact ? 'py-6' : 'py-10'}`}>
      <Mascot size={compact ? 64 : 88} mood={mood} motion="bob" interactive className="mb-3" />
      <h3 className="text-[17px] font-bold text-fg">{title}</h3>
      <p className="text-[13.5px] font-semibold text-fg-3 max-w-[340px] mt-1 leading-5">{description}</p>
      {action && (
        <button onClick={action.onClick} className="btn-primary mt-4">
          {action.label}
        </button>
      )}
    </div>
  )
}
