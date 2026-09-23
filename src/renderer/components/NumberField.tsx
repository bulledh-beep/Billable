import { useEffect, useState } from 'react'

/**
 * Number input that shows a tidy fixed-decimal value ("65.00") until you
 * focus it, then lets you type freely.
 */
export default function NumberField({ value, onChange, decimals = 2, className = '', step, min, placeholder }: {
  value: number
  onChange: (n: number) => void
  decimals?: number
  className?: string
  step?: string
  min?: number
  placeholder?: string
}) {
  const [focused, setFocused] = useState(false)
  const [draft, setDraft] = useState(String(value ?? ''))

  useEffect(() => {
    if (!focused) setDraft(String(value ?? ''))
  }, [value, focused])

  const shown = focused ? draft : (Number.isFinite(value) ? Number(value).toFixed(decimals) : '')

  return (
    <input
      type="text"
      inputMode="decimal"
      className={`input num ${className}`}
      value={shown}
      step={step}
      min={min}
      placeholder={placeholder}
      onFocus={e => { setFocused(true); setDraft(String(value ?? '')); requestAnimationFrame(() => e.target.select()) }}
      onBlur={() => setFocused(false)}
      onChange={e => {
        const raw = e.target.value.replace(/[^0-9.\-]/g, '')
        setDraft(raw)
        const n = parseFloat(raw)
        onChange(Number.isFinite(n) ? n : 0)
      }}
      onKeyDown={e => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur() }}
    />
  )
}
