import { useEffect, useState } from 'react'
import { Sparkles, ExternalLink } from 'lucide-react'
import Modal from './Modal'

const STORAGE_KEY = 'billable.lastSeenVersion'

interface ReleaseInfo {
  name: string
  body: string
  html_url: string
}

/**
 * Shows release notes on first launch after an update.
 *
 * Strategy:
 *   1. On mount, read the current app version
 *   2. Compare to localStorage 'lastSeenVersion'
 *   3. If newer (and not the very first install), fetch release notes for
 *      that version and show this modal
 *   4. Mark current version as "seen" once dismissed
 *
 * If the very first launch detects no stored version, we record current and
 * skip the modal — no point showing release notes for a fresh install.
 */
export default function WhatsNewModal() {
  const [open, setOpen] = useState(false)
  const [info, setInfo] = useState<ReleaseInfo | null>(null)
  const [version, setVersion] = useState<string>('')

  useEffect(() => {
    let cancelled = false
    const run = async () => {
      try {
        const current: string = await window.api.updater.currentVersion()
        if (!current) return

        const lastSeen = (() => {
          try { return localStorage.getItem(STORAGE_KEY) } catch { return null }
        })()

        if (!lastSeen) {
          // Fresh install — silently mark current as seen, no modal
          try { localStorage.setItem(STORAGE_KEY, current) } catch { /* ignore */ }
          return
        }

        if (lastSeen === current) return // already seen this version

        // Different version detected. Try to fetch release notes; only show
        // the modal if we successfully got notes (no point in an empty modal).
        const notes: ReleaseInfo | null = await window.api.updater.releaseNotes(current)
        if (cancelled) return
        if (notes && notes.body?.trim()) {
          setVersion(current)
          setInfo(notes)
          setOpen(true)
        } else {
          // Couldn't fetch — silently mark seen so we don't keep trying
          try { localStorage.setItem(STORAGE_KEY, current) } catch { /* ignore */ }
        }
      } catch {
        // Network error or similar — fail silent, don't bug the user
      }
    }
    run()
    return () => { cancelled = true }
  }, [])

  const close = () => {
    if (version) {
      try { localStorage.setItem(STORAGE_KEY, version) } catch { /* ignore */ }
    }
    setOpen(false)
  }

  if (!info) return null

  return (
    <Modal isOpen={open} onClose={close} title={`What's new in ${info.name}`}>
      <div className="space-y-4">
        <div className="flex items-start gap-3 p-4 rounded-lg bg-accent/[0.05] border border-accent/20">
          <Sparkles className="w-4 h-4 text-accent flex-shrink-0 mt-0.5" />
          <div>
            <div className="text-sm font-medium text-text-primary">
              You just updated to v{version}
            </div>
            <div className="text-xs text-text-tertiary mt-0.5">
              Here's everything that's changed since your last version.
            </div>
          </div>
        </div>

        <pre className="max-h-80 overflow-y-auto text-xs text-text-secondary whitespace-pre-wrap font-mono leading-relaxed p-3 rounded-lg bg-surface-200">
          {info.body}
        </pre>

        <div className="flex items-center justify-between gap-3 pt-1">
          <a
            href={info.html_url}
            target="_blank"
            rel="noreferrer"
            className="text-xs text-text-tertiary hover:text-accent inline-flex items-center gap-1.5 transition-colors"
          >
            View on GitHub <ExternalLink className="w-3 h-3" />
          </a>
          <button onClick={close} className="btn-primary text-sm">
            Got it
          </button>
        </div>
      </div>
    </Modal>
  )
}
