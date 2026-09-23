import { createContext, useContext, Children, type ReactNode } from 'react'

export type Tone = 'accent' | 'green' | 'blue' | 'red' | 'amber' | 'violet' | 'gray'

export const TONE_DOT: Record<Tone, string> = {
  accent: 'bg-accent',
  green: 'bg-green',
  blue: 'bg-blue',
  red: 'bg-red',
  amber: 'bg-accent',
  violet: 'bg-violet',
  gray: 'bg-gray',
}

const InStrip = createContext(false)

interface MetricProps {
  label: string
  value: ReactNode
  sub?: ReactNode
  onClick?: () => void
  /** Marks the metric that is filtering the list below. */
  active?: boolean
  className?: string
}

/** A labelled figure. Inside a MetricStrip it's a cell; on its own it's a box. */
export default function Metric({ label, value, sub, onClick, active, className = '' }: MetricProps) {
  const inStrip = useContext(InStrip)
  const Tag = onClick ? 'button' : 'div'
  return (
    <Tag
      onClick={onClick}
      aria-pressed={onClick && active !== undefined ? active : undefined}
      className={`relative text-left min-w-0 px-4 pt-3 pb-3.5 ${inStrip ? '' : 'card'} ${
        onClick ? 'transition-colors hover:bg-fg/[0.025] focus-visible:outline-none focus-visible:bg-fg/[0.04]' : ''
      } ${active ? 'bg-fg/[0.035]' : ''} ${className}`}
    >
      <div className={`text-xs truncate ${active ? 'text-fg font-medium' : 'text-fg-3'}`}>{label}</div>
      <div className="mt-1.5 font-figures text-[27px] leading-[30px] text-fg truncate">{value}</div>
      {sub && <div className="text-xs text-fg-3 truncate">{sub}</div>}
      {active && <span className="absolute left-4 right-4 bottom-0 h-[2px] rounded-full bg-accent" />}
    </Tag>
  )
}

/** Figures side by side in one box, split by hairlines. */
export function MetricStrip({ children, className = '' }: { children: ReactNode; className?: string }) {
  const count = Children.toArray(children).filter(Boolean).length || 1
  return (
    <InStrip.Provider value={true}>
      <div
        className={`card grid divide-x divide-line overflow-hidden ${className}`}
        style={{ gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))` }}
      >
        {children}
      </div>
    </InStrip.Provider>
  )
}
