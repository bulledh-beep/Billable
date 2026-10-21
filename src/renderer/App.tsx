import { Routes, Route, Navigate, useNavigate, useLocation } from 'react-router-dom'
import { useEffect, useRef, useState } from 'react'
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
import ContentHQView from './components/ContentHQView'
import SuiteBar from './components/SuiteBar'
import type { SuiteApp } from './components/SuiteSwitch'
import SuiteGlide, { shouldGlide, type Glide } from './components/SuiteGlide'
import WhatsNewModal from './components/WhatsNewModal'
import MascotParty from './components/MascotParty'
import PhoneConnectModal from './components/PhoneConnectModal'
import { useTimer } from './hooks/useTimer'
import { notifyBillingChanged } from './utils/events'
import toast from 'react-hot-toast'
import { Mascot } from './components/Illustrations'

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

  // Content HQ loads the first time you open it, then stays alive
  const onContentHq = location.pathname === '/content'
  const [contentHqOpened, setContentHqOpened] = useState(false)
  useEffect(() => { if (onContentHq) setContentHqOpened(true) }, [onContentHq])

  // The suite switch: remember where you were in Billable and which app was last open
  const lastBillableRoute = useRef('/')
  // Billable keeps showing where it was left while Content HQ is up, so it
  // can glide away (and back) as itself rather than as an empty page.
  const lastBillableLocation = useRef(location)
  useEffect(() => {
    if (!onContentHq) {
      lastBillableRoute.current = location.pathname + location.search
      lastBillableLocation.current = location
    }
    try { localStorage.setItem('billable.suite.app', onContentHq ? 'content' : 'billable') } catch { /* ignore */ }
  }, [location, onContentHq])
  const billableLocation = onContentHq ? lastBillableLocation.current : location

  // New Billable page, start at the top (going over to Content HQ and back is not one)
  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 })
  }, [billableLocation.pathname])

  // Moving between the apps glides from the chosen icon (SuiteGlide).
  const layers = { billable: useRef<HTMLDivElement>(null), content: useRef<HTMLDivElement>(null) }
  const [glide, setGlide] = useState<Glide | null>(null)
  const showing = useRef<SuiteApp>(onContentHq ? 'content' : 'billable')
  showing.current = onContentHq ? 'content' : 'billable'
  const switchApp = (app: SuiteApp, from?: DOMRect | null) => {
    const current = showing.current
    if (app === current) return
    if (app === 'content') setContentHqOpened(true)
    // From the menu there is no tap: start from the icon in the switch on screen.
    const icon = from ?? document.querySelector(`[data-suite-layer="${current}"] [data-suite-icon="${app}"]`)?.getBoundingClientRect() ?? null
    if (icon && icon.width > 0 && shouldGlide()) {
      setGlide({ key: Date.now(), from: current, to: app, x: icon.left + icon.width / 2, y: icon.top + icon.height / 2, icon: icon.width })
    }
    navigate(app === 'content' ? '/content' : lastBillableRoute.current || '/')
  }
  useEffect(() => {
    try { if (localStorage.getItem('billable.suite.app') === 'content') navigate('/content') } catch { /* ignore */ }
    const off = window.api.on('suite:switch', (app: SuiteApp) => switchApp(app))
    return () => { off?.() }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Stopping a timer creates billable time, so refresh money totals
  useEffect(() => { notifyBillingChanged() }, [isRunning, isPaused])

  // Changes made on the phone: refresh what's on screen and say what happened
  useEffect(() => {
    const off = window.api.on('phone:applied', (results: Array<{ status: string; summary: string; message?: string }>) => {
      notifyBillingChanged()
      checkActive()
      if (!results?.length) return
      const rejected = results.filter(r => r.status === 'rejected')
      const text = results.length === 1
        ? (rejected.length ? `Couldn’t apply a change from your phone: ${rejected[0].message || rejected[0].summary}` : `From your phone: ${results[0].summary}`)
        : `${results.length} changes from your phone${rejected.length ? `, ${rejected.length} couldn’t be applied` : ''}`
      toast(text, { icon: <Mascot size={26} mood={rejected.length ? 'worried' : 'wave'} />, duration: 4000 })
    })
    return () => { off?.() }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Starting a timer gets a small cheer from the mascot
  const startTimerWithCheer = async (projectId: number, description?: string) => {
    const entry = await startTimer(projectId, description)
    if (entry) toast(`On the clock for ${entry.project_name || 'your project'}`, { icon: <Mascot size={26} mood="happy" motion="hop" />, duration: 2500 })
    return entry
  }

  const timerProps = {
    onStartTimer: startTimerWithCheer,
    onStopTimer: stopTimer,
    onPauseTimer: pauseTimer,
    onResumeTimer: resumeTimer,
    isTimerRunning: isRunning,
    isTimerPaused: isPaused,
    activeEntry,
  }

  return (
    <HeaderSlotsProvider value={slots}>
      <div className="relative h-screen text-fg overflow-hidden">
        {/* Billable: always laid out, shown while it is the app showing or gliding. */}
        <div
          ref={layers.billable}
          data-suite-layer="billable"
          className={`absolute inset-0 flex z-[1] ${!onContentHq || glide ? '' : 'invisible pointer-events-none'}`}
          aria-hidden={onContentHq}
        >
          {/* Under the same location as Billable's pages, so its nav keeps its
            * place while Content HQ is up and as Billable glides away. */}
          <Routes location={billableLocation}>
            <Route path="*" element={<Sidebar isRunning={isRunning || isPaused} onStopTimer={stopTimer} onSwitchApp={switchApp} />} />
          </Routes>

          <div className="flex-1 flex flex-col min-w-0 bg-bg">
            {/* Window toolbar: page title and actions portal in here */}
            <header className="drag-region h-[58px] shrink-0 flex items-center gap-3 pl-7 pr-4 border-b border-line bg-bg z-10">
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
                onStart={startTimerWithCheer}
              />
            </header>

            <div className="relative flex-1 min-h-0">
              <main ref={mainRef} className="absolute inset-0 overflow-y-auto">
                <Routes location={billableLocation}>
                  <Route path="/" element={<Dashboard {...timerProps} />} />
                  <Route path="/clients" element={<Clients />} />
                  <Route path="/clients/:id" element={<ClientDetail />} />
                  <Route path="/projects" element={<Projects {...timerProps} />} />
                  <Route path="/projects/:id" element={<ProjectDetail {...timerProps} />} />
                  <Route path="/time" element={<TimeTracking {...timerProps} checkActive={checkActive} />} />
                  <Route path="/billing" element={<Billing />} />
                  <Route path="/invoices" element={<Navigate to="/billing" replace />} />
                  {/* Keyed so switching between new and edit never reuses a half-filled builder */}
                  <Route path="/invoices/new" element={<InvoiceCreate key={`new${billableLocation.search}`} />} />
                  <Route path="/invoices/:id/edit" element={<InvoiceCreate key={billableLocation.pathname} />} />
                  <Route path="/invoices/:id" element={<InvoiceDetail />} />
                  <Route path="/reports" element={<Reports isTimerRunning={isRunning} isTimerPaused={isPaused} />} />
                  <Route path="/commissions" element={<Commissions />} />
                  <Route path="/tax-overview" element={<TaxOverviewPage />} />
                  <Route path="/tax-settings" element={<TaxSettingsPage />} />
                  <Route path="/settings" element={<SettingsPage />} />
                  <Route path="/content" element={null} />
                  <Route path="*" element={<Navigate to="/" replace />} />
                </Routes>
              </main>
            </div>
          </div>
        </div>

        {/* Content HQ: loads the first time it is opened, then stays alive. */}
        {contentHqOpened && (
          <div
            ref={layers.content}
            data-suite-layer="content"
            className={`absolute inset-0 flex flex-col bg-bg z-[1] ${onContentHq || glide ? '' : 'invisible pointer-events-none'}`}
            aria-hidden={!onContentHq}
          >
            <SuiteBar
              onSwitch={switchApp}
              timer={
                <TimerControl
                  entry={activeEntry}
                  isRunning={isRunning}
                  isPaused={isPaused}
                  onPause={pauseTimer}
                  onResume={resumeTimer}
                  onStop={stopTimer}
                  onStart={startTimerWithCheer}
                />
              }
            />
            <div className="relative flex-1 min-h-0">
              <ContentHQView visible={onContentHq || Boolean(glide)} />
            </div>
          </div>
        )}

        {glide && <SuiteGlide key={glide.key} glide={glide} layers={layers} onDone={() => setGlide(null)} />}
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
            borderRadius: '16px',
            fontSize: '14px',
            fontWeight: 600,
            padding: '10px 14px',
            maxWidth: '420px',
          },
          success: { iconTheme: { primary: 'rgb(var(--green))', secondary: 'rgb(var(--panel))' } },
          error: { iconTheme: { primary: 'rgb(var(--red))', secondary: 'rgb(var(--panel))' } },
        }}
      />
      <WhatsNewModal />
      <MascotParty />
      <PhoneConnectModal />
    </HeaderSlotsProvider>
  )
}
