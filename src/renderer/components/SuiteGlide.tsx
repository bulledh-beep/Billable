import { useLayoutEffect, useRef, type RefObject } from 'react'
import { ProductMark } from './ProductMarks'
import type { SuiteApp } from './SuiteSwitch'

/**
 * Moving between the two apps, the way a phone closes one app into its icon
 * and opens another out of its own - the same move as Content HQ's own switch.
 * The app showing glides down into the icon just chosen, its corners rounding
 * as it shrinks, while the icon lifts with a soft glow of its app's colour
 * over the other app's ground a shade darker; the other app then grows out of
 * the icon into place. Both apps stay loaded in the window, so this moves the
 * real screens, live. Nothing spins, flashes or overshoots; under Reduce
 * Motion there is no glide (see shouldGlide).
 */

export type Glide = { key: number; from: SuiteApp; to: SuiteApp; x: number; y: number; icon: number }

const RECEDE_MS = 360
/** The other app comes out as the one showing is all but gone. */
const EMERGE_FROM_MS = 280
const EMERGE_MS = 500
const COLOR: Record<SuiteApp, string> = { billable: '#ff9a1a', content: '#1c9a8b' }

export function shouldGlide() {
  return !window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/** The ground of the app being opened, a shade darker. */
function ground(to: SuiteApp) {
  const dark = document.documentElement.classList.contains('dark')
  const base = to === 'billable' ? 'rgb(var(--bg))' : dark ? '#0b1415' : '#eef4f2'
  return `color-mix(in srgb, ${base} 90%, #000)`
}

export default function SuiteGlide({ glide, layers, onDone }: {
  glide: Glide
  layers: Record<SuiteApp, RefObject<HTMLDivElement | null>>
  onDone: () => void
}) {
  const glowRef = useRef<HTMLDivElement>(null)
  const markRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const out = layers[glide.from].current
    const into = layers[glide.to].current
    const glow = glowRef.current
    const mark = markRef.current
    if (!out || !into || !glow || !mark) { onDone(); return }
    const shadow = `drop-shadow(0 4px 14px color-mix(in srgb, ${COLOR[glide.to]} 55%, transparent))`
    const origin = `${glide.x}px ${glide.y}px`
    out.style.transformOrigin = origin
    into.style.transformOrigin = origin
    into.style.zIndex = '2'
    const running: Animation[] = []
    const run = (el: Element, keyframes: Keyframe[], options: KeyframeAnimationOptions) => {
      const a = el.animate(keyframes, { fill: 'both', ...options })
      running.push(a)
      return a
    }

    // The app showing glides into the icon, easing in and out so the whole
    // way down is seen. Its corners are clipped in its own size, so they read
    // as a steady rounded card all the way and close to the icon's at the end.
    run(out, [
      { transform: 'none', clipPath: 'inset(0 round 0px)', opacity: 1, filter: 'none' },
      { clipPath: 'inset(0 round 44px)', offset: 0.4 },
      { opacity: 1, offset: 0.62 },
      { transform: 'scale(0.06)', clipPath: 'inset(0 round 120px)', opacity: 0, filter: 'blur(1.5px)' },
    ], { duration: RECEDE_MS, easing: 'cubic-bezier(0.42, 0, 0.58, 1)' })
    // ...and the other grows out of it into place.
    run(into, [
      { transform: 'scale(0.06)', clipPath: 'inset(0 round 120px)', opacity: 0, filter: 'blur(2px)' },
      { opacity: 1, offset: 0.14 },
      { filter: 'blur(0px)', offset: 0.4 },
      { clipPath: 'inset(0 round 36px)', offset: 0.62 },
      { transform: 'none', clipPath: 'inset(0 round 0px)', opacity: 1, filter: 'none' },
    ], { duration: EMERGE_MS, delay: EMERGE_FROM_MS, easing: 'cubic-bezier(0.2, 0.85, 0.25, 1)' })
    // The icon lifts with its glow, then settles back under the app.
    run(mark, [{ transform: 'scale(0.6667)', filter: 'none' }, { transform: 'none', filter: shadow }], { duration: 340, easing: 'cubic-bezier(0.3, 0.7, 0.4, 1)' })
    run(mark, [{ transform: 'none', opacity: 1, filter: shadow }, { transform: 'scale(0.6667)', opacity: 0, filter: 'none' }], { duration: 300, delay: EMERGE_FROM_MS + 40, easing: 'cubic-bezier(0.3, 0.7, 0.4, 1)', fill: 'forwards' })
    run(glow, [{ transform: 'scale(0.2)', opacity: 0 }, { transform: 'scale(0.7)', opacity: 0.55 }], { duration: 360, easing: 'cubic-bezier(0.3, 0.7, 0.4, 1)' })
    run(glow, [{ transform: 'scale(0.7)', opacity: 0.55 }, { transform: 'scale(1.5)', opacity: 0 }], { duration: 360, delay: EMERGE_FROM_MS, easing: 'ease-out', fill: 'forwards' })

    let cancelled = false
    Promise.all(running.map(a => a.finished)).then(() => { if (!cancelled) onDone() }, () => undefined)
    return () => {
      cancelled = true
      for (const a of running) a.cancel()
      out.style.transformOrigin = ''
      into.style.transformOrigin = ''
      into.style.zIndex = ''
    }
    // A glide runs once, start to finish.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [glide.key])

  const size = Math.round(glide.icon * 1.5)
  return (
    <>
      <div
        aria-hidden="true"
        className="absolute inset-0 z-0"
        style={{ background: `radial-gradient(75% 55% at ${glide.x}px ${glide.y}px, color-mix(in srgb, ${COLOR[glide.to]} 14%, transparent) 0%, transparent 72%), ${ground(glide.to)}` }}
      />
      <div aria-hidden="true" className="pointer-events-none absolute inset-0 z-[3]">
        <div
          ref={glowRef}
          className="absolute rounded-full"
          style={{
            left: glide.x - 180, top: glide.y - 180, width: 360, height: 360, opacity: 0,
            background: `radial-gradient(circle, color-mix(in srgb, ${COLOR[glide.to]} 50%, white) 0%, color-mix(in srgb, ${COLOR[glide.to]} 32%, transparent) 20%, transparent 64%)`,
          }}
        />
        <div ref={markRef} className="absolute" style={{ left: glide.x - size / 2, top: glide.y - size / 2, width: size, height: size }}>
          <ProductMark app={glide.to} size={size} />
        </div>
      </div>
    </>
  )
}
