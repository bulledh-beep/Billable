import { useLayoutEffect, useRef, type RefObject } from 'react'
import { ProductMark } from './ProductMarks'
import type { SuiteApp } from './SuiteSwitch'

/**
 * Moving between the two apps. The app showing eases back towards the icon
 * just chosen and fades, while the other eases in from the same point, the
 * icon lifting with a soft glow of its app's colour - a short, light move
 * suited to a large window, where Content HQ's phone glides all the way into
 * the icon. Both apps stay loaded in the window, so this moves the real
 * screens, live.
 *
 * Only transform and opacity animate, which the GPU composites without
 * repainting: the rounded corners are a fixed clip for the length of the move,
 * not an animated one, and nothing is blurred - the window is big and sits on
 * macOS's own vibrancy. Under Reduce Motion there is no glide (shouldGlide).
 */

export type Glide = { key: number; from: SuiteApp; to: SuiteApp; x: number; y: number; icon: number }

/** The app showing eases back and is gone in this long... */
const LEAVE_MS = 240
/** ...and the other starts in this far into it, so the two cross. */
const ENTER_FROM_MS = 110
const ENTER_MS = 420
const COLOR: Record<SuiteApp, string> = { billable: '#ff9a1a', content: '#1c9a8b' }

export function shouldGlide() {
  return !window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

/** The ground of the app being opened, a shade darker. */
function ground(to: SuiteApp) {
  const dark = document.documentElement.classList.contains('dark')
  const base = to === 'billable' ? 'rgb(var(--bg))' : dark ? '#0b1415' : '#eef4f2'
  return `color-mix(in srgb, ${base} 94%, #000)`
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
    const shadow = `drop-shadow(0 3px 10px color-mix(in srgb, ${COLOR[glide.to]} 50%, transparent))`
    const origin = `${glide.x}px ${glide.y}px`
    // For the length of the move: corners clipped, both on their own layer.
    for (const layer of [out, into]) {
      layer.style.transformOrigin = origin
      layer.style.borderRadius = '12px'
      layer.style.overflow = 'hidden'
      layer.style.willChange = 'transform, opacity'
    }
    into.style.zIndex = '2'
    const running: Animation[] = []
    const run = (el: Element, keyframes: Keyframe[], options: KeyframeAnimationOptions) => {
      const a = el.animate(keyframes, { fill: 'both', ...options })
      running.push(a)
      return a
    }

    run(out, [
      { transform: 'none', opacity: 1 },
      { transform: 'scale(0.92)', opacity: 0 },
    ], { duration: LEAVE_MS, easing: 'cubic-bezier(0.4, 0, 1, 1)' })
    run(into, [
      { transform: 'scale(0.92)', opacity: 0 },
      { opacity: 1, offset: 0.4 },
      { transform: 'none', opacity: 1 },
    ], { duration: ENTER_MS, delay: ENTER_FROM_MS, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' })
    // The icon lifts with its glow as the apps cross, then gives way.
    run(mark, [{ transform: 'scale(0.75)', opacity: 1, filter: 'none' }, { transform: 'none', opacity: 1, filter: shadow }], { duration: 200, easing: 'cubic-bezier(0.3, 0.7, 0.4, 1)' })
    run(mark, [{ transform: 'none', opacity: 1, filter: shadow }, { transform: 'scale(0.75)', opacity: 0, filter: 'none' }], { duration: 260, delay: 220, easing: 'ease-out', fill: 'forwards' })
    run(glow, [{ transform: 'scale(0.3)', opacity: 0 }, { transform: 'scale(0.8)', opacity: 0.45 }], { duration: 220, easing: 'ease-out' })
    run(glow, [{ transform: 'scale(0.8)', opacity: 0.45 }, { transform: 'scale(1.4)', opacity: 0 }], { duration: 320, delay: 200, easing: 'ease-out', fill: 'forwards' })

    let cancelled = false
    Promise.all(running.map(a => a.finished)).then(() => { if (!cancelled) onDone() }, () => undefined)
    return () => {
      cancelled = true
      for (const a of running) a.cancel()
      for (const layer of [out, into]) {
        layer.style.transformOrigin = ''
        layer.style.borderRadius = ''
        layer.style.overflow = ''
        layer.style.willChange = ''
      }
      into.style.zIndex = ''
    }
    // A glide runs once, start to finish.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [glide.key])

  const size = Math.round(glide.icon * (4 / 3))
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
