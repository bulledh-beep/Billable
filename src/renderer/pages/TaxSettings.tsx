import { useState, useEffect } from 'react'
import { Info } from 'lucide-react'
import PageHeader from '../components/PageHeader'
import { Row, Section } from '../components/SettingsLayout'
import type { TaxSettings, CanadianProvince } from '@shared/types'
import toast from 'react-hot-toast'

const PROVINCES: { code: CanadianProvince; name: string }[] = [
  { code: 'AB', name: 'Alberta' },
  { code: 'BC', name: 'British Columbia' },
  { code: 'MB', name: 'Manitoba' },
  { code: 'NB', name: 'New Brunswick' },
  { code: 'NL', name: 'Newfoundland and Labrador' },
  { code: 'NS', name: 'Nova Scotia' },
  { code: 'NT', name: 'Northwest Territories' },
  { code: 'NU', name: 'Nunavut' },
  { code: 'ON', name: 'Ontario' },
  { code: 'PE', name: 'Prince Edward Island' },
  { code: 'QC', name: 'Quebec' },
  { code: 'SK', name: 'Saskatchewan' },
  { code: 'YT', name: 'Yukon' },
]

// Combined GST/HST rate a registered freelancer charges. HST provinces blend
// GST with the provincial portion; the rest charge the 5% federal GST.
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

  useEffect(() => {
    window.api.tax.getSettings()
      .then(setSettings)
      .catch((err: any) => toast.error(`Couldn't load tax settings: ${err.message || err}`))
  }, [])

  const update = (patch: Partial<TaxSettings>) => {
    setSettings(s => (s ? { ...s, ...patch } : s))
    setDirty(true)
  }

  const handleProvinceChange = (province: CanadianProvince | '') => {
    if (!settings) return
    const patch: Partial<TaxSettings> = { province }
    if (province && settings.gst_hst_registered) patch.default_tax_rate = PROVINCIAL_GST_HST[province].rate
    update(patch)
  }

  const handleGstToggle = (registered: boolean) => {
    if (!settings) return
    const patch: Partial<TaxSettings> = { gst_hst_registered: registered ? 1 : 0 }
    if (registered && settings.province) patch.default_tax_rate = PROVINCIAL_GST_HST[settings.province as CanadianProvince].rate
    update(patch)
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
      toast.error(`Couldn't save: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  if (!settings) return null

  const provinceInfo = settings.province ? PROVINCIAL_GST_HST[settings.province as CanadianProvince] : null

  return (
    <div className="page">
      <div className="max-w-[860px]">
      <PageHeader
        title="Tax settings"
        actions={<button onClick={handleSave} disabled={!dirty || saving} className="btn-primary">{saving ? 'Saving…' : 'Save changes'}</button>}
      />

      <Section title="Where you do business">
        <Row label="Legal name" help="Used on your tax summary.">
          <input className="input" value={settings.business_name} onChange={e => update({ business_name: e.target.value })} placeholder="Sole proprietor or business name" />
        </Row>
        <Row label="Address">
          <textarea className="input" rows={2} value={settings.business_address} onChange={e => update({ business_address: e.target.value })} placeholder="Street, city, province, postal code" />
        </Row>
        <Row label="Province" help="Sets the suggested GST/HST rate.">
          <select className="input max-w-[280px]" value={settings.province} onChange={e => handleProvinceChange(e.target.value as CanadianProvince | '')}>
            <option value="">Choose a province</option>
            {PROVINCES.map(p => <option key={p.code} value={p.code}>{p.name}</option>)}
          </select>
        </Row>
        <Row label="Reporting currency">
          <select className="input max-w-[280px]" value={settings.currency} onChange={e => update({ currency: e.target.value as 'CAD' | 'USD' })}>
            <option value="CAD">CAD, Canadian dollar</option>
            <option value="USD">USD, US dollar</option>
          </select>
        </Row>
      </Section>

      <Section title="GST/HST" description="You must register once you earn over $30,000 in four consecutive quarters.">
        <Row label="Registered" help="Turns on GST/HST for new invoices.">
          <label className="flex items-center gap-2.5 cursor-pointer select-none h-8">
            <input type="checkbox" checked={!!settings.gst_hst_registered} onChange={e => handleGstToggle(e.target.checked)} />
            <span className="text-sm text-fg">I'm registered for GST/HST</span>
          </label>
        </Row>
        {!!settings.gst_hst_registered && (
          <>
            <Row label="GST/HST number" help="Printed on invoices that charge GST/HST.">
              <input className="input max-w-[280px]" value={settings.gst_hst_number} onChange={e => update({ gst_hst_number: e.target.value })} placeholder="123456789 RT0001" />
            </Row>
            <Row label="Rate">
              <div className="flex items-center gap-3">
                <div className="relative w-28">
                  <input className="input pr-7 num" type="number" step="0.01" value={settings.default_tax_rate || ''} onChange={e => update({ default_tax_rate: parseFloat(e.target.value) || 0 })} />
                  <span className="absolute right-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">%</span>
                </div>
                {provinceInfo && <span className="text-xs text-fg-3">{settings.province} usually charges <span className="text-fg-2 font-medium">{provinceInfo.label}</span></span>}
              </div>
            </Row>
          </>
        )}
      </Section>

      <Section title="Income tax estimate" description="Used on Tax overview to suggest how much to set aside.">
        <Row label="Marginal tax rate" help="A rough combined federal and provincial rate.">
          <div className="relative w-28">
            <input className="input pr-7 num" type="number" step="0.5" value={settings.income_tax_bracket || ''} onChange={e => update({ income_tax_bracket: parseFloat(e.target.value) || 0 })} />
            <span className="absolute right-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">%</span>
          </div>
        </Row>
        <Row label="Fiscal year starts" help="Most sole proprietors use the calendar year, 01-01.">
          <input className="input w-28 num" value={settings.fiscal_year_start} onChange={e => update({ fiscal_year_start: e.target.value })} placeholder="01-01" />
        </Row>
      </Section>

      <div className="flex items-start gap-2.5 text-xs text-fg-3 leading-5">
        <Info className="w-3.5 h-3.5 shrink-0 mt-0.5" />
        <p>Billable gives estimates to help you plan. It isn't tax advice, so check with an accountant before you file or remit.</p>
      </div>
      </div>
    </div>
  )
}
