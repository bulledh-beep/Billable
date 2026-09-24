import { useEffect, useRef, useState } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { motion, AnimatePresence } from 'framer-motion'
import toast from 'react-hot-toast'
import ProfileSwitcher from './ProfileSwitcher'
import UpdateBanner from './UpdateBanner'
import {
  IconHome, IconClients, IconProjects, IconTime, IconBilling, IconReports,
  IconCommissions, IconTax, IconReceipt, IconSettings,
} from './Illustrations'
import { onBillingChanged } from '../utils/events'
import SuiteSwitch, { type SuiteApp } from './SuiteSwitch'

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
  onSwitchApp: (app: SuiteApp) => void
}

const navItems = [
  { to: '/', icon: IconHome, label: 'Dashboard' },
  { to: '/clients', icon: IconClients, label: 'Clients' },
  { to: '/projects', icon: IconProjects, label: 'Projects' },
  { to: '/time', icon: IconTime, label: 'Time' },
  { to: '/billing', icon: IconBilling, label: 'Billing', badge: true },
  { to: '/reports', icon: IconReports, label: 'Reports' },
]

const businessItems = [
  { to: '/commissions', icon: IconCommissions, label: 'Commissions' },
  { to: '/tax-overview', icon: IconTax, label: 'Tax overview' },
  { to: '/tax-settings', icon: IconReceipt, label: 'Tax settings' },
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

export default function Sidebar({ isRunning, onStopTimer, onSwitchApp }: SidebarProps) {
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
    <aside className="w-[228px] shrink-0 flex flex-col h-full bg-sidebar border-r border-line">
      {/* Traffic-light row doubles as the window drag handle; the app switch sits beside the buttons */}
      <div className="drag-region h-[48px] shrink-0 flex items-center pl-[78px]">
        <SuiteSwitch value="billable" onChange={onSwitchApp} />
      </div>

      {/* Wordmark, goes to the Dashboard (and hides an easter egg) */}
      <div className="px-5 pb-3 shrink-0">
        <NavLink to="/" end onClick={handleLogoClick} className="relative inline-flex items-center" title="Go to Dashboard">
          <AnimatePresence>
            {showRing && (
              <motion.div
                key="egg-ring"
                initial={{ scale: 0.85, opacity: 0.7 }}
                animate={{ scale: 1.6, opacity: 0 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.9, ease: 'easeOut' }}
                className="absolute -inset-2 rounded-[14px] border-2 border-accent pointer-events-none"
              />
            )}
          </AnimatePresence>
          <motion.span
            animate={{ rotate: spinCount * 360 }}
            transition={{ duration: 0.7, ease: [0.4, 0, 0.2, 1] }}
            className="text-[24px] leading-none font-bold tracking-[-0.03em] text-accent"
          >
            billable
          </motion.span>
        </NavLink>
      </div>

      <nav className="flex-1 overflow-y-auto px-3 pb-2">
        <div className="space-y-0.5">
          {navItems.map(({ to, icon: Icon, label, badge }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) => `sidebar-link wiggle-on-hover ${isActive || (badge && isBillingActive) ? 'active' : ''}`}
            >
              {({ isActive }) => (
                <>
              {(isActive || (badge && isBillingActive)) && <motion.span layoutId="nav-pill" className="nav-pill" transition={{ type: 'spring', stiffness: 500, damping: 38 }} />}
              <Icon className="wiggle-target" />
              <span className="flex-1 truncate">{label}</span>
              {badge && attention > 0 && (
                <span
                  key={attention}
                  className="pop num min-w-[20px] h-[20px] px-1.5 rounded-full bg-red text-white text-[11.5px] font-bold flex items-center justify-center"
                  title={`${attention} billing item${attention === 1 ? '' : 's'} need attention`}
                >
                  {attention}
                </span>
              )}
                </>
              )}
            </NavLink>
          ))}
        </div>

        <div className="sidebar-heading">Business</div>
        <div className="space-y-0.5">
          {businessItems.map(({ to, icon: Icon, label }) => (
            <NavLink key={to} to={to} className={({ isActive }) => `sidebar-link wiggle-on-hover ${isActive ? 'active' : ''}`}>
              {({ isActive }) => (
                <>
                  {isActive && <motion.span layoutId="nav-pill" className="nav-pill" transition={{ type: 'spring', stiffness: 500, damping: 38 }} />}
                  <Icon className="wiggle-target" />
                  <span className="truncate">{label}</span>
                </>
              )}
            </NavLink>
          ))}
        </div>
      </nav>

      <UpdateBanner />

      <div className="px-3 pt-1 shrink-0">
        <NavLink to="/settings" className={({ isActive }) => `sidebar-link wiggle-on-hover ${isActive ? 'active' : ''}`}>
          {({ isActive }) => (
            <>
              {isActive && <motion.span layoutId="nav-pill" className="nav-pill" transition={{ type: 'spring', stiffness: 500, damping: 38 }} />}
              <IconSettings className="wiggle-target" />
              <span className="flex-1">Settings</span>
              <span className="kbd">⌘,</span>
            </>
          )}
        </NavLink>
      </div>
      <div className="pt-1 pb-3 shrink-0">
        <ProfileSwitcher isTimerRunning={isRunning} onStopTimer={onStopTimer} />
      </div>
    </aside>
  )
}
