import type { ElementType } from 'react'

interface EmptyStateProps {
  icon: ElementType
  title: string
  description: string
  action?: {
    label: string
    onClick: () => void
  }
  compact?: boolean
}

export default function EmptyState({ icon: Icon, title, description, action, compact }: EmptyStateProps) {
  return (
    <div className={`flex flex-col items-center justify-center text-center px-6 ${compact ? 'py-7' : 'py-12'}`}>
      <Icon className={`${compact ? 'w-6 h-6' : 'w-7 h-7'} text-fg-4 mb-2.5`} strokeWidth={1.5} />
      <h3 className="text-[13px] font-semibold text-fg">{title}</h3>
      <p className="text-xs text-fg-3 max-w-[320px] mt-1 leading-[17px]">{description}</p>
      {action && (
        <button onClick={action.onClick} className="btn-secondary mt-3.5">
          {action.label}
        </button>
      )}
    </div>
  )
}
