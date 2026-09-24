import { useEffect, useRef, useState } from 'react'

// Flat, bright illustrations: the mascot, the sidebar icons and a few
// badges. Each shape has a base color, a deeper shade for weight and a
// light highlight.

const C = {
  orange: '#FF9600', orangeDeep: '#E07B00', orangeLight: '#FFB84D',
  yellow: '#FFC800', yellowDeep: '#E5A800', yellowLight: '#FFE066',
  green: '#58CC02', greenDeep: '#46A302', greenLight: '#89E219',
  blue: '#1CB0F6', blueDeep: '#1899D6', blueLight: '#84D8FF',
  red: '#FF4B4B', redDeep: '#EA2B2B',
  purple: '#CE82FF', purpleDeep: '#A568CC',
  grey: '#AFAFAF', greyDeep: '#8F8F8F',
  ink: '#4B4B4B', white: '#FFFFFF', blush: '#FF9EB0',
}

type IconProps = { className?: string }

/** Fired when the mascot is clicked five times quickly. */
export const MASCOT_PARTY = 'billable:mascot-party'

// ---------------------------------------------------------------- Mascot

export type Mood = 'idle' | 'happy' | 'wave' | 'sleepy' | 'worried'

/**
 * Billable's mascot: a little alarm clock with bells for ears. Moods change
 * the eyes, mouth and arms. It blinks now and then. When `interactive`, its
 * eyes follow the pointer and clicking it makes it hop with joy.
 */
