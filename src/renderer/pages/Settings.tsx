import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { Save, Download, Upload, Plus, Trash2, RefreshCw, ExternalLink } from 'lucide-react'
import type { Settings, PaymentMethod } from '@shared/types'
import ThemeToggle from '../components/ThemeToggle'
import { useUpdater } from '../hooks/useUpdater'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.06 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

export default function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [dirty, setDirty] = useState(false)
  const updater = useUpdater()

  useEffect(() => { loadSettings() }, [])

  const loadSettings = async () => {
    const s = await window.api.settings.get()
    setSettings(s)
  }

  const updateField = (key: string, value: any) => {
    if (!settings) return
    setSettings({ ...settings, [key]: value } as any)
    setDirty(true)
  }

  const handleSave = async () => {
    if (!settings) return
    await window.api.settings.update(settings)
    toast.success('Settings saved')
    setDirty(false)
  }

  const handleExport = async () => {
    const result = await window.api.settings.exportDB()
    if (result) toast.success('Database exported')
  }

  const handleImport = async () => {
    await window.api.settings.importDB()
  }

  if (!settings) return null

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8 max-w-2xl">
      <motion.div variants={item} className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Settings</h1>
          <p className="text-sm text-text-secondary mt-1">Configure your business profile and preferences</p>
        </div>
        <button
          onClick={handleSave}
          disabled={!dirty}
          className={`btn-primary flex items-center gap-2 ${!dirty ? 'opacity-50 cursor-not-allowed' : ''}`}
        >
          <Save className="w-4 h-4" /> Save Changes
        </button>
      </motion.div>

      {/* Business Profile */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-4">Business Profile</h2>
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Business Name</label>
            <input
              className="input-field"
              value={settings.business_name}
              onChange={e => updateField('business_name', e.target.value)}
              placeholder="Your Business Name"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Email</label>
            <input
              className="input-field"
              type="email"
              value={settings.business_email}
              onChange={e => updateField('business_email', e.target.value)}
              placeholder="billing@example.com"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Address</label>
            <textarea
              className="input-field"
              rows={2}
              value={settings.business_address}
              onChange={e => updateField('business_address', e.target.value)}
              placeholder="123 Main St, City, State 12345"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Tax ID</label>
            <input
              className="input-field"
              value={settings.tax_id}
              onChange={e => updateField('tax_id', e.target.value)}
              placeholder="XX-XXXXXXX"
            />
          </div>
        </div>
      </motion.div>

      {/* Invoice Settings */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-4">Invoice Settings</h2>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Invoice Number Prefix</label>
            <input
              className="input-field"
              value={settings.invoice_prefix}
              onChange={e => updateField('invoice_prefix', e.target.value)}
              placeholder="INV-"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Next Invoice Number</label>
            <input
              className="input-field"
              type="number"
              value={settings.invoice_next_number || ''}
              onChange={e => updateField('invoice_next_number', parseInt(e.target.value) || 1)}
            />
          </div>
        </div>
      </motion.div>

      {/* Payment Methods */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold text-text-primary">Payment Methods</h2>
          <button
            onClick={() => {
              const methods: PaymentMethod[] = JSON.parse(settings.payment_methods || '[]')
              methods.push({ name: '', email: '' })
              updateField('payment_methods', JSON.stringify(methods))
            }}
            className="text-xs text-accent hover:text-accent-light flex items-center gap-1"
          >
            <Plus className="w-3 h-3" /> Add Method
          </button>
        </div>
        <div className="space-y-3">
          {(JSON.parse(settings.payment_methods || '[]') as PaymentMethod[]).map((method, i) => (
            <div key={i} className="flex gap-3 items-start">
              <div className="flex-1 grid grid-cols-2 gap-3">
                <input
                  className="input-field"
                  value={method.name}
                  onChange={e => {
                    const methods: PaymentMethod[] = JSON.parse(settings.payment_methods || '[]')
                    const oldName = methods[i].name
                    methods[i].name = e.target.value
                    const updated: any = { payment_methods: JSON.stringify(methods) }
                    if (settings.default_payment_method === oldName) {
                      updated.default_payment_method = e.target.value
                    }
                    setSettings({ ...settings, ...updated } as any)
                    setDirty(true)
                  }}
                  placeholder="e.g., e-Transfer, PayPal"
                />
                <input
                  className="input-field"
                  value={method.email}
                  onChange={e => {
                    const methods: PaymentMethod[] = JSON.parse(settings.payment_methods || '[]')
                    methods[i].email = e.target.value
                    updateField('payment_methods', JSON.stringify(methods))
                  }}
                  placeholder="Email for this method"
                />
              </div>
              <button
                onClick={() => {
                  const methods: PaymentMethod[] = JSON.parse(settings.payment_methods || '[]')
                  const removed = methods.splice(i, 1)[0]
                  const updated: any = { payment_methods: JSON.stringify(methods) }
                  if (settings.default_payment_method === removed.name && methods.length > 0) {
                    updated.default_payment_method = methods[0].name
                  }
                  setSettings({ ...settings, ...updated } as any)
                  setDirty(true)
                }}
                className="p-2 hover:bg-red-500/10 rounded transition-colors mt-0.5"
              >
                <Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" />
              </button>
            </div>
          ))}
          {(JSON.parse(settings.payment_methods || '[]') as PaymentMethod[]).length === 0 && (
            <p className="text-sm text-text-tertiary text-center py-2">No payment methods configured</p>
          )}
        </div>
        {(JSON.parse(settings.payment_methods || '[]') as PaymentMethod[]).length > 0 && (
          <div className="mt-4">
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Default Payment Method</label>
            <select
              className="input-field"
              value={settings.default_payment_method}
              onChange={e => updateField('default_payment_method', e.target.value)}
            >
              {(JSON.parse(settings.payment_methods || '[]') as PaymentMethod[]).map((m, i) => (
                <option key={i} value={m.name}>{m.name}{m.email ? ` — ${m.email}` : ''}</option>
              ))}
            </select>
            <p className="text-xs text-text-tertiary mt-1">
              Auto-selected when creating new invoices
            </p>
          </div>
        )}
      </motion.div>

      {/* Defaults */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-4">Defaults</h2>
        <div className="grid grid-cols-2 gap-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Default Hourly Rate</label>
            <input
              className="input-field"
              type="number"
              value={settings.default_rate || ''}
              onChange={e => updateField('default_rate', parseFloat(e.target.value) || 0)}
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Default Currency</label>
            <select
              className="input-field"
              value={settings.default_currency}
              onChange={e => updateField('default_currency', e.target.value)}
            >
              <option value="USD">USD ($)</option>
              <option value="EUR">EUR (&euro;)</option>
              <option value="GBP">GBP (&pound;)</option>
              <option value="CAD">CAD ($)</option>
              <option value="AUD">AUD ($)</option>
              <option value="JPY">JPY (&yen;)</option>
            </select>
          </div>
        </div>
      </motion.div>

      {/* Time Tracking */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-4">Time Tracking</h2>
        <div>
          <label className="text-xs font-medium text-text-secondary mb-1.5 block">Time Rounding</label>
          <select
            className="input-field w-60"
            value={settings.time_rounding}
            onChange={e => updateField('time_rounding', e.target.value)}
          >
            <option value="none">No rounding</option>
            <option value="6">Round to nearest 6 minutes</option>
            <option value="15">Round to nearest 15 minutes</option>
            <option value="30">Round to nearest 30 minutes</option>
          </select>
        </div>
      </motion.div>

      {/* Appearance */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-1">Appearance</h2>
        <p className="text-xs text-text-tertiary mb-4">
          Theme preference is shared across all profiles.
        </p>
        <div className="flex items-center gap-4">
          <ThemeToggle />
          <span className="text-xs text-text-tertiary">
            Auto follows your macOS appearance setting.
          </span>
        </div>
      </motion.div>

      {/* Updates */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-1">Updates</h2>
        <p className="text-xs text-text-tertiary mb-4">
          Billable checks GitHub for new releases on launch.
        </p>
        <div className="flex items-center justify-between gap-4">
          <div className="flex-1 min-w-0">
            <div className="text-xs text-text-secondary">
              Current: <span className="font-mono text-text-primary">v{updater.status?.current_version || '—'}</span>
              {updater.status?.latest_version && (
                <>
                  {' · '}Latest: <span className="font-mono text-text-primary">v{updater.status.latest_version}</span>
                </>
              )}
            </div>
            {updater.status?.last_checked_at && (
              <div className="text-[10px] text-text-tertiary mt-0.5">
                Last checked {new Date(updater.status.last_checked_at).toLocaleString()}
              </div>
            )}
            {updater.status?.update_available && (
              <div className="text-[11px] text-accent mt-1.5 font-medium">
                Update available — v{updater.status.latest_version} is ready to download
              </div>
            )}
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => updater.checkNow().catch(() => {})}
              disabled={updater.checking}
              className="btn-secondary flex items-center gap-2 text-xs"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${updater.checking ? 'animate-spin' : ''}`} />
              {updater.checking ? 'Checking…' : 'Check Now'}
            </button>
            {updater.status?.update_available && (
              <button
                onClick={updater.download}
                disabled={updater.downloadState === 'downloading'}
                className="btn-primary flex items-center gap-2 text-xs"
              >
                <Download className="w-3.5 h-3.5" />
                {updater.downloadState === 'downloading'
                  ? `Downloading ${updater.progress?.percent ?? 0}%`
                  : updater.downloadState === 'done'
                    ? 'Downloaded'
                    : 'Download Update'}
              </button>
            )}
          </div>
        </div>
        {updater.status?.update_available && updater.status.release_notes && (
          <details className="mt-4 group">
            <summary className="cursor-pointer text-xs text-text-secondary hover:text-text-primary transition-colors flex items-center gap-1.5">
              <span className="group-open:hidden">Show release notes</span>
              <span className="hidden group-open:inline">Hide release notes</span>
              {updater.status.release_url && (
                <a
                  href={updater.status.release_url}
                  target="_blank"
                  rel="noreferrer"
                  className="ml-2 text-text-tertiary hover:text-accent inline-flex items-center gap-1"
                  onClick={e => e.stopPropagation()}
                >
                  on GitHub <ExternalLink className="w-3 h-3" />
                </a>
              )}
            </summary>
            <pre className="mt-2 p-3 rounded-lg bg-surface-200 text-[11px] text-text-secondary whitespace-pre-wrap font-mono leading-relaxed max-h-60 overflow-y-auto">
              {updater.status.release_notes}
            </pre>
          </details>
        )}
      </motion.div>

      {/* Data */}
      <motion.div variants={item} className="glass-panel p-6">
        <h2 className="text-sm font-semibold text-text-primary mb-4">Data</h2>
        <div className="flex gap-3">
          <button onClick={handleExport} className="btn-secondary flex items-center gap-2">
            <Download className="w-4 h-4" /> Export Database
          </button>
          <button onClick={handleImport} className="btn-secondary flex items-center gap-2">
            <Upload className="w-4 h-4" /> Import Database
          </button>
        </div>
        <p className="text-xs text-text-tertiary mt-3">
          Export creates a backup of your entire database. Import will replace all current data.
        </p>
      </motion.div>
    </motion.div>
  )
}
