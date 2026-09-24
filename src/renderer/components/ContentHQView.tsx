import { useEffect, useRef, useState } from 'react'
import type { WebviewTag } from 'electron'
import { useNavigate } from 'react-router-dom'
import { Mascot } from './Illustrations'
import { contentHqUrl, registerContentHqView, CONTENT_HQ_NAV } from '../utils/contentHq'

/**
 * Content HQ, the team content workspace, running inside Billable's window.
 * It stays alive while you move around Billable, so you come back to where
 * you left it. It only loads once you first open it.
 */
export default function ContentHQView({ visible }: { visible: boolean }) {
  const navigate = useNavigate()
  const ref = useRef<WebviewTag | null>(null)
  const [src, setSrc] = useState<string | null>(null)
  const [state, setState] = useState<'loading' | 'ready' | 'error'>('loading')
  const [error, setError] = useState('')

  useEffect(() => {
    window.api.settings.get().then((s: any) => setSrc(contentHqUrl(s)))
  }, [])

  useEffect(() => {
    const view = ref.current
    if (!view || !src) return
    registerContentHqView(view)
    const announce = () => window.dispatchEvent(new Event(CONTENT_HQ_NAV))
    const onStop = () => { setState(s => (s === 'error' ? s : 'ready')); announce() }
    const onFail = (e: any) => {
      // -3 is an aborted load, which happens on normal redirects
      if (!e.isMainFrame || e.errorCode === -3) return
      setError(e.errorDescription || 'The page could not be reached.')
      setState('error')
    }
    view.addEventListener('did-stop-loading', onStop)
    view.addEventListener('did-fail-load', onFail)
    view.addEventListener('did-navigate', announce)
    view.addEventListener('did-navigate-in-page', announce)
    return () => {
      view.removeEventListener('did-stop-loading', onStop)
      view.removeEventListener('did-fail-load', onFail)
      view.removeEventListener('did-navigate', announce)
      view.removeEventListener('did-navigate-in-page', announce)
      registerContentHqView(null)
    }
  }, [src])

  const retry = () => {
    setState('loading')
    ref.current?.reload()
  }

  return (
    <div className={`absolute inset-0 bg-bg ${visible ? '' : 'invisible pointer-events-none'}`} aria-hidden={!visible}>
      {src && (
        <webview
          ref={ref as any}
          src={src}
          partition="persist:contenthq"
          // A plain Chrome browser ID; Google refuses sign-in in apps it spots as embedded
          useragent={navigator.userAgent.replace(/\s(Electron|Billable|billable)\/\S+/g, '')}
          style={{ width: '100%', height: '100%', display: 'flex' }}
        />
      )}
      {state !== 'ready' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-bg text-center px-6">
          {state === 'loading' ? (
            <>
              <Mascot size={92} mood="wave" motion="bob" />
              <p className="mt-3 text-[16px] font-semibold text-fg">Opening Content HQ…</p>
            </>
          ) : (
            <>
              <Mascot size={92} mood="worried" />
              <p className="mt-3 text-[17px] font-bold text-fg">Couldn't reach Content HQ</p>
              <p className="mt-1 text-[13px] text-fg-3 max-w-[360px]">{error} Check your internet connection or the address in Settings.</p>
              <div className="flex gap-2 mt-4">
                <button onClick={retry} className="btn-primary">Try again</button>
                <button onClick={() => navigate('/settings')} className="btn-secondary">Open Settings</button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  )
}