export function Mascot({ size = 96, mood = 'idle', motion = 'none', interactive = false, ringing = false, className = '' }: {
  size?: number
  mood?: Mood
  motion?: 'none' | 'bob' | 'hop'
  interactive?: boolean
  /** Shakes the bells like an alarm going off. */
  ringing?: boolean
  className?: string
}) {
  const ref = useRef<SVGSVGElement>(null)
  const [look, setLook] = useState({ x: 0, y: 0 })
  const [boops, setBoops] = useState(0)
  const [cheer, setCheer] = useState(false)
  const clicks = useRef<number[]>([])

  useEffect(() => {
    if (!interactive) return
    let frame = 0
    const onMove = (e: MouseEvent) => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        const box = ref.current?.getBoundingClientRect()
        if (!box) return
        const dx = e.clientX - (box.left + box.width / 2)
        const dy = e.clientY - (box.top + box.height * 0.5)
        const dist = Math.hypot(dx, dy) || 1
        const reach = Math.min(1, dist / 240)
        setLook({ x: (dx / dist) * 3.2 * reach, y: (dy / dist) * 2.6 * reach })
      })
    }
    window.addEventListener('mousemove', onMove)
    return () => { window.removeEventListener('mousemove', onMove); cancelAnimationFrame(frame) }
  }, [interactive])

  const boop = () => {
    if (!interactive) return
    setBoops(b => b + 1)
    setCheer(true)
    window.setTimeout(() => setCheer(false), 1300)
    // Five quick clicks set off the party
    const now = Date.now()
    clicks.current = [...clicks.current.filter(t => now - t < 1600), now]
    if (clicks.current.length >= 5) {
      clicks.current = []
      window.dispatchEvent(new Event(MASCOT_PARTY))
    }
  }

  const face: Mood = cheer ? 'happy' : mood
  const anim = boops > 0 && cheer
    ? 'mascot-hop 0.8s ease-in-out 1'
    : motion === 'bob' ? 'mascot-bob 3.2s ease-in-out infinite' : motion === 'hop' ? 'mascot-hop 0.9s ease-in-out 2' : undefined
  const armsUp = face === 'happy'
  const wave = face === 'wave'
  return (
    <svg
      key={boops}
      ref={ref}
      width={size}
      height={size}
      viewBox="0 0 120 120"
      aria-hidden="true"
      onClick={boop}
      className={`shrink-0 overflow-visible ${interactive ? 'cursor-pointer' : ''} ${className}`}
      style={{ animation: anim }}
    >
      <ellipse cx="60" cy="113" rx="28" ry="4.5" fill="#000" opacity="0.1" />
      {/* Feet */}
      <ellipse cx="45" cy="104" rx="11" ry="7" fill={C.orangeDeep} />
      <ellipse cx="75" cy="104" rx="11" ry="7" fill={C.orangeDeep} />
      {/* Bells */}
      <g style={ringing ? { transformOrigin: '60px 40px', animation: 'bell-ring 0.18s ease-in-out infinite' } : undefined}>
      <g transform="rotate(-24 30 26)">
        <path d="M17 32 a13 13 0 0 1 26 0 z" fill={C.yellow} />
        <path d="M17 32 h26 v2.5 a2 2 0 0 1 -2 2 h-22 a2 2 0 0 1 -2 -2 z" fill={C.yellowDeep} />
        <ellipse cx="24.5" cy="25" rx="3" ry="4.5" fill={C.yellowLight} transform="rotate(-20 24.5 25)" />
      </g>
      <g transform="rotate(24 90 26)">
        <path d="M77 32 a13 13 0 0 1 26 0 z" fill={C.yellow} />
        <path d="M77 32 h26 v2.5 a2 2 0 0 1 -2 2 h-22 a2 2 0 0 1 -2 -2 z" fill={C.yellowDeep} />
        <ellipse cx="84.5" cy="25" rx="3" ry="4.5" fill={C.yellowLight} transform="rotate(-20 84.5 25)" />
      </g>
      </g>
      <rect x="55" y="11" width="10" height="9" rx="3" fill={C.orangeDeep} />
      {/* Arms, behind the body */}
      {armsUp ? (
        <>
          <path d="M24 58 q-12 -8 -10 -24" stroke={C.orange} strokeWidth="9" strokeLinecap="round" fill="none" />
          <path d="M96 58 q12 -8 10 -24" stroke={C.orange} strokeWidth="9" strokeLinecap="round" fill="none" />
        </>
      ) : wave ? (
        <>
          <path d="M20 70 q-8 8 -5 18" stroke={C.orange} strokeWidth="9" strokeLinecap="round" fill="none" />
          <g style={{ transformOrigin: '97px 58px', animation: 'wiggle 1.4s ease-in-out infinite' }}>
            <path d="M97 58 q13 -6 13 -22" stroke={C.orange} strokeWidth="9" strokeLinecap="round" fill="none" />
          </g>
        </>
      ) : (
        <>
          <path d="M20 70 q-8 8 -5 18" stroke={C.orange} strokeWidth="9" strokeLinecap="round" fill="none" />
          <path d="M100 70 q8 8 5 18" stroke={C.orange} strokeWidth="9" strokeLinecap="round" fill="none" />
        </>
      )}
      {/* Body */}
      <circle cx="60" cy="62" r="44" fill={C.orange} />
      <path d="M104 62 a44 44 0 0 1 -80 25 a46 46 0 0 0 80 -25 z" fill={C.orangeDeep} opacity="0.45" />
      <ellipse cx="36" cy="36" rx="9" ry="5.5" fill={C.white} opacity="0.35" transform="rotate(-38 36 36)" />
      {/* Face */}
      <circle cx="60" cy="65" r="33" fill={C.white} />
      {[0, 90, 180, 270].map(a => (
        <rect key={a} x="58" y="35" width="4" height="7" rx="2" fill={C.orangeLight} transform={`rotate(${a} 60 65)`} />
      ))}
      {/* Eyes */}
      {face === 'happy' ? (
        <>
          <path d="M42 62 q6 -9 12 0" stroke={C.ink} strokeWidth="4.5" strokeLinecap="round" fill="none" />
          <path d="M66 62 q6 -9 12 0" stroke={C.ink} strokeWidth="4.5" strokeLinecap="round" fill="none" />
        </>
      ) : face === 'sleepy' ? (
        <>
          <path d="M42 60 q6 5 12 0" stroke={C.ink} strokeWidth="4" strokeLinecap="round" fill="none" />
          <path d="M66 60 q6 5 12 0" stroke={C.ink} strokeWidth="4" strokeLinecap="round" fill="none" />
        </>
      ) : (
        <>
          <g style={{ transform: `translate(${look.x}px, ${look.y}px)`, transition: 'transform 120ms ease-out' }}>
            <g style={{ transformOrigin: '60px 59px', animation: 'blink 4.6s ease-in-out infinite' }}>
              <ellipse cx="48" cy="59" rx="6.5" ry={face === 'worried' ? 7 : 8.5} fill={C.ink} />
              <ellipse cx="72" cy="59" rx="6.5" ry={face === 'worried' ? 7 : 8.5} fill={C.ink} />
              <circle cx="50.3" cy="55.5" r="2.4" fill={C.white} />
              <circle cx="74.3" cy="55.5" r="2.4" fill={C.white} />
            </g>
          </g>
          {face === 'worried' && (
            <>
              <path d="M40 47 l12 3" stroke={C.ink} strokeWidth="3" strokeLinecap="round" />
              <path d="M80 47 l-12 3" stroke={C.ink} strokeWidth="3" strokeLinecap="round" />
            </>
          )}
        </>
      )}
      {/* Cheeks */}
      <ellipse cx="39" cy="73" rx="5.5" ry="3.2" fill={C.blush} opacity="0.75" />
      <ellipse cx="81" cy="73" rx="5.5" ry="3.2" fill={C.blush} opacity="0.75" />
      {/* Mouth */}
      {face === 'happy' || face === 'wave' ? (
        <>
          <path d="M49 72 q11 16 22 0 z" fill={C.ink} />
          <path d="M54.5 79 q5.5 -4 11 0 q-5.5 5 -11 0 z" fill="#FF6B81" />
        </>
      ) : face === 'sleepy' ? (
        <ellipse cx="60" cy="77" rx="4" ry="3" fill={C.ink} />
      ) : face === 'worried' ? (
        <path d="M52 80 q8 -6 16 0" stroke={C.ink} strokeWidth="3.5" strokeLinecap="round" fill="none" />
      ) : (
        <path d="M52 75 q8 7 16 0" stroke={C.ink} strokeWidth="3.5" strokeLinecap="round" fill="none" />
      )}
      {face === 'sleepy' && (
        <g fill={C.blue} fontFamily="Billable Rounded, sans-serif" fontWeight="900">
          <text x="92" y="28" fontSize="14" style={{ animation: 'float-z 2.4s ease-in-out infinite' }}>z</text>
          <text x="102" y="16" fontSize="10" style={{ animation: 'float-z 2.4s ease-in-out 1.2s infinite' }}>z</text>
        </g>
      )}
    </svg>
  )
}

