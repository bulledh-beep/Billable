import { useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { Download, X, RefreshCw, CheckCircle2, ChevronDown, ChevronUp, ArrowDownCircle } from 'lucide-react'
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
        className="mx-2.5 mb-2 px-3 py-2.5 rounded-[8px] bg-panel border border-line shadow-card"
      >
        <div className="flex items-start gap-2">
          {isInstalling ? (
            <CheckCircle2 className="w-3.5 h-3.5 text-green flex-shrink-0 mt-0.5" />
          ) : (
            <ArrowDownCircle className="w-3.5 h-3.5 text-accent-text flex-shrink-0 mt-0.5" />
          )}
          <div className="flex-1 min-w-0">
            <div className="text-xs font-semibold text-fg truncate">
              {isInstalling
                ? 'Installing, Billable will relaunch'
                : `Version ${status.latest_version} is available`}
            </div>
            {downloadState === 'idle' && (
              <div className="text-2xs text-fg-3 mt-0.5">
                You're on {status.current_version} · {formatSize(status.download_size_bytes)}
              </div>
            )}
            {isDownloading && (
              <div className="mt-1.5">
                <div className="h-1 rounded-full bg-fg/10 overflow-hidden">
                  <div
                    className="h-full bg-accent transition-all duration-150"
                    style={{ width: progress ? `${progress.percent}%` : '5%' }}
                  />
                </div>
                <div className="text-2xs text-fg-3 mt-1 num">
                  {progress ? `${progress.percent}% · downloading` : 'Starting…'}
                </div>
              </div>
            )}
            {isInstalling && (
              <div className="text-2xs text-fg-3 mt-0.5">
                The new version will open in a moment.
              </div>
            )}
          </div>
          {!isInstalling && !isDownloading && (
            <button
              onClick={dismiss}
              className="p-0.5 hover:bg-fg/10 rounded transition-colors flex-shrink-0"
              title="Dismiss this update"
            >
              <X className="w-3 h-3 text-fg-3" />
            </button>
          )}
        </div>

        {/* Release notes preview */}
        {downloadState === 'idle' && status.release_notes && (
          <div className="mt-2 pt-2 border-t border-line">
            <button
              onClick={() => setShowNotes(v => !v)}
              className="flex items-center gap-1 text-2xs text-fg-2 hover:text-fg transition-colors"
            >
              {showNotes ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
              {showNotes ? 'Hide' : 'What\'s new'}
            </button>
            {showNotes && (
              <pre className="mt-1.5 max-h-40 overflow-y-auto text-2xs text-fg-2 whitespace-pre-wrap font-sans leading-relaxed">
                {status.release_notes}
              </pre>
            )}
          </div>
        )}

        {downloadState === 'idle' && (
          <button
            onClick={install}
            className="btn-primary btn-sm w-full mt-2"
          >
            <Download className="w-3 h-3" />
            {canInstall ? 'Install and relaunch' : 'Download update'}
          </button>
        )}
        {isDownloading && (
          <button
            disabled
            className="btn-secondary btn-sm w-full mt-2 cursor-not-allowed"
          >
            <RefreshCw className="w-3 h-3 animate-spin" />
            Downloading {progress?.percent ?? 0}%…
          </button>
        )}
        {isInstalling && (
          <div className="w-full mt-2 h-7 rounded-md bg-green/10 text-green text-xs font-medium flex items-center justify-center gap-1.5">
            <CheckCircle2 className="w-3 h-3" /> Quitting and relaunching…
          </div>
        )}
        {downloadState === 'error' && (
          <button
            onClick={install}
            className="btn-danger btn-sm w-full mt-2"
          >
            Retry
          </button>
        )}
      </motion.div>
    </AnimatePresence>
  )
}
