import { useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { Download, X, RefreshCw, CheckCircle2, ChevronDown, ChevronUp, Sparkles } from 'lucide-react'
import { useUpdater } from '../hooks/useUpdater'

function formatSize(bytes: number | null): string {
  if (!bytes) return ''
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

export default function UpdateBanner() {
  const { status, showBanner, downloadState, progress, install, dismiss, canInstall } = useUpdater()
  const [showNotes, setShowNotes] = useState(false)

  if (!showBanner || !status) return null

  const isInstalling = downloadState === 'installing'
  const isDownloading = downloadState === 'downloading'

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0, y: -10 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -10 }}
        className="mx-3 mb-2 px-3 py-2.5 rounded-lg bg-accent/[0.08] border border-accent/30"
      >
        <div className="flex items-start gap-2">
          {isInstalling ? (
            <CheckCircle2 className="w-3.5 h-3.5 text-status-paid flex-shrink-0 mt-0.5" />
          ) : (
            <Sparkles className="w-3.5 h-3.5 text-accent flex-shrink-0 mt-0.5" />
          )}
          <div className="flex-1 min-w-0">
            <div className="text-xs font-medium text-text-primary truncate">
              {isInstalling
                ? 'Installing — Billable will relaunch'
                : `Update available — v${status.latest_version}`}
            </div>
            {downloadState === 'idle' && (
              <div className="text-[10px] text-text-tertiary mt-0.5">
                You're on v{status.current_version} · {formatSize(status.download_size_bytes)}
              </div>
            )}
            {isDownloading && (
              <div className="mt-1.5">
                <div className="h-1 rounded-full bg-surface-300 overflow-hidden">
                  <div
                    className="h-full bg-accent transition-all duration-150"
                    style={{ width: progress ? `${progress.percent}%` : '5%' }}
                  />
                </div>
                <div className="text-[10px] text-text-tertiary mt-1 font-mono">
                  {progress ? `${progress.percent}% · downloading` : 'Starting…'}
                </div>
              </div>
            )}
            {isInstalling && (
              <div className="text-[10px] text-text-tertiary mt-0.5">
                The new version will open in a moment.
              </div>
            )}
          </div>
          {!isInstalling && !isDownloading && (
            <button
              onClick={dismiss}
              className="p-0.5 hover:bg-surface-300 rounded transition-colors flex-shrink-0"
              title="Dismiss this update"
            >
              <X className="w-3 h-3 text-text-tertiary" />
            </button>
          )}
        </div>

        {/* Release notes preview */}
        {downloadState === 'idle' && status.release_notes && (
          <div className="mt-2 pt-2 border-t border-accent/15">
            <button
              onClick={() => setShowNotes(v => !v)}
              className="flex items-center gap-1 text-[10px] text-text-secondary hover:text-text-primary transition-colors"
            >
              {showNotes ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
              {showNotes ? 'Hide' : 'What\'s new'}
            </button>
            {showNotes && (
              <pre className="mt-1.5 max-h-40 overflow-y-auto text-[10px] text-text-secondary whitespace-pre-wrap font-mono leading-relaxed">
                {status.release_notes}
              </pre>
            )}
          </div>
        )}

        {downloadState === 'idle' && (
          <button
            onClick={install}
            className="w-full mt-2 px-2.5 py-1.5 rounded-md bg-accent hover:bg-accent-light text-[11px] font-medium transition-colors flex items-center justify-center gap-1.5"
            style={{ color: '#1a1a1a' }}
          >
            <Download className="w-3 h-3" />
            {canInstall ? 'Install Update' : 'Download Update'}
          </button>
        )}
        {isDownloading && (
          <button
            disabled
            className="w-full mt-2 px-2.5 py-1.5 rounded-md bg-surface-200 text-text-tertiary text-[11px] font-medium flex items-center justify-center gap-1.5 cursor-not-allowed"
          >
            <RefreshCw className="w-3 h-3 animate-spin" />
            Downloading {progress?.percent ?? 0}%…
          </button>
        )}
        {isInstalling && (
          <div className="w-full mt-2 px-2.5 py-1.5 rounded-md bg-status-paid/10 text-status-paid text-[11px] font-medium flex items-center justify-center gap-1.5">
            <CheckCircle2 className="w-3 h-3" /> Quitting & relaunching…
          </div>
        )}
        {downloadState === 'error' && (
          <button
            onClick={install}
            className="w-full mt-2 px-2.5 py-1.5 rounded-md bg-red-500/10 hover:bg-red-500/20 text-red-400 text-[11px] font-medium transition-colors flex items-center justify-center gap-1.5"
          >
            Retry
          </button>
        )}
      </motion.div>
    </AnimatePresence>
  )
}
