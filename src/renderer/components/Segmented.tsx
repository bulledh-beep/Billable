import { useId, type ReactNode } from 'react'
import { motion } from 'framer-motion'

export interface SegmentOption<T extends string> {
  value: T
  label: ReactNode
  count?: number
}

/** Pill-shaped switch for filters and view modes, with a sliding highlight. */
export default function Segmented<T extends string>({ value, options, onChange, className = '' }: {
  value: T
  options: SegmentOption<T>[]
  onChange: (v: T) => void
  className?: string
}) {
  const id = useId()
  return (
    <div className={`seg ${className}`} role="tablist">
      {options.map(o => {
        const active = value === o.value
        return (
          <button
            key={o.value}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(o.value)}
            className={`seg-item ${active ? 'active' : ''}`}
          >
            {active && (
              <motion.span
                layoutId={`seg-${id}`}
                className="seg-thumb"
                transition={{ type: 'spring', stiffness: 520, damping: 38 }}
              />
            )}
            <span className="relative z-[1] flex items-center gap-1.5">
              {o.label}
              {o.count !== undefined && (
                <span className={`num text-2xs ${active ? 'text-fg-3' : 'text-fg-4'}`}>{o.count}</span>
              )}
            </span>
          </button>
        )
      })}
    </div>
  )
}
