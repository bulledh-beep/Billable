import { Moon, Sun, Monitor } from 'lucide-react'
import Segmented from './Segmented'
import { useTheme, type ThemePreference } from '../hooks/useTheme'

export default function ThemeToggle() {
  const { preference, setPreference } = useTheme()
  return (
    <Segmented<ThemePreference>
      value={preference}
      onChange={setPreference}
      options={[
        { value: 'light', label: <><Sun className="w-3.5 h-3.5" /> Light</> },
        { value: 'dark', label: <><Moon className="w-3.5 h-3.5" /> Dark</> },
        { value: 'auto', label: <><Monitor className="w-3.5 h-3.5" /> Auto</> },
      ]}
    />
  )
}
