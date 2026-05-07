import { useState, useEffect } from 'react'
import { motion } from 'framer-motion'
import { Save, Info, Receipt } from 'lucide-react'
import type { TaxSettings, CanadianProvince } from '@shared/types'
import toast from 'react-hot-toast'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.06 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

const PROVINCES: { code: CanadianProvince; name: string }[] = [
  { code: 'AB', name: 'Alberta' },
  { code: 'BC', name: 'British Columbia' },
  { code: 'MB', name: 'Manitoba' },
  { code: 'NB', name: 'New Brunswick' },
  { code: 'NL', name: 'Newfoundland & Labrador' },
  { code: 'NS', name: 'Nova Scotia' },
  { code: 'NT', name: 'Northwest Territories' },
  { code: 'NU', name: 'Nunavut' },
  { code: 'ON', name: 'Ontario' },
  { code: 'PE', name: 'Prince Edward Island' },
  { code: 'QC', name: 'Quebec' },
  { code: 'SK', name: 'Saskatchewan' },
  { code: 'YT', name: 'Yukon' },
]

// Combined GST/HST rate that a registered freelancer charges federally.
// Provinces with HST blend GST + provincial portion. GST-only provinces
// charge just the 5% federal portion (provincial sales tax is separate
// and not collected via the same registration).
const PROVINCIAL_GST_HST: Record<CanadianProvince, { rate: number; label: string }> = {
  AB: { rate: 5, label: 'GST 5%' },
  BC: { rate: 5, label: 'GST 5%' },
  MB: { rate: 5, label: 'GST 5%' },
  NB: { rate: 15, label: 'HST 15%' },
  NL: { rate: 15, label: 'HST 15%' },
  NS: { rate: 15, label: 'HST 15%' },
  NT: { rate: 5, label: 'GST 5%' },
  NU: { rate: 5, label: 'GST 5%' },
  ON: { rate: 13, label: 'HST 13%' },
  PE: { rate: 15, label: 'HST 15%' },
  QC: { rate: 5, label: 'GST 5%' },
  SK: { rate: 5, label: 'GST 5%' },
  YT: { rate: 5, label: 'GST 5%' },
}

