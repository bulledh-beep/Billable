import { useState, useEffect } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowLeft, Plus, Trash2 } from 'lucide-react'
import { formatMoney, todayISO, addDays } from '../utils/format'
import type { Client, Project, Settings, PaymentMethod } from '@shared/types'
import toast from 'react-hot-toast'

interface LineItem {
  description: string
  quantity: number
  unit_price: number
  total: number
}

export default function InvoiceCreate() {
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const presetProjectId = searchParams.get('project_id')
  const presetProjectIds = searchParams.get('project_ids')

  const [clients, setClients] = useState<Client[]>([])
  const [projects, setProjects] = useState<Project[]>([])
  const [clientId, setClientId] = useState(0)
  const [projectId, setProjectId] = useState(0)
  const [multiProjectIds, setMultiProjectIds] = useState<number[]>([])
  const [issueDate, setIssueDate] = useState(todayISO())
  const [dueDateOption, setDueDateOption] = useState('30')
  const [dueDate, setDueDate] = useState(addDays(todayISO(), 30))
  const [taxRate, setTaxRate] = useState(0)
  const [notes, setNotes] = useState('')
  const [items, setItems] = useState<LineItem[]>([])
  const [paymentMethods, setPaymentMethods] = useState<PaymentMethod[]>([])
  const [selectedPaymentMethod, setSelectedPaymentMethod] = useState('')

  useEffect(() => { loadData() }, [])

  const loadData = async () => {
    const [c, p, s] = await Promise.all([
      window.api.clients.list(),
      window.api.projects.list(),
      window.api.settings.get(),
    ])
    setClients(c)
    setProjects(p)

    // Load payment methods
    const methods: PaymentMethod[] = JSON.parse(s.payment_methods || '[]')
    setPaymentMethods(methods)

    // Pre-select default payment method
    const defaultMethod = methods.find(m => m.name === s.default_payment_method) || methods[0]
    if (defaultMethod) {
      setSelectedPaymentMethod(defaultMethod.name)
      setNotes(buildPaymentNote(defaultMethod))
    }

    if (presetProjectIds) {
      // Multi-project invoice
      const ids = presetProjectIds.split(',').map(Number).filter(n => !isNaN(n))
      if (ids.length > 0) {
        setMultiProjectIds(ids)
        const firstProj = p.find((pr: any) => ids.includes(pr.id))
        if (firstProj) setClientId(firstProj.client_id)
        await loadUnbilledEntriesMulti(ids, p)
      }
    } else if (presetProjectId) {
      const pid = parseInt(presetProjectId)
      setProjectId(pid)
      const proj = p.find((pr: any) => pr.id === pid)
      if (proj) {
        setClientId(proj.client_id)
        await loadUnbilledEntries(pid)
      }
    }
  }

  const loadUnbilledEntries = async (pid: number) => {
    const entries = await window.api.time.unbilled(pid)
    const project = projects.find(p => p.id === pid) || (await window.api.projects.get(pid))
    const rate = project?.rate || 0

    const lineItems: LineItem[] = entries.map((e: any) => ({
      description: e.description || `Work on ${new Date(e.start_time).toLocaleDateString()}`,
      quantity: parseFloat((e.duration_minutes / 60).toFixed(2)),
      unit_price: e.rate || rate,
      total: parseFloat(((e.duration_minutes / 60) * (e.rate || rate)).toFixed(2)),
    }))

    setItems(lineItems)
  }

  const loadUnbilledEntriesMulti = async (ids: number[], allProjects?: Project[]) => {
    const entries = await window.api.time.unbilledMulti(ids)
    const projList = allProjects || projects

    const lineItems: LineItem[] = entries.map((e: any) => {
      const proj = projList.find(p => p.id === e.project_id)
      const rate = e.rate || proj?.rate || 0
      return {
        description: `[${e.project_name || proj?.name || 'Project'}] ${e.description || `Work on ${new Date(e.start_time).toLocaleDateString()}`}`,
        quantity: parseFloat((e.duration_minutes / 60).toFixed(2)),
        unit_price: rate,
        total: parseFloat(((e.duration_minutes / 60) * rate).toFixed(2)),
      }
    })

    setItems(lineItems)
  }

  const handleProjectChange = async (pid: number) => {
    setProjectId(pid)
    const proj = projects.find(p => p.id === pid)
    if (proj) {
      setClientId(proj.client_id)
      await loadUnbilledEntries(pid)
    }
  }

  const handleClientChange = (cid: number) => {
    setClientId(cid)
    setProjectId(0)
    setItems([])
  }

  const buildPaymentNote = (method: PaymentMethod) => {
    if (method.email) return `Payment via ${method.name} — ${method.email}`
    return `Payment via ${method.name}`
  }

  const handlePaymentMethodChange = (name: string) => {
    setSelectedPaymentMethod(name)
    const method = paymentMethods.find(m => m.name === name)
    if (method) setNotes(buildPaymentNote(method))
  }

  const handleDueDateOptionChange = (opt: string) => {
    setDueDateOption(opt)
    if (opt !== 'custom') {
      setDueDate(addDays(issueDate, parseInt(opt)))
    }
  }

  const updateItem = (index: number, field: keyof LineItem, value: string | number) => {
    setItems(prev => prev.map((item, i) => {
      if (i !== index) return item
      const updated = { ...item, [field]: value }
      if (field === 'quantity' || field === 'unit_price') {
        updated.total = parseFloat((Number(updated.quantity) * Number(updated.unit_price)).toFixed(2))
      }
      return updated
    }))
  }

  const addItem = () => {
    setItems(prev => [...prev, { description: '', quantity: 1, unit_price: 0, total: 0 }])
  }

  const removeItem = (index: number) => {
    setItems(prev => prev.filter((_, i) => i !== index))
  }

  const subtotal = items.reduce((sum, item) => sum + item.total, 0)
  const taxAmount = subtotal * (taxRate / 100)
  const total = subtotal + taxAmount

  const handleCreate = async () => {
    if (!clientId) return toast.error('Select a client')
    if (items.length === 0) return toast.error('Add at least one line item')

    await window.api.invoices.create({
      client_id: clientId,
      project_id: multiProjectIds.length > 0 ? null : (projectId || null),
      project_ids: multiProjectIds.length > 0 ? multiProjectIds : (projectId ? [projectId] : []),
      issue_date: issueDate,
      due_date: dueDate,
      subtotal,
      tax_rate: taxRate,
      total,
      notes,
      items,
    })

    toast.success('Invoice created')
    navigate('/invoices')
  }

  const clientProjects = projects.filter(p => p.client_id === clientId)

  return (
    <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="p-8">
      <button
        onClick={() => navigate('/invoices')}
        className="flex items-center gap-1.5 text-sm text-text-secondary hover:text-text-primary transition-colors mb-6"
      >
        <ArrowLeft className="w-4 h-4" /> Invoices
      </button>

      <h1 className="text-2xl font-bold text-text-primary mb-6">Create Invoice</h1>

      <div className="grid grid-cols-2 gap-8">
        {/* Left Column - Details */}
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Client *</label>
            <select className="input-field" value={clientId} onChange={e => handleClientChange(parseInt(e.target.value))}>
              <option value={0}>Select client...</option>
              {clients.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
            </select>
          </div>

          {multiProjectIds.length > 0 ? (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Projects</label>
              <div className="glass-panel p-3 space-y-1">
                {multiProjectIds.map(id => {
                  const proj = projects.find(p => p.id === id)
                  return proj ? (
                    <div key={id} className="flex items-center gap-2 text-sm">
                      <div className="w-2.5 h-2.5 rounded-full" style={{ backgroundColor: proj.color }} />
                      <span className="text-text-primary">{proj.name}</span>
                      <span className="text-text-tertiary text-xs">({formatMoney(proj.rate)}/hr)</span>
                    </div>
                  ) : null
                })}
              </div>
            </div>
          ) : clientId > 0 && clientProjects.length > 0 ? (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Project</label>
              <select className="input-field" value={projectId} onChange={e => handleProjectChange(parseInt(e.target.value))}>
                <option value={0}>All projects</option>
                {clientProjects.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
          ) : null}

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Issue Date</label>
              <input className="input-field" type="date" value={issueDate} onChange={e => {
                setIssueDate(e.target.value)
                if (dueDateOption !== 'custom') setDueDate(addDays(e.target.value, parseInt(dueDateOption)))
              }} />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Payment Terms</label>
              <select className="input-field" value={dueDateOption} onChange={e => handleDueDateOptionChange(e.target.value)}>
                <option value="15">Net 15</option>
                <option value="30">Net 30</option>
                <option value="45">Net 45</option>
                <option value="custom">Custom</option>
              </select>
            </div>
          </div>

          {dueDateOption === 'custom' && (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Due Date</label>
              <input className="input-field" type="date" value={dueDate} onChange={e => setDueDate(e.target.value)} />
            </div>
          )}

          {paymentMethods.length > 0 && (
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Payment Method</label>
              <select
                className="input-field"
                value={selectedPaymentMethod}
                onChange={e => handlePaymentMethodChange(e.target.value)}
              >
                {paymentMethods.map((m, i) => (
                  <option key={i} value={m.name}>{m.name}{m.email ? ` — ${m.email}` : ''}</option>
                ))}
              </select>
            </div>
          )}

          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Tax Rate (%)</label>
            <input className="input-field" type="number" step="0.01" value={taxRate || ''} onChange={e => setTaxRate(parseFloat(e.target.value) || 0)} />
          </div>

          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Notes / Payment Instructions</label>
            <textarea className="input-field" rows={3} value={notes} onChange={e => setNotes(e.target.value)}
              placeholder="e.g., Payment via bank transfer to..." />
          </div>
        </div>

        {/* Right Column - Line Items */}
        <div>
          <div className="flex items-center justify-between mb-3">
            <label className="text-xs font-medium text-text-secondary">Line Items</label>
            <button onClick={addItem} className="text-xs text-accent hover:text-accent-light flex items-center gap-1">
              <Plus className="w-3 h-3" /> Add Item
            </button>
          </div>

          <div className="space-y-2 mb-4">
            {items.map((lineItem, i) => (
              <div key={i} className="glass-panel p-3 space-y-2">
                <input
                  className="input-field"
                  value={lineItem.description}
                  onChange={e => updateItem(i, 'description', e.target.value)}
                  placeholder="Description"
                />
                <div className="flex gap-2 items-center">
                  <input
                    className="input-field w-24"
                    type="number"
                    step="0.01"
                    value={lineItem.quantity || ''}
                    onChange={e => updateItem(i, 'quantity', parseFloat(e.target.value) || 0)}
                    placeholder="Hours"
                  />
                  <span className="text-xs text-text-tertiary">×</span>
                  <input
                    className="input-field w-24"
                    type="number"
                    step="0.01"
                    value={lineItem.unit_price || ''}
                    onChange={e => updateItem(i, 'unit_price', parseFloat(e.target.value) || 0)}
                    placeholder="Rate"
                  />
                  <span className="text-xs text-text-tertiary">=</span>
                  <span className="font-mono text-sm text-text-primary w-24 text-right">{formatMoney(lineItem.total)}</span>
                  <button onClick={() => removeItem(i)} className="p-1 hover:bg-red-500/10 rounded">
                    <Trash2 className="w-3.5 h-3.5 text-text-tertiary hover:text-red-400" />
                  </button>
                </div>
              </div>
            ))}
            {items.length === 0 && (
              <p className="text-sm text-text-tertiary text-center py-4">
                {projectId ? 'No unbilled entries found' : 'Select a project to auto-populate, or add items manually'}
              </p>
            )}
          </div>

          {/* Totals */}
          <div className="glass-panel p-4 space-y-2">
            <div className="flex justify-between text-sm">
              <span className="text-text-secondary">Subtotal</span>
              <span className="font-mono text-text-primary">{formatMoney(subtotal)}</span>
            </div>
            {taxRate > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-text-secondary">Tax ({taxRate}%)</span>
                <span className="font-mono text-text-primary">{formatMoney(taxAmount)}</span>
              </div>
            )}
            <div className="flex justify-between text-lg font-semibold pt-2 border-t border-white/[0.04]">
              <span className="text-text-primary">Total</span>
              <span className="font-mono text-accent">{formatMoney(total)}</span>
            </div>
          </div>

          <div className="flex justify-end gap-3 mt-6">
            <button onClick={() => navigate('/invoices')} className="btn-secondary">Cancel</button>
            <button onClick={handleCreate} className="btn-primary">Create Invoice</button>
          </div>
        </div>
      </div>
    </motion.div>
  )
}
