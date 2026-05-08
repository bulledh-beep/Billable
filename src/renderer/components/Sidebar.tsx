import { NavLink, useLocation } from 'react-router-dom'
import { motion } from 'framer-motion'
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
} from 'lucide-react'
import ProfileSwitcher from './ProfileSwitcher'
import UpdateBanner from './UpdateBanner'

interface SidebarProps {
  isRunning: boolean
  elapsed: string
  activeProjectName?: string
  onStopTimer: () => void
}

const navItems = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
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

  return (
    <aside className="w-56 flex-shrink-0 bg-surface border-r border-rim/[0.04] flex flex-col h-full">
      {/* Traffic light spacer — drag-only, no content */}
      <div className="drag-region h-[52px] flex-shrink-0" />

      {/* Logo — clicks through to the Dashboard */}
      <NavLink
        to="/"
        end
        className="px-5 pb-3 flex items-center gap-2.5 flex-shrink-0 group no-drag"
        title="Go to Dashboard"
      >
        <div className="w-7 h-7 rounded-lg bg-accent flex items-center justify-center transition-transform group-hover:scale-105 group-active:scale-95">
          <Clock className="w-4 h-4 text-surface" />
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
        {navItems.map(({ to, icon: Icon, label }) => (
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
