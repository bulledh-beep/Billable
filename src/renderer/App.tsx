import { Routes, Route, Navigate, useNavigate, useLocation } from 'react-router-dom'
import { useEffect, useRef } from 'react'
import { Toaster } from 'react-hot-toast'
import Sidebar from './components/Sidebar'
import TimerControl from './components/TimerControl'
import { HeaderSlotsProvider, useHeaderSlots } from './components/PageHeader'
import Dashboard from './pages/Dashboard'
import Clients from './pages/Clients'
import ClientDetail from './pages/ClientDetail'
import Projects from './pages/Projects'
import ProjectDetail from './pages/ProjectDetail'
import TimeTracking from './pages/TimeTracking'
import Billing from './pages/Billing'
import InvoiceDetail from './pages/InvoiceDetail'
import InvoiceCreate from './pages/InvoiceCreate'
import Reports from './pages/Reports'
import SettingsPage from './pages/Settings'
import TaxSettingsPage from './pages/TaxSettings'
import TaxOverviewPage from './pages/TaxOverview'
import Commissions from './pages/Commissions'
import WhatsNewModal from './components/WhatsNewModal'
import { useTimer } from './hooks/useTimer'
import { notifyBillingChanged } from './utils/events'

export default function App() {
  const navigate = useNavigate()
  const location = useLocation()
  const mainRef = useRef<HTMLElement>(null)
  const { slots, titleRef, actionsRef } = useHeaderSlots()
  const {
    activeEntry,
    isRunning,
    isPaused,
    startTimer,
    stopTimer,
    pauseTimer,
    resumeTimer,
    checkActive,
  } = useTimer()

  useEffect(() => {
    const unsub = window.api.on('navigate', (path: string) => {
      navigate(path)
    })
    return () => { unsub?.() }
  }, [navigate])

  // New page, start at the top
  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 })
  }, [location.pathname])

  // Stopping a timer creates billable time, so refresh money totals
  useEffect(() => { notifyBillingChanged() }, [isRunning, isPaused])

  const timerProps = {
    onStartTimer: startTimer,
    onStopTimer: stopTimer,
    onPauseTimer: pauseTimer,
    onResumeTimer: resumeTimer,
    isTimerRunning: isRunning,
    isTimerPaused: isPaused,
    activeEntry,
  }

  return (
    <HeaderSlotsProvider value={slots}>
      <div className="flex h-screen text-fg overflow-hidden">
        <Sidebar isRunning={isRunning || isPaused} onStopTimer={stopTimer} />

        <div className="flex-1 flex flex-col min-w-0 bg-bg border-l border-line">
          {/* Window toolbar: page title and actions portal in here */}
          <header className="drag-region h-[52px] shrink-0 flex items-center gap-3 pl-6 pr-4 border-b border-line bg-bg z-10">
            <div ref={titleRef} className="flex-1 min-w-0 flex items-center" />
            <div ref={actionsRef} className="toolbar-actions no-drag flex items-center gap-2" />
            <div className="toolbar-divider w-px h-4 bg-line-strong" aria-hidden="true" />
            <TimerControl
              entry={activeEntry}
              isRunning={isRunning}
              isPaused={isPaused}
              onPause={pauseTimer}
              onResume={resumeTimer}
              onStop={stopTimer}
              onStart={startTimer}
            />
          </header>

          <main ref={mainRef} className="flex-1 overflow-y-auto">
            <Routes>
              <Route path="/" element={<Dashboard {...timerProps} />} />
              <Route path="/clients" element={<Clients />} />
              <Route path="/clients/:id" element={<ClientDetail />} />
              <Route path="/projects" element={<Projects {...timerProps} />} />
              <Route path="/projects/:id" element={<ProjectDetail {...timerProps} />} />
              <Route path="/time" element={<TimeTracking {...timerProps} checkActive={checkActive} />} />
              <Route path="/billing" element={<Billing />} />
              <Route path="/invoices" element={<Navigate to="/billing" replace />} />
              {/* Keyed so switching between new and edit never reuses a half-filled builder */}
              <Route path="/invoices/new" element={<InvoiceCreate key={`new${location.search}`} />} />
              <Route path="/invoices/:id/edit" element={<InvoiceCreate key={location.pathname} />} />
              <Route path="/invoices/:id" element={<InvoiceDetail />} />
              <Route path="/reports" element={<Reports isTimerRunning={isRunning} isTimerPaused={isPaused} />} />
              <Route path="/commissions" element={<Commissions />} />
              <Route path="/tax-overview" element={<TaxOverviewPage />} />
              <Route path="/tax-settings" element={<TaxSettingsPage />} />
              <Route path="/settings" element={<SettingsPage />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </main>
        </div>
      </div>

      <Toaster
        position="bottom-right"
        gutter={8}
        toastOptions={{
          duration: 3200,
          style: {
            background: 'rgb(var(--panel))',
            color: 'rgb(var(--fg))',
            border: 'none',
            boxShadow: 'var(--shadow-pop)',
            borderRadius: '10px',
            fontSize: '13px',
            padding: '8px 12px',
            maxWidth: '420px',
          },
          success: { iconTheme: { primary: 'rgb(var(--green))', secondary: 'rgb(var(--panel))' } },
          error: { iconTheme: { primary: 'rgb(var(--red))', secondary: 'rgb(var(--panel))' } },
        }}
      />
      <WhatsNewModal />
    </HeaderSlotsProvider>
  )
}
