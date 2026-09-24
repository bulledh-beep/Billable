import { motion } from 'framer-motion'

export type SuiteApp = 'billable' | 'content'

/**
 * The switch between the two apps in the suite. It sits in the same spot,
 * beside the window buttons, whichever app is showing.
 */
export default function SuiteSwitch({ value, onChange }: { value: SuiteApp; onChange: (app: SuiteApp) => void }) {
  const options: { value: SuiteApp; label: string }[] = [
    { value: 'billable', label: 'Billable' },
    { value: 'content', label: 'Content HQ' },
  ]
  return (
    <div className="no-drag relative inline-flex items-center p-[3px] rounded-full bg-fg/[0.07]" role="radiogroup" aria-label="Switch app">
      {options.map(o => {
        const active = o.value === value
        return (
          <button
            key={o.value}
            role="radio"
            aria-checked={active}
            onClick={() => !active && onChange(o.value)}
            className={`relative h-[24px] px-2.5 rounded-full text-[12px] font-semibold whitespace-nowrap transition-colors ${active ? 'text-accent-fg' : 'text-fg-3 hover:text-fg'}`}
          >
            {active && (
              <motion.span
                layoutId="suite-switch-thumb"
                className="absolute inset-0 rounded-full bg-accent"
                style={{ boxShadow: '0 2px 8px -2px rgb(255 140 10 / 0.6)' }}
                transition={{ type: 'spring', stiffness: 520, damping: 36 }}
              />
            )}
            <span className="relative">{o.label}</span>
          </button>
        )
      })}
    </div>
  )
}
