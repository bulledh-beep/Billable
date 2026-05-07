import { useEffect, useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { X, Trash2 } from 'lucide-react'
import type { Expense, ExpenseCategory } from '@shared/types'
import { todayISO } from '../utils/format'
import toast from 'react-hot-toast'

interface ExpensePanelProps {
  open: boolean
  expense?: Expense | null // null/undefined = create mode
  defaultTaxYear: number
  onClose: () => void
  onSaved: () => void
  onDeleted?: () => void
}

interface CategoryOption {
  value: ExpenseCategory
  label: string
  hint?: string
}

const CATEGORIES: CategoryOption[] = [
  { value: 'equipment', label: 'Equipment / Hardware' },
  { value: 'software', label: 'Software / Subscriptions' },
  { value: 'home_office', label: 'Home Office', hint: 'Use the % of your home used for work' },
  { value: 'phone_internet', label: 'Phone & Internet' },
  { value: 'travel', label: 'Travel' },
  { value: 'meals', label: 'Meals & Entertainment', hint: '50% deductible — only the deductible half should be entered' },
  { value: 'professional_development', label: 'Professional Development' },
  { value: 'other', label: 'Other' },
]

interface FormState {
  date: string
  category: ExpenseCategory
  description: string
  amount: number
  receipt_note: string
}

export default function ExpensePanel({
  open,
  expense,
  defaultTaxYear,
  onClose,
  onSaved,
  onDeleted,
}: ExpensePanelProps) {
  const isEdit = !!expense
  const [form, setForm] = useState<FormState>({
    date: todayISO(),
    category: 'other',
    description: '',
    amount: 0,
    receipt_note: '',
  })
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!open) return
    if (expense) {
      setForm({
        date: expense.date.slice(0, 10),
        category: expense.category,
        description: expense.description,
        amount: expense.amount,
        receipt_note: expense.receipt_note,
      })
    } else {
      // Default the date to Jan 1 of the selected tax year if it's in the past, else today
      const currentYear = new Date().getFullYear()
      const date = defaultTaxYear === currentYear ? todayISO() : `${defaultTaxYear}-01-01`
      setForm({ date, category: 'other', description: '', amount: 0, receipt_note: '' })
    }
  }, [open, expense, defaultTaxYear])

  const update = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm(prev => ({ ...prev, [key]: value }))
  }

  const handleSave = async () => {
    if (!form.description.trim()) {
      toast.error('Description is required')
      return
    }
    if (!form.amount || form.amount <= 0) {
      toast.error('Amount must be greater than 0')
      return
    }
    setSaving(true)
    try {
      if (isEdit && expense) {
        await window.api.expenses.update(expense.id, {
          ...form,
          tax_year: new Date(form.date).getFullYear(),
        })
        toast.success('Expense updated')
      } else {
        await window.api.expenses.create({
          ...form,
          tax_year: new Date(form.date).getFullYear(),
        })
        toast.success('Expense added')
      }
      onSaved()
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  const handleDelete = async () => {
    if (!expense) return
    if (!confirm('Delete this expense? This cannot be undone.')) return
    try {
      await window.api.expenses.delete(expense.id)
      toast.success('Expense deleted')
      onDeleted?.()
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    }
  }

  const categoryHint = CATEGORIES.find(c => c.value === form.category)?.hint

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
            className="fixed inset-0 bg-black/40 z-40"
          />
          {/* Panel */}
          <motion.div
            initial={{ x: 320, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            exit={{ x: 320, opacity: 0 }}
            transition={{ type: 'spring', damping: 28, stiffness: 280 }}
            className="fixed top-0 right-0 bottom-0 w-[400px] bg-surface-100 border-l border-white/[0.06] z-50 flex flex-col shadow-2xl"
          >
            <div className="flex items-center justify-between p-5 border-b border-white/[0.04]">
              <div>
                <h2 className="text-base font-semibold text-text-primary">
                  {isEdit ? 'Edit Expense' : 'New Expense'}
                </h2>
                <p className="text-xs text-text-tertiary mt-0.5">
                  Logged for tax year {new Date(form.date).getFullYear()}
                </p>
              </div>
              <button
                onClick={onClose}
                className="p-1.5 hover:bg-surface-200 rounded-lg transition-colors"
              >
                <X className="w-4 h-4 text-text-tertiary" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-5 space-y-4">
              <div>
                <label className="text-xs font-medium text-text-secondary mb-1.5 block">Date</label>
                <input
                  className="input-field"
                  type="date"
                  value={form.date}
                  onChange={e => update('date', e.target.value)}
                />
              </div>

              <div>
                <label className="text-xs font-medium text-text-secondary mb-1.5 block">Category</label>
                <select
                  className="input-field"
                  value={form.category}
                  onChange={e => update('category', e.target.value as ExpenseCategory)}
                >
                  {CATEGORIES.map(c => (
                    <option key={c.value} value={c.value}>{c.label}</option>
                  ))}
                </select>
                {categoryHint && (
                  <p className="text-[10px] text-accent/80 mt-1">{categoryHint}</p>
                )}
              </div>

              <div>
                <label className="text-xs font-medium text-text-secondary mb-1.5 block">Description</label>
                <input
                  className="input-field"
                  value={form.description}
                  onChange={e => update('description', e.target.value)}
                  placeholder="e.g. Adobe Creative Cloud, March"
                  autoFocus
                />
              </div>

              <div>
                <label className="text-xs font-medium text-text-secondary mb-1.5 block">Amount</label>
                <div className="relative">
                  <span className="absolute left-3 top-1/2 -translate-y-1/2 text-sm text-text-tertiary font-mono">$</span>
                  <input
                    className="input-field pl-7 font-mono"
                    type="number"
                    step="0.01"
                    value={form.amount || ''}
                    onChange={e => update('amount', parseFloat(e.target.value) || 0)}
                    placeholder="0.00"
                  />
                </div>
              </div>

              <div>
                <label className="text-xs font-medium text-text-secondary mb-1.5 block">Receipt Note</label>
                <input
                  className="input-field"
                  value={form.receipt_note}
                  onChange={e => update('receipt_note', e.target.value)}
                  placeholder="e.g. Receipt in Dropbox/2025/Mar/"
                />
                <p className="text-[10px] text-text-tertiary mt-1">
                  A pointer to where the receipt is stored. (Receipt attachments come in Phase 4.)
                </p>
              </div>
            </div>

            <div className="p-5 border-t border-white/[0.04] flex items-center justify-between gap-3">
              {isEdit ? (
                <button
                  onClick={handleDelete}
                  className="btn-ghost flex items-center gap-2 text-status-overdue hover:bg-red-500/10"
                >
                  <Trash2 className="w-4 h-4" /> Delete
                </button>
              ) : <div />}
              <div className="flex gap-2">
                <button onClick={onClose} className="btn-secondary">Cancel</button>
                <button
                  onClick={handleSave}
                  disabled={saving}
                  className={`btn-primary ${saving ? 'opacity-50 cursor-not-allowed' : ''}`}
                >
                  {saving ? 'Saving…' : isEdit ? 'Save Changes' : 'Add Expense'}
                </button>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  )
}