export default function TaxSettingsPage() {
  const [settings, setSettings] = useState<TaxSettings | null>(null)
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)

  useEffect(() => { loadSettings() }, [])

  const loadSettings = async () => {
    try {
      const s = await window.api.tax.getSettings()
      setSettings(s)
    } catch (err: any) {
      toast.error(`Failed to load tax settings: ${err.message || err}`)
    }
  }

  const updateField = <K extends keyof TaxSettings>(key: K, value: TaxSettings[K]) => {
    if (!settings) return
    setSettings({ ...settings, [key]: value })
    setDirty(true)
  }

  const handleProvinceChange = (province: CanadianProvince | '') => {
    if (!settings) return
    const next: TaxSettings = { ...settings, province }
    // If GST/HST registered and the rate looks like a default match for the previous
    // province (or zero), suggest the new province's rate.
    if (province && settings.gst_hst_registered) {
      next.default_tax_rate = PROVINCIAL_GST_HST[province].rate
    }
    setSettings(next)
    setDirty(true)
  }

  const handleGstToggle = (registered: boolean) => {
    if (!settings) return
    const next: TaxSettings = { ...settings, gst_hst_registered: registered ? 1 : 0 }
    if (registered && settings.province) {
      next.default_tax_rate = PROVINCIAL_GST_HST[settings.province as CanadianProvince].rate
    }
    setSettings(next)
    setDirty(true)
  }

  const handleSave = async () => {
    if (!settings) return
    setSaving(true)
    try {
      const saved = await window.api.tax.saveSettings(settings)
      setSettings(saved)
      setDirty(false)
      toast.success('Tax settings saved')
    } catch (err: any) {
      toast.error(`Failed to save: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  if (!settings) return null

  const provinceInfo = settings.province
    ? PROVINCIAL_GST_HST[settings.province as CanadianProvince]
    : null

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8 max-w-2xl">
      <motion.div variants={item} className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Tax Settings</h1>
          <p className="text-sm text-text-secondary mt-1">
            Used to estimate set-asides and populate invoices. Not tax advice.
          </p>
        </div>
        <button
          onClick={handleSave}
          disabled={!dirty || saving}
          className={`btn-primary flex items-center gap-2 ${!dirty || saving ? 'opacity-50 cursor-not-allowed' : ''}`}
        >
          <Save className="w-4 h-4" /> {saving ? 'Saving…' : 'Save Changes'}
        </button>
      </motion.div>

      {/* Business Identity */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-4">Business Identity</h2>
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Business Name</label>
            <input
              className="input-field"
              value={settings.business_name}
              onChange={e => updateField('business_name', e.target.value)}
              placeholder="Sole proprietor or business name"
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Business Address</label>
            <textarea
              className="input-field"
              rows={2}
              value={settings.business_address}
              onChange={e => updateField('business_address', e.target.value)}
              placeholder="Street, City, Province, Postal Code"
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Province</label>
              <select
                className="input-field"
                value={settings.province}
                onChange={e => handleProvinceChange(e.target.value as CanadianProvince | '')}
              >
                <option value="">Select province…</option>
                {PROVINCES.map(p => (
                  <option key={p.code} value={p.code}>{p.name}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Currency</label>
              <select
                className="input-field"
                value={settings.currency}
                onChange={e => updateField('currency', e.target.value as 'CAD' | 'USD')}
              >
                <option value="CAD">CAD — Canadian Dollar</option>
                <option value="USD">USD — US Dollar</option>
              </select>
            </div>
          </div>
        </div>
      </motion.div>

      {/* GST / HST */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <div className="flex items-start justify-between mb-4">
          <div>
            <h2 className="text-sm font-semibold text-text-primary">GST / HST</h2>
            <p className="text-xs text-text-tertiary mt-1">
              Required if you earn over $30,000 in revenue over four consecutive quarters.
            </p>
          </div>
          <label className="flex items-center gap-2 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={!!settings.gst_hst_registered}
              onChange={e => handleGstToggle(e.target.checked)}
              className="rounded border-rim/20 bg-surface-300 text-accent focus:ring-accent"
            />
            <span className="text-xs font-medium text-text-secondary">Registered</span>
          </label>
        </div>

        {!!settings.gst_hst_registered && (
          <div className="space-y-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">
                GST/HST Number
              </label>
              <input
                className="input-field font-mono"
                value={settings.gst_hst_number}
                onChange={e => updateField('gst_hst_number', e.target.value)}
                placeholder="123456789 RT0001"
              />
              <p className="text-xs text-text-tertiary mt-1">
                Shown on invoices when GST/HST is applied.
              </p>
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">
                Default Rate (%)
              </label>
              <div className="flex gap-3 items-center">
                <input
                  className="input-field w-32"
                  type="number"
                  step="0.01"
                  value={settings.default_tax_rate || ''}
                  onChange={e => updateField('default_tax_rate', parseFloat(e.target.value) || 0)}
                />
                {provinceInfo && (
                  <span className="text-xs text-text-tertiary">
                    Suggested for {settings.province}: <span className="text-accent font-mono">{provinceInfo.label}</span>
                  </span>
                )}
              </div>
            </div>
          </div>
        )}
      </motion.div>

      {/* Income Tax Estimate */}
      <motion.div variants={item} className="glass-panel p-6 mb-6">
        <h2 className="text-sm font-semibold text-text-primary mb-1">Income Tax Estimate</h2>
        <p className="text-xs text-text-tertiary mb-4">
          Used on the Tax Overview to estimate how much to set aside per paid invoice.
        </p>
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">
              Estimated personal income tax bracket (%)
            </label>
            <input
              className="input-field w-32"
              type="number"
              step="0.5"
              value={settings.income_tax_bracket || ''}
              onChange={e => updateField('income_tax_bracket', parseFloat(e.target.value) || 0)}
            />
            <div className="flex items-start gap-2 mt-2 text-xs text-text-tertiary">
              <Info className="w-3.5 h-3.5 flex-shrink-0 mt-0.5" />
              <span>
                A rough combined federal + provincial marginal rate. This is used to estimate set-asides only and does not constitute tax advice.
              </span>
            </div>
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Fiscal Year Start (MM-DD)</label>
            <input
              className="input-field font-mono w-32"
              value={settings.fiscal_year_start}
              onChange={e => updateField('fiscal_year_start', e.target.value)}
              placeholder="01-01"
            />
            <p className="text-xs text-text-tertiary mt-1">
              Most Canadian sole proprietors use a calendar year (01-01).
            </p>
          </div>
        </div>
      </motion.div>

      {/* Disclaimer */}
      <motion.div variants={item} className="flex items-start gap-3 p-4 rounded-lg bg-accent/[0.04] border border-accent/10">
        <Receipt className="w-4 h-4 text-accent flex-shrink-0 mt-0.5" />
        <p className="text-xs text-text-secondary leading-relaxed">
          Billable provides estimates to help you plan — not professional tax advice. Always consult a CPA or qualified tax professional before filing or remitting.
        </p>
      </motion.div>
    </motion.div>
  )
}
