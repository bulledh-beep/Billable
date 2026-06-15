import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import { Save, Download, Upload, Plus, Trash2, RefreshCw, ExternalLink, X } from 'lucide-react'
import type { Settings, PaymentMethod } from '@shared/types'
import ThemeToggle from '../components/ThemeToggle'
import { useUpdater } from '../hooks/useUpdater'
import toast from 'react-hot-toast'
import Modal from '../components/Modal'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.06 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

export default function SettingsPage() {
  const navigate = useNavigate()
  const [settings, setSettings] = useState<Settings | null>(null)
  const [dirty, setDirty] = useState(false)
  const [gmailStatus, setGmailStatus] = useState<{
    connected: boolean
    email: string
    clientId: string
    clientSecret: string
  } | null>(null)
  const [rules, setRules] = useState<any[]>([])
  const [showRuleModal, setShowRuleModal] = useState(false)
  const [ruleForm, setRuleForm] = useState({
    rule_name: '',
    sender_contains: '',
    subject_contains: '',
    vendor: '',
    category: 'other',
    record_type: 'bill',
    recurring_frequency: 'monthly',
    auto_approve: 0,
  })
  const updater = useUpdater()

  useEffect(() => {
    loadSettings()
    loadGmailStatus()
    loadRules()
  }, [])

  // Esc closes Settings (with an unsaved-changes guard)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
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

  const loadSettings = async () => {
    const s = await window.api.settings.get()
    setSettings(s)
  }

  const loadGmailStatus = async () => {
    const status = await window.api.gmail.status()
    setGmailStatus(status)
  }

  const handleConnectGmail = async () => {
    try {
      if (!settings?.google_client_id || !settings?.google_client_secret) {
        toast.error('Please enter your Google Client ID and Secret, and click Save first.')
        return
      }
      toast.loading('Opening authentication screen in browser...', { id: 'gmail-connect' })
      const email = await window.api.gmail.connect()
      toast.success(`Connected to ${email}!`, { id: 'gmail-connect' })
      loadGmailStatus()
    } catch (err: any) {
      toast.error(`Connection failed: ${err.message}`, { id: 'gmail-connect' })
    }
  }

  const handleDisconnectGmail = async () => {
    await window.api.gmail.disconnect()
    toast.success('Gmail disconnected')
    loadGmailStatus()
  }

  const loadRules = async () => {
    const data = await window.api.automationRules.list()
    setRules(data)
  }

  const handleRuleSubmit = async () => {
    if (!ruleForm.rule_name.trim()) return toast.error('Rule name is required.')
    try {
      await window.api.automationRules.create(ruleForm)
      toast.success('Automation rule created!')
      setShowRuleModal(false)
      loadRules()
    } catch (err: any) {
      toast.error(err.message)
    }
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
    loadGmailStatus()
    window.dispatchEvent(new Event('settings-updated'))
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
        <div className="flex items-center gap-2">
          <button
            onClick={handleSave}
            disabled={!dirty}
            className={`btn-primary flex items-center gap-2 ${!dirty ? 'opacity-50 cursor-not-allowed' : ''}`}
          >
            <Save className="w-4 h-4" /> Save Changes
          </button>
          <button
            onClick={handleClose}
            className="p-2 rounded-lg hover:bg-surface-200 text-text-tertiary hover:text-text-primary transition-colors"
            title="Close settings (Esc)"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
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

      {/* Features */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-1">Features</h2>
        <p className="text-xs text-text-tertiary mb-4">
          Enable or disable optional modules in Billable.
        </p>
        <div className="flex items-center justify-between">
          <div>
            <div className="text-xs font-semibold text-text-primary">Bill & Subscription Tracking</div>
            <div className="text-[10px] text-text-tertiary mt-0.5">Track cash flow (money in & money out), pending bills, and subscriptions.</div>
          </div>
          <input
            type="checkbox"
            checked={settings.bill_tracking_enabled !== '0'}
            onChange={e => updateField('bill_tracking_enabled', e.target.checked ? '1' : '0')}
            className="w-4 h-4 text-accent border-rim/6 rounded focus:ring-accent bg-surface-100"
          />
        </div>
      </motion.div>

      {/* Gmail Integration */}
      {settings.bill_tracking_enabled !== '0' && (
        <motion.div variants={item} className="glass-panel p-6 mb-6">
          <h2 className="text-sm font-semibold text-text-primary mb-1">Gmail Connection</h2>
          <p className="text-xs text-text-tertiary mb-4">
            Connect your Google Account to automatically scan for bills, receipts, and subscriptions.
          </p>
          <div className="space-y-4">
            {!gmailStatus?.connected ? (
              <>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <label className="text-xs font-medium text-text-secondary mb-1.5 block">Google Client ID</label>
                    <input
                      className="input-field"
                      value={settings.google_client_id || ''}
                      onChange={e => updateField('google_client_id', e.target.value)}
                      placeholder="Enter OAuth Client ID"
                    />
                  </div>
                  <div>
                    <label className="text-xs font-medium text-text-secondary mb-1.5 block">Google Client Secret</label>
                    <input
                      className="input-field"
                      type="password"
                      value={settings.google_client_secret || ''}
                      onChange={e => updateField('google_client_secret', e.target.value)}
                      placeholder="Enter OAuth Client Secret"
                    />
                  </div>
                </div>
                <div className="flex items-center justify-between pt-2">
                  <span className="text-[11px] text-text-tertiary">
                    * Click "Save Changes" at the top before connecting.
                  </span>
                  <button
                    type="button"
                    onClick={handleConnectGmail}
                    disabled={!settings.google_client_id || !settings.google_client_secret}
                    className={`btn-primary text-xs ${(!settings.google_client_id || !settings.google_client_secret) ? 'opacity-50 cursor-not-allowed' : ''}`}
                  >
                    Connect Gmail
                  </button>
                </div>
              </>
            ) : (
              <div className="flex items-center justify-between p-4 rounded-lg bg-surface-200 border border-rim/6">
                <div>
                  <span className="text-[10px] uppercase font-bold tracking-wider text-text-tertiary">Connected Account</span>
                  <div className="text-sm font-semibold text-accent mt-0.5">{gmailStatus.email}</div>
                </div>
                <button
                  type="button"
                  onClick={handleDisconnectGmail}
                  className="btn-secondary text-red-400 hover:text-red-300 border-red-500/20 hover:bg-red-500/10 text-xs px-3 py-1.5"
                >
                  Disconnect
                </button>
              </div>
            )}
          </div>
        </motion.div>
      )}

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
                onClick={updater.install}
                disabled={updater.downloadState === 'downloading' || updater.downloadState === 'installing'}
                className="btn-primary flex items-center gap-2 text-xs"
              >
                <Download className="w-3.5 h-3.5" />
                {updater.downloadState === 'downloading'
                  ? `Downloading ${updater.progress?.percent ?? 0}%`
                  : updater.downloadState === 'installing'
                    ? 'Installing — relaunching…'
                    : updater.canInstall
                      ? 'Install & Relaunch'
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

      {/* Automation Rules */}
      {settings.bill_tracking_enabled !== '0' && (
        <motion.div variants={item} className="glass-panel p-6 mb-6">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-sm font-semibold text-text-primary">Automation Rules</h2>
            <button
              onClick={() => {
                setRuleForm({
                  rule_name: '',
                  sender_contains: '',
                  subject_contains: '',
                  vendor: '',
                  category: 'other',
                  record_type: 'bill',
                  recurring_frequency: 'monthly',
                  auto_approve: 0,
                })
                setShowRuleModal(true)
              }}
              className="text-xs text-accent hover:text-accent-light flex items-center gap-1"
            >
              <Plus className="w-3.5 h-3.5" /> Add Rule
            </button>
          </div>

          {rules.length === 0 ? (
            <p className="text-xs text-text-tertiary text-center py-4">
              No automation rules defined yet. Create rules to auto-categorize incoming emails.
            </p>
          ) : (
            <div className="space-y-2">
              {rules.map(rule => (
                <div key={rule.id} className="flex justify-between items-center p-3 rounded-xl bg-surface-200 border border-rim/6 text-xs">
                  <div>
                    <div className="font-semibold text-text-primary">{rule.rule_name}</div>
                    <div className="text-[10px] text-text-tertiary mt-0.5">
                      {rule.sender_contains && `Sender contains: "${rule.sender_contains}"`}
                      {rule.sender_contains && rule.subject_contains && ' AND '}
                      {rule.subject_contains && `Subject contains: "${rule.subject_contains}"`}
                    </div>
                    <div className="text-[10px] text-accent mt-1 font-medium">
                      Maps to: {rule.vendor || 'Extract Vendor'} ({rule.record_type}) · {rule.category}
                      {rule.auto_approve === 1 && <span className="ml-2 text-green-400 font-bold">[Auto-Approve]</span>}
                    </div>
                  </div>
                  <button
                    onClick={async () => {
                      await window.api.automationRules.delete(rule.id)
                      toast.success('Rule deleted')
                      loadRules()
                    }}
                    className="p-1.5 hover:bg-red-500/10 rounded transition-colors text-text-tertiary hover:text-red-400"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}
        </motion.div>
      )}

      {/* Modal: Create Automation Rule */}
      <Modal isOpen={showRuleModal} onClose={() => setShowRuleModal(false)} title="Create Automation Rule">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1 block">Rule Name</label>
            <input
              className="input-field"
              value={ruleForm.rule_name}
              onChange={e => setRuleForm({ ...ruleForm, rule_name: e.target.value })}
              placeholder="e.g. BC Hydro Rule"
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">If Sender contains</label>
              <input
                className="input-field"
                value={ruleForm.sender_contains}
                onChange={e => setRuleForm({ ...ruleForm, sender_contains: e.target.value })}
                placeholder="e.g. bchydro"
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">If Subject contains</label>
              <input
                className="input-field"
                value={ruleForm.subject_contains}
                onChange={e => setRuleForm({ ...ruleForm, subject_contains: e.target.value })}
                placeholder="e.g. bill"
              />
            </div>
          </div>
          <div className="p-3 bg-surface-200/50 rounded-lg border border-rim/6 space-y-3">
            <h3 className="text-xs font-semibold text-text-primary">Then Auto Assign</h3>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-[10px] font-medium text-text-secondary mb-1 block">Vendor</label>
                <input
                  className="input-field"
                  value={ruleForm.vendor}
                  onChange={e => setRuleForm({ ...ruleForm, vendor: e.target.value })}
                  placeholder="e.g. BC Hydro"
                />
              </div>
              <div>
                <label className="text-[10px] font-medium text-text-secondary mb-1 block">Category</label>
                <select
                  className="input-field"
                  value={ruleForm.category}
                  onChange={e => setRuleForm({ ...ruleForm, category: e.target.value })}
                >
                  <option value="utilities">Utilities</option>
                  <option value="software">Software</option>
                  <option value="phone_internet">Phone & Internet</option>
                  <option value="travel">Travel</option>
                  <option value="meals">Meals</option>
                  <option value="other">Other</option>
                </select>
              </div>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="text-[10px] font-medium text-text-secondary mb-1 block">Record Type</label>
                <select
                  className="input-field"
                  value={ruleForm.record_type}
                  onChange={e => setRuleForm({ ...ruleForm, record_type: e.target.value })}
                >
                  <option value="bill">Bill</option>
                  <option value="expense">Expense</option>
                  <option value="receipt">Receipt</option>
                  <option value="subscription">Subscription</option>
                  <option value="payment">Payment</option>
                </select>
              </div>
              <div>
                <label className="text-[10px] font-medium text-text-secondary mb-1 block">Recurring Frequency</label>
                <select
                  className="input-field"
                  value={ruleForm.recurring_frequency}
                  onChange={e => setRuleForm({ ...ruleForm, recurring_frequency: e.target.value })}
                >
                  <option value="one_time">One-time</option>
                  <option value="weekly">Weekly</option>
                  <option value="monthly">Monthly</option>
                  <option value="quarterly">Quarterly</option>
                  <option value="yearly">Yearly</option>
                </select>
              </div>
            </div>
            <div className="flex items-center justify-between pt-1">
              <div>
                <div className="text-[11px] font-semibold text-text-primary">Auto-Approve Candidate</div>
                <div className="text-[9px] text-text-tertiary">Directly import into records without review</div>
              </div>
              <input
                type="checkbox"
                checked={ruleForm.auto_approve === 1}
                onChange={e => setRuleForm({ ...ruleForm, auto_approve: e.target.checked ? 1 : 0 })}
                className="w-4 h-4 text-accent border-rim/6 rounded focus:ring-accent"
              />
            </div>
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <button onClick={() => setShowRuleModal(false)} className="btn-secondary text-xs">
              Cancel
            </button>
            <button onClick={handleRuleSubmit} className="btn-primary text-xs">
              Create Rule
            </button>
          </div>
        </div>
      </Modal>

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
