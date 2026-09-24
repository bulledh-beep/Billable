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

const TINT = {
  orange: '#FF9600',
  blue: '#1CB0F6',
  red: '#FF4B4B',
  green: '#58CC02',
  purple: '#CE82FF',
  yellow: '#FFC800',
} as const
export type MetricTint = keyof typeof TINT

interface MetricProps {
  label: string
  value: ReactNode
  sub?: ReactNode
  /** A small illustration beside the figure. */
  icon?: ReactNode
  /** Washes the card in a soft tint of this color. */
  tint?: MetricTint
  onClick?: () => void
  /** Marks the metric that is filtering the list below. */
  active?: boolean
  className?: string
}

/** A friendly stat card: illustration, big number, label. */
export default function Metric({ label, value, sub, icon, tint, onClick, active, className = '' }: MetricProps) {
  useContext(InStrip)
  const Tag = onClick ? 'button' : 'div'
  const color = tint ? TINT[tint] : null
  return (
    <Tag
      onClick={onClick}
      aria-pressed={onClick && active !== undefined ? active : undefined}
      className={`group wiggle-on-hover relative text-left min-w-0 flex items-center gap-3 px-4 py-3.5 rounded-card border shadow-card transition-[transform,box-shadow,border-color] duration-150 ${
        color ? '' : 'bg-panel border-line'
      } ${onClick ? 'hover:-translate-y-0.5 active:translate-y-0 focus-visible:outline-none focus-visible:ring-4 focus-visible:ring-accent/25' : ''} ${
        active ? 'ring-2 ring-accent' : ''
      } ${className}`}
      style={color ? {
        background: `color-mix(in srgb, ${color} 9%, rgb(var(--panel)))`,
        borderColor: `color-mix(in srgb, ${color} 22%, rgb(var(--line)))`,
      } : undefined}
    >
      {icon && <div className="wiggle-target w-9 h-9 shrink-0 [&>svg]:w-full [&>svg]:h-full">{icon}</div>}
      <div className="min-w-0">
        <div className="font-figures text-[22px] leading-[27px] text-fg truncate">{value}</div>
        <div className={`text-[13px] font-semibold truncate ${active ? 'text-accent-text' : 'text-fg-2'}`}>{label}</div>
        {sub && <div className="text-[12px] text-fg-3 truncate">{sub}</div>}
      </div>
    </Tag>
  )
}

/** Stat cards side by side. */
export function MetricStrip({ children, className = '' }: { children: ReactNode; className?: string }) {
  const count = Children.toArray(children).filter(Boolean).length || 1
  return (
    <InStrip.Provider value={true}>
      <div className={`grid gap-3 ${className}`} style={{ gridTemplateColumns: `repeat(${count}, minmax(0, 1fr))` }}>
        {children}
      </div>
    </InStrip.Provider>
  )
}
