/**
 * A money figure with the cents set back, so the dollars read first:
 * $1,153.75 shows as $1,153 with a quieter .75.
 */
export default function Money({ amount, currency = 'USD', className = '' }: {
  amount: number
  currency?: string
  className?: string
}) {
  const locale = currency === 'CAD' ? 'en-CA' : 'en-US'
  const parts = new Intl.NumberFormat(locale, {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).formatToParts(Number(amount) || 0)
  let whole = ''
  let cents = ''
  let seenDecimal = false
  for (const p of parts) {
    if (p.type === 'decimal') seenDecimal = true
    if (seenDecimal && (p.type === 'decimal' || p.type === 'fraction')) cents += p.value
    else whole += p.value
  }
  return (
    <span className={`num whitespace-nowrap ${className}`}>
      {whole}
      <span className="opacity-45">{cents}</span>
    </span>
  )
}
