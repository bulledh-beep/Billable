import { useEffect, useRef, useState } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { motion, AnimatePresence } from 'framer-motion'
import {
  LayoutDashboard,
  Users,
  FolderKanban,
  Clock,
  Wallet,
  BarChart3,
  Settings,
  Calculator,
  Receipt,
  HandCoins,
} from 'lucide-react'
import toast from 'react-hot-toast'
import ProfileSwitcher from './ProfileSwitcher'
import UpdateBanner from './UpdateBanner'
import { LiveClockMark } from './Dial'
import { onBillingChanged } from '../utils/events'

// 🥚 Tap the logo 7 times in 3 seconds to discover this.
const SECRET_MESSAGES = [
  'Stay billable. ✨',
  "Time is money — and you're rich.",
  'Hours saved. Soul intact.',
  '💰 Cha-ching simulator activated.',
  'Keep tracking. Keep cooking. 🔥',
  'Invoice every minute. Even this one.',
  'Your future bookkeeper says thank you.',
  '🎩 Magic happens here.',
  'You found me.',
  'Freelance like nobody is watching.',
]
const EGG_CLICK_THRESHOLD = 7
const EGG_CLICK_WINDOW_MS = 3000

interface SidebarProps {
  isRunning: boolean
  onStopTimer: () => Promise<unknown>
}

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/clients', icon: Users, label: 'Clients' },
  { to: '/projects', icon: FolderKanban, label: 'Projects' },
  { to: '/time', icon: Clock, label: 'Time' },
  { to: '/billing', icon: Wallet, label: 'Billing', badge: true },
  { to: '/reports', icon: BarChart3, label: 'Reports' },
]

const businessItems = [
  { to: '/commissions', icon: HandCoins, label: 'Commissions' },
  { to: '/tax-overview', icon: Calculator, label: 'Tax overview' },
  { to: '/tax-settings', icon: Receipt, label: 'Tax settings' },
]

/** How many billing items need a look. Refreshes on navigation, focus, and billing changes. */
function useAttentionCount(isRunning: boolean) {
  const location = useLocation()
  const [count, setCount] = useState(0)
  useEffect(() => {
    let alive = true
    const load = () => window.api.billing.overview()
      .then((o: any) => { if (alive) setCount(o.attention.filter((a: any) => a.kind !== 'duplicate_clients').length) })
      .catch(() => {})
    load()
    const off = onBillingChanged(load)
    window.addEventListener('focus', load)
    return () => { alive = false; off(); window.removeEventListener('focus', load) }
  }, [location.pathname, isRunning])
  return count
}

export default function Sidebar({ isRunning, onStopTimer }: SidebarProps) {
  const location = useLocation()
  const attention = useAttentionCount(isRunning)
  const clickTimesRef = useRef<number[]>([])
  const [spinCount, setSpinCount] = useState(0)
  const [showRing, setShowRing] = useState(false)

  const handleLogoClick = (e: React.MouseEvent<HTMLAnchorElement>) => {
    const now = Date.now()
    clickTimesRef.current = clickTimesRef.current.filter(t => now - t < EGG_CLICK_WINDOW_MS)
    clickTimesRef.current.push(now)

    if (clickTimesRef.current.length >= EGG_CLICK_THRESHOLD) {
      e.preventDefault()
      clickTimesRef.current = []
      setSpinCount(c => c + 1)
      setShowRing(true)
      window.setTimeout(() => setShowRing(false), 900)
      const msg = SECRET_MESSAGES[Math.floor(Math.random() * SECRET_MESSAGES.length)]
      toast.success(msg, { icon: '✨', duration: 2500 })
    }
  }

  const isBillingActive = location.pathname.startsWith('/billing') || location.pathname.startsWith('/invoices')

  return (
    <aside className="w-[220px] shrink-0 flex flex-col h-full">
      {/* Traffic-light row doubles as the window drag handle */}
      <div className="drag-region h-[44px] shrink-0" />

      {/* Brand, goes to the Dashboard (and hides an easter egg) */}
      <div className="px-2.5 shrink-0">
        <NavLink
          to="/"
          end
          onClick={handleLogoClick}
          className="flex items-center gap-2 h-[30px] px-2 rounded-[6px] hover:bg-fg/[0.05] transition-colors"
          title="Go to Dashboard"
        >
          <div className="relative w-[18px] h-[18px] shrink-0">
            <AnimatePresence>
              {showRing && (
                <motion.div
                  key="egg-ring"
                  initial={{ scale: 0.85, opacity: 0.7 }}
                  animate={{ scale: 2.6, opacity: 0 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.9, ease: 'easeOut' }}
                  className="absolute inset-0 rounded-[5px] border-2 border-accent pointer-events-none"
                />
              )}
            </AnimatePresence>
            <motion.div animate={{ rotate: spinCount * 360 }} transition={{ duration: 0.7, ease: [0.4, 0, 0.2, 1] }}>
              <LiveClockMark size={18} />
            </motion.div>
          </div>
          {/* Set like the maker's name on a watch dial */}
          <span className="font-stamp text-[15px] leading-none pt-[3px] tracking-[0.16em] text-fg">BILLABLE</span>
        </NavLink>
      </div>

      <nav className="flex-1 overflow-y-auto px-2.5 pt-3 pb-2">
        <div className="space-y-px">
          {navItems.map(({ to, icon: Icon, label, badge }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) => `sidebar-link ${isActive || (badge && isBillingActive) ? 'active' : ''}`}
            >
              <Icon strokeWidth={2} />
              <span className="flex-1">{label}</span>
              {badge && attention > 0 && (
                <span
                  className="num min-w-[18px] h-[16px] px-1.5 rounded-full bg-fg/[0.09] text-fg-2 text-2xs font-semibold flex items-center justify-center"
                  title={`${attention} billing item${attention === 1 ? '' : 's'} need attention`}
                >
                  {attention}
                </span>
              )}
            </NavLink>
          ))}
        </div>

        <div className="sidebar-heading">Business</div>
        <div className="space-y-px">
          {businessItems.map(({ to, icon: Icon, label }) => (
            <NavLink key={to} to={to} className={({ isActive }) => `sidebar-link ${isActive ? 'active' : ''}`}>
              <Icon strokeWidth={2} />
              <span>{label}</span>
            </NavLink>
          ))}
        </div>
      </nav>

      <UpdateBanner />

      <div className="px-2.5 pt-1 shrink-0">
        <NavLink to="/settings" className={({ isActive }) => `sidebar-link ${isActive ? 'active' : ''}`}>
          <Settings strokeWidth={2} />
          <span className="flex-1">Settings</span>
          <span className="text-2xs text-fg-4">⌘,</span>
        </NavLink>
      </div>
      <div className="pt-1 pb-2.5 shrink-0">
        <ProfileSwitcher isTimerRunning={isRunning} onStopTimer={onStopTimer} />
      </div>
    </aside>
  )
}