// ---------------------------------------------------------------- Sidebar icons

export function IconHome({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="7" y="13" width="18" height="15" rx="3.5" fill={C.yellow} />
      <path d="M7 24h18v.5a3.5 3.5 0 0 1-3.5 3.5h-11A3.5 3.5 0 0 1 7 24.5z" fill={C.yellowDeep} />
      <rect x="13" y="19" width="6" height="9" rx="2" fill={C.orangeDeep} />
      <path d="M3.5 15.5 16 5l12.5 10.5" stroke={C.red} strokeWidth="4.2" strokeLinecap="round" strokeLinejoin="round" fill="none" />
    </svg>
  )
}

export function IconClients({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <circle cx="21.5" cy="10.5" r="4.5" fill={C.purple} />
      <path d="M14 26v-2.5a7.5 7.5 0 0 1 15 0V26a1.5 1.5 0 0 1-1.5 1.5h-12A1.5 1.5 0 0 1 14 26z" fill={C.purpleDeep} />
      <circle cx="12" cy="12" r="5.5" fill={C.blue} />
      <path d="M3 27v-2.8a9 9 0 0 1 18 0V27a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 27z" fill={C.blueDeep} />
      <ellipse cx="10" cy="10" rx="1.6" ry="2.2" fill={C.white} opacity="0.4" transform="rotate(-30 10 10)" />
    </svg>
  )
}

export function IconProjects({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <path d="M3.5 9A2.5 2.5 0 0 1 6 6.5h6.2c.8 0 1.5.4 2 1l1.6 2H26A2.5 2.5 0 0 1 28.5 12v2h-25z" fill={C.yellowDeep} />
      <rect x="3.5" y="11.5" width="25" height="15" rx="3" fill={C.yellow} />
      <path d="M3.5 23h25v.5a3 3 0 0 1-3 3h-19a3 3 0 0 1-3-3z" fill={C.yellowDeep} opacity="0.7" />
      <rect x="7" y="14.5" width="9" height="2.6" rx="1.3" fill={C.yellowLight} />
    </svg>
  )
}

export function IconTime({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="13.5" y="2.5" width="5" height="4" rx="1.5" fill={C.orangeDeep} />
      <circle cx="16" cy="18" r="12" fill={C.orange} />
      <circle cx="16" cy="18" r="8.5" fill={C.white} />
      <path d="M16 18v-5M16 18l3.5 2.2" stroke={C.ink} strokeWidth="2.4" strokeLinecap="round" />
      <circle cx="16" cy="18" r="1.6" fill={C.orange} />
    </svg>
  )
}

