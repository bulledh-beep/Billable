import { useEffect, useState, type ReactNode } from 'react'
import { ArrowLeft, ArrowRight, RotateCw, ExternalLink } from 'lucide-react'
import SuiteSwitch, { type SuiteApp } from './SuiteSwitch'
import { contentHqView, CONTENT_HQ_NAV } from '../utils/contentHq'

/**
 * The slim bar across the top while Content HQ fills the window: the app
 * switch, simple browser controls, and Billable's timer.
 */
export default function SuiteBar({ onSwitch, timer }: { onSwitch: (app: SuiteApp) => void; timer: ReactNode }) {
  const [nav, setNav] = useState({ back: false, forward: false })

  useEffect(() => {
    const read = () => {
      const view = contentHqView()
      if (!view) return
      try { setNav({ back: view.canGoBack(), forward: view.canGoForward() }) } catch { /* not attached yet */ }
    }
    read()
    window.addEventListener(CONTENT_HQ_NAV, read)
    const id = window.setInterval(read, 1500)
    return () => { window.removeEventListener(CONTENT_HQ_NAV, read); window.clearInterval(id) }
  }, [])

  const view = () => contentHqView()

  return (
    <div className="drag-region h-[48px] shrink-0 flex items-center gap-1 pl-[78px] pr-3 border-b border-line bg-sidebar">
      <SuiteSwitch value="content" onChange={onSwitch} />
      <div className="no-drag flex items-center gap-0.5 ml-3">
        <button onClick={() => view()?.goBack()} disabled={!nav.back} className="btn-icon-sm disabled:opacity-30" title="Back" aria-label="Back"><ArrowLeft /></button>
        <button onClick={() => view()?.goForward()} disabled={!nav.forward} className="btn-icon-sm disabled:opacity-30" title="Forward" aria-label="Forward"><ArrowRight /></button>
        <button onClick={() => view()?.reload()} className="btn-icon-sm" title="Reload" aria-label="Reload"><RotateCw /></button>
        <button
          onClick={() => { const url = view()?.getURL(); if (url) window.api.contentHq.openInBrowser(url) }}
          className="btn-icon-sm"
          title="Open in browser"
          aria-label="Open in browser"
        >
          <ExternalLink />
        </button>
      </div>
      <div className="flex-1" />
      <div className="no-drag">{timer}</div>
    </div>
  )
}
