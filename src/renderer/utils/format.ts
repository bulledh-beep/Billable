export function formatMoney(amount: number, currency: string = 'USD'): string {
  // en-CA renders CAD as "$" (en-US would show "CA$")
  const locale = currency === 'CAD' ? 'en-CA' : 'en-US'
  return new Intl.NumberFormat(locale, {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(Number(amount) || 0)
}

/** "$1,154" / "$12.4k" for tight spots like badges and chart axes. */
export function formatMoneyCompact(amount: number): string {
  const n = Number(amount) || 0
  if (Math.abs(n) >= 10_000) return `$${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
  return `$${Math.round(n).toLocaleString('en-US')}`
}

export function formatHours(hours: number): string {
  return (Number(hours) || 0).toFixed(1)
}

/** Decimal hours shown the same way as durations: "14h 45m", "45m", "3h", "0h". Invoice lines keep decimals. */
export function formatHoursShort(hours: number): string {
  const minutes = Math.round((Number(hours) || 0) * 60)
  return minutes === 0 ? '0h' : formatDurationShort(minutes)
}

export function formatDuration(minutes: number): string {
  const h = Math.floor(minutes / 60)
  const m = Math.floor(minutes % 60)
  const s = Math.floor((minutes * 60) % 60)
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

export function formatElapsed(startTime: string): string {
  const start = new Date(startTime).getTime()
  const now = Date.now()
  const diff = Math.max(0, now - start)
  const totalSeconds = Math.floor(diff / 1000)
  const h = Math.floor(totalSeconds / 3600)
  const m = Math.floor((totalSeconds % 3600) / 60)
  const s = totalSeconds % 60
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

/** Parse "YYYY-MM-DD" as a local day, SQLite "YYYY-MM-DD HH:MM:SS" as UTC, anything else natively. */
export function toDate(dateStr: string): Date {
  const s = String(dateStr || '')
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) return parseLocalDate(s)
  if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}(:\d{2})?$/.test(s)) return new Date(s.replace(' ', 'T') + 'Z')
  return new Date(s)
}

export function formatDate(dateStr: string): string {
  if (!dateStr) return ''
  return toDate(dateStr).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  })
}

/** "Jun 12", or "Jun 12, 2025" outside the current year. */
export function formatDay(dateStr: string): string {
  if (!dateStr) return ''
  const d = toDate(dateStr)
  const sameYear = d.getFullYear() === new Date().getFullYear()
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', ...(sameYear ? {} : { year: 'numeric' }) })
}

/** Whole days from dateStr to today (positive = in the past). */
export function daysSince(dateStr: string): number {
  const d = toDate(dateStr)
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const now = new Date()
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  return Math.round((today - day) / 86_400_000)
}

/** "today", "yesterday", "12 days ago", "in 3 days". */
export function relativeDays(dateStr: string): string {
  const n = daysSince(dateStr)
  if (n === 0) return 'today'
  if (n === 1) return 'yesterday'
  if (n === -1) return 'tomorrow'
  if (n > 1) return `${n} days ago`
  return `in ${-n} days`
}

export function formatTime(dateStr: string): string {
  return toDate(dateStr).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })
}

export function formatRelative(dateStr: string): string {
  const date = toDate(dateStr)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMins = Math.floor(diffMs / 60000)
  const diffHours = Math.floor(diffMs / 3600000)

  if (diffMins < 1) return 'Just now'
  if (diffMins < 60) return `${diffMins}m ago`
  if (diffHours < 24 && daysSince(dateStr) === 0) return `${diffHours}h ago`
  const days = daysSince(dateStr)
  if (days === 1) return 'Yesterday'
  if (days < 7) return `${days}d ago`
  return formatDay(dateStr)
}

export function getInitials(name: string): string {
  return name
    .split(' ')
    .map(n => n[0])
    .join('')
    .toUpperCase()
    .slice(0, 2)
}

export function getAvatarColor(name: string): string {
  const colors = [
    '#F5A623', '#E74C3C', '#3498DB', '#2ECC71', '#9B59B6',
    '#1ABC9C', '#E67E22', '#EC407A', '#26A69A', '#7E57C2',
  ]
  let hash = 0
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash)
  }
  return colors[Math.abs(hash) % colors.length]
}

/**
 * Local-timezone YYYY-MM-DD. Never use toISOString() for "today" — that's UTC,
 * so after ~5pm Pacific it silently rolls over to tomorrow's date.
 */
export function toLocalISODate(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

/** Parse a YYYY-MM-DD string as a LOCAL date (not UTC midnight). */
export function parseLocalDate(dateStr: string): Date {
  const [y, m, d] = dateStr.split('-').map(Number)
  return new Date(y, m - 1, d)
}

export function addDays(dateStr: string, days: number): string {
  const date = parseLocalDate(dateStr)
  date.setDate(date.getDate() + days)
  return toLocalISODate(date)
}

export function todayISO(): string {
  return toLocalISODate(new Date())
}

/** "2h 15m" / "45m" / "3h" — for logged entries where seconds are noise. */
export function formatDurationShort(minutes: number): string {
  const total = Math.round(minutes)
  const h = Math.floor(total / 60)
  const m = total % 60
  if (h && m) return `${h}h ${m}m`
  if (h) return `${h}h`
  return `${m}m`
}

/**
 * Human duration → minutes. Accepts "2h 30m", "2h", "30m", "1.5h", "2:30", "150".
 * Returns null if it can't be parsed.
 */
export function parseDurationInput(input: string): number | null {
  const s = input.trim().toLowerCase()
  if (!s) return null
  const colon = s.match(/^(\d+):(\d{1,2})$/)
  if (colon) return parseInt(colon[1]) * 60 + parseInt(colon[2])
  if (/^\d+(\.\d+)?$/.test(s)) return Math.round(parseFloat(s))
  let total = 0
  let matched = false
  const h = s.match(/(\d+(?:\.\d+)?)\s*h/)
  if (h) { total += parseFloat(h[1]) * 60; matched = true }
  const m = s.match(/(\d+(?:\.\d+)?)\s*m/)
  if (m) { total += parseFloat(m[1]); matched = true }
  return matched ? Math.round(total) : null
}