export function IconBilling({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="3" y="8" width="26" height="17" rx="3.5" fill={C.green} />
      <path d="M3 21.5h26v.5a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3z" fill={C.greenDeep} />
      <circle cx="16" cy="16" r="4.6" fill={C.greenLight} />
      <path d="M16 13.2v5.6M17.6 14.3c-.4-.5-1-.7-1.6-.7-.9 0-1.6.5-1.6 1.2 0 1.6 3.3.9 3.3 2.5 0 .7-.8 1.2-1.7 1.2-.7 0-1.3-.3-1.7-.8" stroke={C.greenDeep} strokeWidth="1.3" strokeLinecap="round" fill="none" />
      <circle cx="7.5" cy="12" r="1.4" fill={C.greenLight} />
      <circle cx="24.5" cy="20" r="1.4" fill={C.greenLight} />
    </svg>
  )
}

export function IconReports({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="4" y="16" width="6.5" height="11" rx="2" fill={C.blue} />
      <rect x="12.75" y="10" width="6.5" height="17" rx="2" fill={C.green} />
      <rect x="21.5" y="4" width="6.5" height="23" rx="2" fill={C.orange} />
      <rect x="4" y="24" width="6.5" height="3" rx="1.5" fill={C.blueDeep} />
      <rect x="12.75" y="24" width="6.5" height="3" rx="1.5" fill={C.greenDeep} />
      <rect x="21.5" y="24" width="6.5" height="3" rx="1.5" fill={C.orangeDeep} />
    </svg>
  )
}

export function IconCommissions({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <ellipse cx="13" cy="22" rx="9.5" ry="4.5" fill={C.yellowDeep} />
      <rect x="3.5" y="17" width="19" height="5" fill={C.yellowDeep} />
      <ellipse cx="13" cy="17" rx="9.5" ry="4.5" fill={C.yellow} />
      <ellipse cx="13" cy="12.5" rx="9.5" ry="4.5" fill={C.yellowDeep} />
      <rect x="3.5" y="8" width="19" height="4.5" fill={C.yellowDeep} />
      <ellipse cx="13" cy="8" rx="9.5" ry="4.5" fill={C.yellow} />
      <circle cx="23" cy="20" r="6.5" fill={C.orange} />
      <circle cx="23" cy="20" r="4" fill={C.orangeLight} />
    </svg>
  )
}

export function IconTax({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="6" y="3" width="20" height="26" rx="4" fill={C.purple} />
      <path d="M6 25h20v.5a3.5 3.5 0 0 1-3.5 3.5h-13A3.5 3.5 0 0 1 6 25.5z" fill={C.purpleDeep} />
      <rect x="9.5" y="6.5" width="13" height="6" rx="1.8" fill={C.white} />
      {[0, 1, 2].map(r => [0, 1, 2].map(c => (
        <rect key={`${r}${c}`} x={9.5 + c * 4.6} y={15 + r * 3.8} width="3.4" height="2.6" rx="1" fill={r === 2 && c === 2 ? C.orange : C.white} opacity={r === 2 && c === 2 ? 1 : 0.85} />
      )))}
    </svg>
  )
}

export function IconReceipt({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <path d="M7 4.5A1.5 1.5 0 0 1 8.5 3h15A1.5 1.5 0 0 1 25 4.5V28l-3-2-3 2-3-2-3 2-3-2-3 2z" fill={C.blueLight} />
      <rect x="10.5" y="8" width="11" height="2.6" rx="1.3" fill={C.blueDeep} />
      <rect x="10.5" y="13" width="7" height="2.6" rx="1.3" fill={C.blue} />
      <rect x="10.5" y="18" width="9" height="2.6" rx="1.3" fill={C.blue} />
    </svg>
  )
}

export function IconSettings({ className = '' }: IconProps) {
  const teeth = Array.from({ length: 8 }, (_, i) => i * 45)
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      {teeth.map(a => <rect key={a} x="13.5" y="2.5" width="5" height="7" rx="1.8" fill={C.grey} transform={`rotate(${a} 16 16)`} />)}
      <circle cx="16" cy="16" r="10" fill={C.grey} />
      <path d="M26 16a10 10 0 0 1-18.7 5 10.4 10.4 0 0 0 18.7-5z" fill={C.greyDeep} />
      <circle cx="16" cy="16" r="4.2" fill={C.white} />
    </svg>
  )
}

