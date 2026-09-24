import { useEffect, useRef, useState } from 'react'

function parts(amount: number, currency: string) {
  const locale = currency === 'CAD' ? 'en-CA' : 'en-US'
  const list = new Intl.NumberFormat(locale, {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).formatToParts(amount)
  let whole = ''
  let cents = ''
  let seenDecimal = false
  for (const p of list) {
    if (p.type === 'decimal') seenDecimal = true
    if (seenDecimal && (p.type === 'decimal' || p.type === 'fraction')) cents += p.value
    else whole += p.value
  }
  return { whole, cents }
}

const reducedMotion = () => !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

/**
 * A money figure with the cents set back, so the dollars read first:
 * $1,153.75 shows as $1,153 with a quieter .75. With `animate`, it counts
 * up to its value, and glides when the value changes.
 */
export default function Money({ amount, currency = 'USD', animate = false, className = '' }: {
  amount: number
  currency?: string
  animate?: boolean
  className?: string
}) {
  const target = Number(amount) || 0
  const [shown, setShown] = useState(animate && !reducedMotion() ? 0 : target)
  const from = useRef(shown)

  useEffect(() => {
    if (!animate || reducedMotion()) { setShown(target); from.current = target; return }
    const start = performance.now()
    const a = from.current
    const duration = 700
    let frame = 0
    const tick = (t: number) => {
      const p = Math.min(1, (t - start) / duration)
      const eased = 1 - Math.pow(1 - p, 3)
      setShown(a + (target - a) * eased)
      if (p < 1) frame = requestAnimationFrame(tick)
      else from.current = target
    }
    frame = requestAnimationFrame(tick)
    return () => { cancelAnimationFrame(frame); from.current = target }
  }, [target, animate])

  const { whole, cents } = parts(shown, currency)
  return (
    <span className={`num whitespace-nowrap ${className}`}>
      {whole}
      <span className="opacity-45">{cents}</span>
    </span>
  )
}
