import { useRef, useState, useEffect } from 'react'
import { NavLink, useLocation } from 'react-router-dom'
import { motion, AnimatePresence } from 'framer-motion'
import {
  LayoutDashboard,
  Users,
  FolderKanban,
  Clock,
  FileText,
  BarChart3,
  Settings,
  Calculator,
  Receipt,
  Square,
  CreditCard,
} from 'lucide-react'
import toast from 'react-hot-toast'
import ProfileSwitcher from './ProfileSwitcher'
import UpdateBanner from './UpdateBanner'

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
  elapsed: string
  activeProjectName?: string
  onStopTimer: () => void
}

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/billing', icon: CreditCard, label: 'Billing' },
  { to: '/clients', icon: Users, label: 'Clients' },
  { to: '/projects', icon: FolderKanban, label: 'Projects' },
  { to: '/time', icon: Clock, label: 'Time' },
  { to: '/invoices', icon: FileText, label: 'Invoices' },
  { to: '/reports', icon: BarChart3, label: 'Reports' },
]

const businessItems = [
  { to: '/tax-overview', icon: Calculator, label: 'Tax Overview' },
  { to: '/tax-settings', icon: Receipt, label: 'Tax Settings' },
]

export default function Sidebar({ isRunning, elapsed, activeProjectName, onStopTimer }: SidebarProps) {
  const location = useLocation()
  const clickTimesRef = useRef<number[]>([])
  const [spinCount, setSpinCount] = useState(0)
  const [showRing, setShowRing] = useState(false)
  const [billTrackingEnabled, setBillTrackingEnabled] = useState(true)

  useEffect(() => {
    const loadSettings = async () => {
      try {
        const s = await window.api.settings.get()
        setBillTrackingEnabled(s?.bill_tracking_enabled !== '0')
      } catch (err) {
        console.error(err)
      }
    }
    loadSettings()
    window.addEventListener('settings-updated', loadSettings)
    return () => window.removeEventListener('settings-updated', loadSettings)
  }, [])

  const handleLogoClick = (e: React.MouseEvent<HTMLAnchorElement>) => {
    const now = Date.now()
    // Drop click timestamps older than the rolling window
    clickTimesRef.current = clickTimesRef.current.filter(t => now - t < EGG_CLICK_WINDOW_MS)
    clickTimesRef.current.push(now)

    if (clickTimesRef.current.length >= EGG_CLICK_THRESHOLD) {
      // 🥚 fire — keep the user on the current page so they can enjoy it in context
      e.preventDefault()
      clickTimesRef.current = []
      setSpinCount(c => c + 1)
      setShowRing(true)
      window.setTimeout(() => setShowRing(false), 900)
      const msg = SECRET_MESSAGES[Math.floor(Math.random() * SECRET_MESSAGES.length)]
      toast.success(msg, { icon: '✨', duration: 2500 })
    }
  }

  return (
    <aside className="w-56 flex-shrink-0 bg-surface border-r border-rim/[0.04] flex flex-col h-full">
      {/* Traffic light spacer — drag-only, no content */}
      <div className="drag-region h-[52px] flex-shrink-0" />

      {/* Logo — clicks through to the Dashboard (and there's an easter egg) */}
      <NavLink
        to="/"
        end
        onClick={handleLogoClick}
        className="px-5 pb-3 flex items-center gap-2.5 flex-shrink-0 group no-drag"
        title="Go to Dashboard"
      >
        <div className="relative w-7 h-7 flex-shrink-0">
          {/* Pulse ring on egg fire */}
          <AnimatePresence>
            {showRing && (
              <motion.div
                key="egg-ring"
                initial={{ scale: 0.85, opacity: 0.7 }}
                animate={{ scale: 2.6, opacity: 0 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.9, ease: 'easeOut' }}
                className="absolute inset-0 rounded-lg border-2 border-accent pointer-events-none"
              />
            )}
          </AnimatePresence>
          <motion.div
            animate={{ rotate: spinCount * 360 }}
            transition={{ duration: 0.7, ease: [0.4, 0, 0.2, 1] }}
            className="w-7 h-7 rounded-lg bg-accent flex items-center justify-center transition-transform group-hover:scale-105 group-active:scale-95"
          >
            <Clock className="w-4 h-4 text-surface" />
          </motion.div>
        </div>
        <span className="text-base font-semibold text-text-primary tracking-tight group-hover:text-accent transition-colors">
          Billable
        </span>
      </NavLink>

      {/* Profile switcher */}
      <ProfileSwitcher
        isTimerRunning={isRunning}
        onStopTimer={onStopTimer as unknown as () => Promise<unknown>}
      />

      {/* Update banner (only shown when an update is available) */}
      <UpdateBanner />

      {/* Navigation */}
      <nav className="flex-1 px-3 py-2 space-y-0.5 overflow-y-auto">
        {navItems
          .filter(item => item.to !== '/billing' || billTrackingEnabled)
          .map(({ to, icon: Icon, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === '/'}
              className={({ isActive }) =>
                `sidebar-link ${isActive ? 'active' : ''}`
              }
            >
              <Icon className="w-4.5 h-4.5 flex-shrink-0" style={{ width: 18, height: 18 }} />
              <span>{label}</span>
            </NavLink>
          ))}

        <div className="pt-4 pb-1 px-2 text-[10px] font-semibold uppercase tracking-wider text-text-tertiary">
          Business
        </div>
        {businessItems.map(({ to, icon: Icon, label }) => (
          <NavLink
            key={to}
            to={to}
            className={({ isActive }) =>
              `sidebar-link ${isActive ? 'active' : ''}`
            }
          >
            <Icon className="w-4.5 h-4.5 flex-shrink-0" style={{ width: 18, height: 18 }} />
            <span>{label}</span>
          </NavLink>
        ))}
      </nav>

      {/* Active Timer */}
      {isRunning && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          className="mx-3 mb-2 p-3 rounded-xl bg-accent/[0.08] border border-accent/20"
        >
          <div className="flex items-center justify-between mb-1.5">
            <div className="flex items-center gap-1.5">
              <motion.div
                animate={{ scale: [1, 1.3, 1] }}
                transition={{ duration: 2, repeat: Infinity }}
                className="w-2 h-2 rounded-full bg-accent"
              />
              <span className="text-xs text-accent font-medium">Recording</span>
            </div>
            <button
              onClick={onStopTimer}
              className="p-1 hover:bg-accent/20 rounded transition-colors no-drag"
            >
              <Square className="w-3 h-3 text-accent fill-accent" />
            </button>
          </div>
          <div className="font-mono text-lg text-accent font-medium tracking-wider">
            {elapsed}
          </div>
          {activeProjectName && (
            <div className="text-xs text-text-secondary mt-1 truncate">
              {activeProjectName}
            </div>
          )}
        </motion.div>
      )}

      {/* Settings */}
      <div className="px-3 pb-4">
        <NavLink
          to="/settings"
          className={({ isActive }) =>
            `sidebar-link ${isActive ? 'active' : ''}`
          }
        >
          <Settings className="w-4.5 h-4.5 flex-shrink-0" style={{ width: 18, height: 18 }} />
          <span>Settings</span>
        </NavLink>
      </div>
    </aside>
  )
}