// ---------------------------------------------------------------- Stat icons

export function IconSend({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <path d="M3.5 14.5 28.5 4l-6 23-7.5-7.5z" fill={C.blue} />
      <path d="M15 19.5 28.5 4 12.8 21.8z" fill={C.blueDeep} />
      <path d="M12.8 21.8 15 19.5l3 3-4.6 3.9z" fill={C.blueDeep} />
    </svg>
  )
}

export function IconAlert({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <circle cx="16" cy="16" r="13" fill={C.red} />
      <path d="M29 16a13 13 0 0 1-24.6 5.9A13.5 13.5 0 0 0 29 16z" fill={C.redDeep} />
      <rect x="14" y="8" width="4" height="10" rx="2" fill={C.white} />
      <circle cx="16" cy="22.5" r="2.2" fill={C.white} />
    </svg>
  )
}

export function IconTrophy({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <path d="M8 7H4.5v3a5 5 0 0 0 5 5M24 7h3.5v3a5 5 0 0 1-5 5" stroke={C.yellowDeep} strokeWidth="2.6" fill="none" strokeLinecap="round" />
      <path d="M8 4h16v8a8 8 0 0 1-16 0z" fill={C.yellow} />
      <path d="M24 11a8 8 0 0 1-15.2 3.5A8 8 0 0 0 24 12z" fill={C.yellowDeep} />
      <rect x="14" y="19" width="4" height="4" fill={C.yellowDeep} />
      <rect x="9.5" y="23" width="13" height="5" rx="2" fill={C.orange} />
      <ellipse cx="12" cy="8" rx="1.6" ry="2.6" fill={C.white} opacity="0.5" />
    </svg>
  )
}

export function IconHourglass({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="6" y="3" width="20" height="4" rx="2" fill={C.orangeDeep} />
      <rect x="6" y="25" width="20" height="4" rx="2" fill={C.orangeDeep} />
      <path d="M9 7h14v2.5c0 3-3 5-5.5 6.5 2.5 1.5 5.5 3.5 5.5 6.5V25H9v-2.5c0-3 3-5 5.5-6.5C12 14.5 9 12.5 9 9.5z" fill={C.blueLight} />
      <path d="M12 23.5c0-2 2-3.3 4-4.3 2 1 4 2.3 4 4.3V25h-8z" fill={C.orange} />
      <path d="M12.5 9.5h7c-.6 1.4-2 2.5-3.5 3.4-1.5-.9-2.9-2-3.5-3.4z" fill={C.orange} />
    </svg>
  )
}

export function Heart({ className = '', color = '#FF4B4B' }: IconProps & { color?: string }) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <path d="M16 28S3 20.2 3 11.5A7 7 0 0 1 16 7.6 7 7 0 0 1 29 11.5C29 20.2 16 28 16 28z" fill={color} />
      <ellipse cx="9.5" cy="11" rx="2.2" ry="3" fill="#fff" opacity="0.35" transform="rotate(-30 9.5 11)" />
    </svg>
  )
}

export function Star({ className = '', color = '#FFC800' }: IconProps & { color?: string }) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <path d="M16 3l3.9 8 8.6 1.2-6.3 6 1.5 8.6L16 22.7l-7.7 4.1 1.5-8.6-6.3-6 8.6-1.2z" fill={color} strokeLinejoin="round" />
    </svg>
  )
}

/** Content HQ: a clapperboard, for the content workspace. */
export function IconContent({ className = '' }: IconProps) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={className}>
      <rect x="3.5" y="12" width="25" height="15.5" rx="3" fill={C.purple} />
      <path d="M3.5 24h25v.5a3 3 0 0 1-3 3h-19a3 3 0 0 1-3-3z" fill={C.purpleDeep} />
      <g transform="rotate(-10 4 11)">
        <rect x="3.5" y="5.5" width="25" height="6" rx="2" fill="#4B4B4B" />
        <path d="M8 5.5h4l-3 6H5zM16 5.5h4l-3 6h-4zM24 5.5h4l-3 6h-4z" fill="#FFFFFF" />
      </g>
      <path d="M13.5 16.5v6.5l5.5-3.25z" fill="#FFFFFF" />
    </svg>
  )
}
