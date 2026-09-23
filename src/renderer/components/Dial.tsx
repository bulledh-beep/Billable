import { useEffect, useState } from 'react'

// Dials drawn from the app icon: a graphite face, a light hour hand and the
// amber minute hand that stands for time being billed.

const AMBER = '#F5A623'

const reducedMotion = () =>
  typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

/** The app icon in miniature, with its hands set to a given time. */
export function ClockMark({ size = 18, hour = 10, minute = 2 }: { size?: number; hour?: number; minute?: number }) {
  const hourAngle = ((hour % 12) + minute / 60) * 30
  const minuteAngle = minute * 6
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" className="shrink-0">
      <defs>
        <linearGradient id="clock-mark-face" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#2E2E33" />
          <stop offset="1" stopColor="#141417" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="7.5" fill="url(#clock-mark-face)" />
      <rect x="0.25" y="0.25" width="31.5" height="31.5" rx="7.25" fill="none" stroke="rgb(255 255 255 / 0.12)" strokeWidth="0.5" />
      <circle cx="16" cy="16" r="10.5" fill="none" stroke="rgb(255 255 255 / 0.2)" strokeWidth="1" />
      {[0, 90, 180, 270].map(a => (
        <line key={a} x1="16" y1="6.6" x2="16" y2="8.2" stroke="rgb(255 255 255 / 0.45)" strokeWidth="1.1" strokeLinecap="round" transform={`rotate(${a} 16 16)`} />
      ))}
      <line x1="16" y1="16" x2="16" y2="10.2" stroke="#E6E6E9" strokeWidth="2" strokeLinecap="round" transform={`rotate(${hourAngle} 16 16)`} />
      <line x1="16" y1="16" x2="16" y2="7.4" stroke={AMBER} strokeWidth="1.5" strokeLinecap="round" transform={`rotate(${minuteAngle} 16 16)`} />
      <circle cx="16" cy="16" r="1.8" fill={AMBER} />
    </svg>
  )
}

/** The app mark showing the actual time. */
export function LiveClockMark({ size = 18 }: { size?: number }) {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 15_000)
    return () => clearInterval(id)
  }, [])
  return <ClockMark size={size} hour={now.getHours()} minute={now.getMinutes()} />
}

/**
 * A stopwatch face for a running timer. The amber hand steps once a second
 * with a small quartz-style settle; the short hand counts minutes.
 */
export function StopwatchDial({ seconds, size = 16, paused = false, className = '' }: {
  seconds: number
  size?: number
  paused?: boolean
  className?: string
}) {
  const hand = paused ? 'rgb(var(--fg-3))' : AMBER
  const secondAngle = seconds * 6
  const minuteAngle = (seconds / 60) * 6
  const tick = reducedMotion() || paused ? undefined : 'transform 180ms cubic-bezier(0.3, 1.8, 0.6, 1)'
  const big = size >= 32
  return (
    <svg width={size} height={size} viewBox="0 0 32 32" aria-hidden="true" className={`shrink-0 ${className}`}>
      <circle cx="16" cy="16" r="14.6" fill="none" stroke="rgb(var(--fg) / 0.28)" strokeWidth={big ? 0.8 : 1.4} />
      {Array.from({ length: big ? 60 : 4 }, (_, i) => {
        const major = big ? i % 5 === 0 : true
        return (
          <line
            key={i}
            x1="16"
            y1={big ? 2.6 : 3.4}
            x2="16"
            y2={big ? (major ? 5.4 : 3.8) : 6.2}
            stroke={`rgb(var(--fg) / ${major ? 0.55 : 0.25})`}
            strokeWidth={big ? (major ? 0.9 : 0.5) : 1.5}
            strokeLinecap="round"
            transform={`rotate(${i * (big ? 6 : 90)} 16 16)`}
          />
        )
      })}
      <line
        x1="16" y1="16" x2="16" y2="9"
        stroke="rgb(var(--fg) / 0.75)" strokeWidth={big ? 1.6 : 2.2} strokeLinecap="round"
        style={{ transform: `rotate(${minuteAngle}deg)`, transformOrigin: '16px 16px' }}
      />
      <line
        x1="16" y1={big ? 19.5 : 18} x2="16" y2={big ? 4.2 : 5}
        stroke={hand} strokeWidth={big ? 1 : 1.6} strokeLinecap="round"
        style={{ transform: `rotate(${secondAngle}deg)`, transformOrigin: '16px 16px', transition: tick }}
      />
      <circle cx="16" cy="16" r={big ? 1.6 : 2.1} fill={hand} />
    </svg>
  )
}

/** A dial at rest, for empty states about time. Sized by className like an icon. */
export function DialGlyph({ className = '' }: { className?: string; strokeWidth?: number }) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <circle cx="16" cy="16" r="14" fill="none" stroke="currentColor" strokeWidth="1.2" />
      {Array.from({ length: 12 }, (_, i) => (
        <line key={i} x1="16" y1="3.6" x2="16" y2={i % 3 === 0 ? 6.4 : 5.2} stroke="currentColor" strokeWidth={i % 3 === 0 ? 1.3 : 0.9} strokeLinecap="round" transform={`rotate(${i * 30} 16 16)`} />
      ))}
      <line x1="16" y1="16" x2="16" y2="10" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" transform="rotate(-60 16 16)" />
      <line x1="16" y1="16" x2="16" y2="6.8" stroke={AMBER} strokeWidth="1.4" strokeLinecap="round" transform="rotate(60 16 16)" />
      <circle cx="16" cy="16" r="1.7" fill={AMBER} />
    </svg>
  )
}
