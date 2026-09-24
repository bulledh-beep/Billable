import { useEffect, useMemo, useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { Mascot, Heart, Star, MASCOT_PARTY } from './Illustrations'
import { burst } from '../utils/burst'

const COLORS = ['#FF4B4B', '#FF9600', '#FFC800', '#58CC02', '#1CB0F6', '#CE82FF', '#FF86C8']

type Balloon = { id: number; kind: 'heart' | 'star' | 'mascot'; left: number; size: number; delay: number; duration: number; drift: number; spin: number; color: string }

function makeBalloons(): Balloon[] {
  return Array.from({ length: 30 }, (_, i) => ({
    id: i,
    kind: i % 7 === 0 ? 'mascot' : i % 2 === 0 ? 'heart' : 'star',
    left: 3 + Math.random() * 94,
    size: 26 + Math.random() * 38,
    delay: Math.random() * 0.9,
    duration: 2.3 + Math.random() * 1.3,
    drift: (Math.random() - 0.5) * 160,
    spin: (Math.random() - 0.5) * 70,
    color: COLORS[i % COLORS.length],
  }))
}

/**
 * The secret party: click the mascot five times quickly and a giant one pops
 * up, rings its bells and sends hearts and stars floating up the window.
 */
export default function MascotParty() {
  const [run, setRun] = useState(0)
  const [open, setOpen] = useState(false)

  useEffect(() => {
    const start = () => {
      if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return
      setRun(r => r + 1)
      setOpen(true)
    }
    window.addEventListener(MASCOT_PARTY, start)
    return () => window.removeEventListener(MASCOT_PARTY, start)
  }, [])

  useEffect(() => {
    if (!open) return
    const close = () => setOpen(false)
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') close() }
    window.addEventListener('keydown', onKey)
    const pop = window.setTimeout(() => burst(window.innerWidth / 2, window.innerHeight * 0.55, 70), 420)
    const done = window.setTimeout(close, 3800)
    return () => { window.removeEventListener('keydown', onKey); window.clearTimeout(pop); window.clearTimeout(done) }
  }, [open, run])

  const balloons = useMemo(makeBalloons, [run])
  const giant = Math.round(Math.min(window.innerHeight * 0.52, 440))

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key={run}
          className="fixed inset-0 z-[90] overflow-hidden cursor-pointer"
          onClick={() => setOpen(false)}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0, transition: { duration: 0.35 } }}
          aria-hidden="true"
        >
          <div className="absolute inset-0 bg-bg/75 backdrop-blur-[3px]" />

          {/* Hearts, stars and little mascots floating up */}
          {balloons.map(b => (
            <div
              key={b.id}
              className="absolute bottom-[-80px]"
              style={{
                left: `${b.left}%`,
                width: b.size,
                height: b.size,
                ['--drift' as any]: `${b.drift}px`,
                ['--spin' as any]: `${b.spin}deg`,
                animation: `float-up ${b.duration}s cubic-bezier(0.3, 0.6, 0.5, 1) ${b.delay}s both`,
              }}
            >
              {b.kind === 'heart' ? <Heart color={b.color} className="w-full h-full" />
                : b.kind === 'star' ? <Star color={b.color} className="w-full h-full" />
                : <Mascot size={b.size} mood="happy" />}
            </div>
          ))}

          {/* The giant mascot */}
          <div className="absolute inset-0 flex items-end justify-center pb-[8vh] pointer-events-none">
            <motion.div
              className="relative"
              initial={{ y: '120%', rotate: -8, scale: 0.7 }}
              animate={{ y: 0, rotate: 0, scale: 1, transition: { type: 'spring', stiffness: 220, damping: 13 } }}
              exit={{ y: '130%', rotate: 10, transition: { duration: 0.35, ease: [0.5, 0, 0.9, 0.5] } }}
            >
              <motion.div
                animate={{ y: [0, -46, 0, -22, 0], scaleY: [1, 1.04, 0.94, 1.02, 1] }}
                transition={{ delay: 0.55, duration: 1.1, ease: 'easeOut' }}
              >
                <Mascot size={giant} mood="happy" ringing />
              </motion.div>
              <motion.div
                className="absolute -top-2 left-[78%] whitespace-nowrap rounded-[20px] bg-panel shadow-pop px-5 py-3 text-[22px] font-bold text-fg"
                initial={{ scale: 0, rotate: -12, opacity: 0 }}
                animate={{ scale: 1, rotate: -4, opacity: 1, transition: { delay: 0.5, type: 'spring', stiffness: 420, damping: 16 } }}
              >
                Ring ring! You're doing great!
                <span className="absolute -left-2 bottom-4 w-5 h-5 rotate-45 bg-panel rounded-[3px]" />
              </motion.div>
            </motion.div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}
