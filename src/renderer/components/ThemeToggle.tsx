import { Moon, Sun, Monitor } from 'lucide-react'
import { useTheme, type ThemePreference } from '../hooks/useTheme'

const OPTIONS: Array<{ value: ThemePreference; label: string; icon: typeof Moon }> = [
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
  { value: 'auto', label: 'Auto', icon: Monitor },
]

interface ThemeToggleProps {
  /** Compact icon-only mode (for tight spots like the sidebar footer). */
  compact?: boolean
}

export default function ThemeToggle({ compact = false }: ThemeToggleProps) {
  const { preference, setPreference } = useTheme()

  return (
    <div
      className={`inline-flex gap-0.5 bg-surface-100 rounded-lg p-0.5 ${compact ? '' : 'border border-text-tertiary/10'}`}
    >
      {OPTIONS.map(({ value, label, icon: Icon }) => {
        const active = preference === value
        return (
          <button
            key={value}
            onClick={() => setPreference(value)}
            title={`${label} mode`}
            className={`flex items-center gap-1.5 rounded-md transition-colors ${
              compact ? 'p-1.5' : 'px-2.5 py-1.5 text-xs font-medium'
            } ${
              active
                ? 'bg-surface-300 text-text-primary'
                : 'text-text-tertiary hover:text-text-secondary'
            }`}
          >
            <Icon className={compact ? 'w-3.5 h-3.5' : 'w-3.5 h-3.5'} />
            {!compact && <span>{label}</span>}
          </button>
        )
      })}
    </div>
  )
}
