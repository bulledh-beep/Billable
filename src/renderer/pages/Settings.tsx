import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { Download, Trash2, RefreshCw, ExternalLink, X, Star } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import ThemeToggle from '../components/ThemeToggle'
import ConfirmDialog from '../components/ConfirmDialog'
import { Row, Section } from '../components/SettingsLayout'
import { useUpdater } from '../hooks/useUpdater'
import type { Settings, PaymentMethod } from '@shared/types'
import toast from 'react-hot-toast'

export default function SettingsPage() {
  const navigate = useNavigate()
  const [settings, setSettings] = useState<Settings | null>(null)
  const [dirty, setDirty] = useState(false)
  const [importOpen, setImportOpen] = useState(false)
  const updater = useUpdater()

  useEffect(() => { window.api.settings.get().then(setSettings) }, [])

  // Esc closes Settings (with an unsaved-changes guard)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !document.querySelector('[role="dialog"]')) {
        e.preventDefault()
        handleClose()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dirty])

  const handleClose = () => {
    if (dirty && !confirm('You have unsaved changes. Close without saving?')) return
    navigate('/')
  }

  const update = (patch: Partial<Settings>) => {
    setSettings(s => (s ? { ...s, ...patch } : s))
    setDirty(true)
  }

  const handleSave = async () => {
    if (!settings) return
    const saved = await window.api.settings.update(settings)
    setSettings(saved)
    toast.success('Settings saved')
    setDirty(false)
  }

  const handleExport = async () => {
    try {
      const result = await window.api.settings.exportDB()
      if (result) toast.success('Backup saved')
    } catch (err: any) {
      toast.error(`Couldn't export: ${err.message || err}`)
    }
  }

  const handleImport = async () => {
    try {
      await window.api.settings.importDB()
    } catch (err: any) {
      toast.error(err.message?.replace(/^Error invoking remote method '[^']+': (Error: )?/, '') || String(err))
    }
  }

  if (!settings) return null

  const methods: PaymentMethod[] = (() => {
    try { return JSON.parse(settings.payment_methods || '[]') } catch { return [] }
  })()
  const setMethods = (next: PaymentMethod[], extra: Partial<Settings> = {}) =>
    update({ payment_methods: JSON.stringify(next), ...extra })

  return (
    <div className="page">
      <div className="max-w-[860px]">
      <PageHeader
        title="Settings"
        actions={
          <>
            <button onClick={handleSave} disabled={!dirty} className="btn-primary">Save changes</button>
            <button onClick={handleClose} className="btn-icon" title="Close settings (Esc)" aria-label="Close settings"><X /></button>
          </>
        }
      />

      <Section title="Business" description="Shown at the top of every invoice.">
        <Row label="Business name">
          <input className="input" value={settings.business_name} onChange={e => update({ business_name: e.target.value })} placeholder="Your name or company" />
        </Row>
        <Row label="Email">
          <input className="input" type="email" value={settings.business_email} onChange={e => update({ business_email: e.target.value })} placeholder="billing@example.com" />
        </Row>
        <Row label="Address">
          <textarea className="input" rows={2} value={settings.business_address} onChange={e => update({ business_address: e.target.value })} placeholder="Street, city, province, postal code" />
        </Row>
        <Row label="Business number" help="Optional. Printed on invoices when filled in. Your GST/HST number lives in Tax settings.">
          <input className="input max-w-[280px]" value={settings.tax_id === '0' ? '' : settings.tax_id} onChange={e => update({ tax_id: e.target.value })} placeholder="123456789" />
        </Row>
      </Section>

      <Section title="Invoices">
        <Row label="Numbering" help="The next invoice gets this number, then it counts up.">
          <div className="flex items-center gap-2">
            <input className="input w-24" value={settings.invoice_prefix} onChange={e => update({ invoice_prefix: e.target.value })} placeholder="INV-" />
            <input className="input w-28 num" type="number" value={settings.invoice_next_number || ''} onChange={e => update({ invoice_next_number: parseInt(e.target.value) || 1 })} />
            <span className="text-xs text-fg-3">Next: <span className="num text-fg-2">{settings.invoice_prefix}{settings.invoice_next_number}</span></span>
          </div>
        </Row>
        <Row label="Payment methods" help="The starred one is filled in on new invoices.">
          <div className="space-y-2">
            {methods.map((m, i) => {
              const isDefault = settings.default_payment_method === m.name
              return (
                <div key={i} className="flex items-center gap-2">
                  <button
                    onClick={() => update({ default_payment_method: m.name })}
                    className={`btn-icon-sm ${isDefault ? '!text-accent-text' : ''}`}
                    title={isDefault ? 'Default method' : 'Make default'}
                    aria-label="Make default"
                  >
                    <Star className={isDefault ? 'fill-current' : ''} />
                  </button>
                  <input
                    className="input"
                    value={m.name}
                    placeholder="e-Transfer"
                    onChange={e => {
                      const next = methods.map((x, j) => (j === i ? { ...x, name: e.target.value } : x))
                      setMethods(next, isDefault ? { default_payment_method: e.target.value } : {})
                    }}
                  />
                  <input
                    className="input"
                    value={m.email}
                    placeholder="Where to send it (optional)"
                    onChange={e => setMethods(methods.map((x, j) => (j === i ? { ...x, email: e.target.value } : x)))}
                  />
                  <button
                    onClick={() => {
                      const next = methods.filter((_, j) => j !== i)
                      setMethods(next, isDefault ? { default_payment_method: next[0]?.name || '' } : {})
                    }}
                    className="btn-icon-sm hover:text-red"
                    aria-label="Remove method"
                  >
                    <Trash2 />
                  </button>
                </div>
              )
            })}
            <button onClick={() => setMethods([...methods, { name: '', email: '' }])} className="btn-ghost btn-sm -ml-1">
              Add method
            </button>
          </div>
        </Row>
      </Section>

      <Section title="Time and rates">
        <Row label="Default hourly rate" help="Used for new clients.">
          <div className="relative w-40">
            <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">$</span>
            <input className="input pl-6 num" type="number" value={settings.default_rate || ''} onChange={e => update({ default_rate: parseFloat(e.target.value) || 0 })} />
          </div>
        </Row>
        <Row label="Currency">
          <select className="input w-40" value={settings.default_currency} onChange={e => update({ default_currency: e.target.value })}>
            <option value="CAD">CAD</option>
            <option value="USD">USD</option>
            <option value="EUR">EUR</option>
            <option value="GBP">GBP</option>
            <option value="AUD">AUD</option>
            <option value="JPY">JPY</option>
          </select>
        </Row>
        <Row label="Timer rounding" help="Applied when a timer stops. Timers under a minute are always discarded.">
          <select className="input w-60" value={settings.time_rounding} onChange={e => update({ time_rounding: e.target.value as Settings['time_rounding'] })}>
            <option value="none">Don't round</option>
            <option value="6">Round up to 6 minutes</option>
            <option value="15">Round up to 15 minutes</option>
            <option value="30">Round up to 30 minutes</option>
          </select>
        </Row>
      </Section>

      <Section title="Appearance">
        <Row label="Theme" help="Auto follows your Mac. Shared by all profiles.">
          <ThemeToggle />
        </Row>
      </Section>

      <Section title="Updates">
        <Row
          label="Version"
          help={updater.status?.last_checked_at ? `Checked ${new Date(updater.status.last_checked_at).toLocaleString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })}` : 'Checks GitHub when the app opens.'}
        >
          <div className="flex items-center gap-3">
            <div className="text-sm text-fg">
              <span className="num">{updater.status?.current_version || '—'}</span>
              {updater.status?.update_available
                ? <span className="text-accent-text"> · {updater.status.latest_version} available</span>
                : updater.status?.latest_version ? <span className="text-fg-3"> · up to date</span> : null}
            </div>
            <div className="ml-auto flex items-center gap-2">
              <button onClick={() => updater.checkNow().catch(() => {})} disabled={updater.checking} className="btn-secondary btn-sm">
                <RefreshCw className={updater.checking ? 'animate-spin' : ''} /> {updater.checking ? 'Checking…' : 'Check now'}
              </button>
              {updater.status?.update_available && (
                <button
                  onClick={updater.install}
                  disabled={updater.downloadState === 'downloading' || updater.downloadState === 'installing'}
                  className="btn-primary btn-sm"
                >
                  <Download />
                  {updater.downloadState === 'downloading'
                    ? `Downloading ${updater.progress?.percent ?? 0}%`
                    : updater.downloadState === 'installing'
                      ? 'Installing…'
                      : updater.canInstall ? 'Install and relaunch' : 'Download update'}
                </button>
              )}
            </div>
          </div>
          {updater.status?.update_available && updater.status.release_notes && (
            <details className="mt-3 group">
              <summary className="cursor-pointer text-xs text-fg-2 hover:text-fg flex items-center gap-1.5">
                <span className="group-open:hidden">What's new</span>
                <span className="hidden group-open:inline">Hide notes</span>
                {updater.status.release_url && (
                  <a href={updater.status.release_url} target="_blank" rel="noreferrer" className="ml-2 text-fg-3 hover:text-accent-text inline-flex items-center gap-1" onClick={e => e.stopPropagation()}>
                    GitHub <ExternalLink className="w-3 h-3" />
                  </a>
                )}
              </summary>
              <pre className="mt-2 p-3 rounded-md bg-panel-2 text-xs text-fg-2 whitespace-pre-wrap font-sans leading-5 max-h-60 overflow-y-auto">
                {updater.status.release_notes}
              </pre>
            </details>
          )}
        </Row>
      </Section>

      <Section title="Data" description="Backups cover the profile you're in now.">
        <Row label="Back up" help="Saves a complete copy of this profile's clients, time, invoices, and expenses.">
          <button onClick={handleExport} className="btn-secondary">Export backup</button>
        </Row>
        <Row label="Restore" help="Replaces this profile's data with a backup. A copy of the current data is kept next to it first.">
          <button onClick={() => setImportOpen(true)} className="btn-secondary">Restore from backup…</button>
        </Row>
      </Section>

      <ConfirmDialog
        isOpen={importOpen}
        onClose={() => setImportOpen(false)}
        onConfirm={handleImport}
        title="Restore from a backup?"
        message="This replaces everything in the current profile with the backup you choose, then restarts Billable. A copy of the current data is saved first, so nothing is lost."
        confirmText="Choose backup…"
        danger={false}
      />
      </div>
    </div>
  )
}
