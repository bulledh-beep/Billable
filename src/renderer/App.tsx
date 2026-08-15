import { Routes, Route, useNavigate } from 'react-router-dom'
import { useEffect } from 'react'
import { Toaster } from 'react-hot-toast'
import { AnimatePresence } from 'framer-motion'
import Sidebar from './components/Sidebar'
import Dashboard from './pages/Dashboard'
import Clients from './pages/Clients'
import ClientDetail from './pages/ClientDetail'
import Projects from './pages/Projects'
import ProjectDetail from './pages/ProjectDetail'
import TimeTracking from './pages/TimeTracking'
import Invoices from './pages/Invoices'
import InvoiceDetail from './pages/InvoiceDetail'
import InvoiceCreate from './pages/InvoiceCreate'
import Reports from './pages/Reports'
import SettingsPage from './pages/Settings'
import TaxSettingsPage from './pages/TaxSettings'
import TaxOverviewPage from './pages/TaxOverview'
import Commissions from './pages/Commissions'
import WhatsNewModal from './components/WhatsNewModal'
import TimerBar from './components/TimerBar'
import { useTimer } from './hooks/useTimer'

export default function App() {
  const navigate = useNavigate()
  const {
    activeEntry,
    elapsed,
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

  return (
    <div className="flex h-screen bg-surface overflow-hidden">
      <Sidebar
        isRunning={isRunning}
        isPaused={isPaused}
        elapsed={elapsed}
        activeProjectName={activeEntry?.project_name}
        onPauseTimer={pauseTimer}
        onResumeTimer={resumeTimer}
        onStopTimer={stopTimer}
      />
      <main className="flex-1 overflow-y-auto">
        <TimerBar
          entry={activeEntry}
          isRunning={isRunning}
          isPaused={isPaused}
          elapsed={elapsed}
          onPause={pauseTimer}
          onResume={resumeTimer}
          onStop={stopTimer}
        />
        <AnimatePresence mode="wait">
          <Routes>
            <Route path="/" element={
              <Dashboard
                onStartTimer={startTimer}
                onStopTimer={stopTimer}
                onPauseTimer={pauseTimer}
                onResumeTimer={resumeTimer}
                isTimerRunning={isRunning}
                isTimerPaused={isPaused}
                activeEntry={activeEntry}
              />
            } />
            <Route path="/clients" element={<Clients />} />
            <Route path="/clients/:id" element={<ClientDetail />} />
            <Route path="/projects" element={<Projects onStartTimer={startTimer} onStopTimer={stopTimer} onPauseTimer={pauseTimer} onResumeTimer={resumeTimer} isTimerRunning={isRunning} isTimerPaused={isPaused} activeEntry={activeEntry} />} />
            <Route path="/projects/:id" element={<ProjectDetail onStartTimer={startTimer} onStopTimer={stopTimer} onPauseTimer={pauseTimer} onResumeTimer={resumeTimer} isTimerRunning={isRunning} isTimerPaused={isPaused} activeEntry={activeEntry} elapsed={elapsed} />} />
            <Route path="/time" element={
              <TimeTracking
                onStartTimer={startTimer}
                onStopTimer={stopTimer}
                onPauseTimer={pauseTimer}
                onResumeTimer={resumeTimer}
                isTimerRunning={isRunning}
                isTimerPaused={isPaused}
                activeEntry={activeEntry}
                elapsed={elapsed}
                checkActive={checkActive}
              />
            } />
            <Route path="/invoices" element={<Invoices />} />
            <Route path="/invoices/new" element={<InvoiceCreate />} />
            <Route path="/invoices/:id/edit" element={<InvoiceCreate />} />
            <Route path="/invoices/:id" element={<InvoiceDetail />} />
            <Route path="/reports" element={<Reports isTimerRunning={isRunning} isTimerPaused={isPaused} />} />
            <Route path="/commissions" element={<Commissions />} />
            <Route path="/tax-overview" element={<TaxOverviewPage />} />
            <Route path="/tax-settings" element={<TaxSettingsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Routes>
        </AnimatePresence>
      </main>
      <Toaster
        position="bottom-right"
        toastOptions={{
          className: '!bg-surface-200 !text-text-primary !shadow-card',
          duration: 3000,
          style: {
            background: 'rgb(var(--surface-200))',
            color: 'rgb(var(--text-primary))',
            border: '1px solid rgb(var(--rim) / 0.06)',
          },
        }}
      />
      <WhatsNewModal />
    </div>
  )
}
