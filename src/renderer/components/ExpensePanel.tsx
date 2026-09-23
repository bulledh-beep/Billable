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
  { value: 'equipment', label: 'Equipment and hardware' },
  { value: 'software', label: 'Software and subscriptions' },
  { value: 'home_office', label: 'Home office', hint: 'Enter only the share of your home used for work.' },
  { value: 'phone_internet', label: 'Phone and internet' },
  { value: 'travel', label: 'Travel' },
  { value: 'meals', label: 'Meals and entertainment', hint: 'Only half is deductible, so enter the deductible half.' },
  { value: 'professional_development', label: 'Professional development' },
  { value: 'other', label: 'Other' },
]

const yearOf = (date: string) => parseInt(String(date).slice(0, 4)) || new Date().getFullYear()

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
      toast.error('Add a description')
      return
    }
    if (!form.amount || form.amount <= 0) {
      toast.error('Enter an amount above zero')
      return
    }
    setSaving(true)
    try {
      if (isEdit && expense) {
        await window.api.expenses.update(expense.id, {
          ...form,
          tax_year: yearOf(form.date),
        })
        toast.success('Expense updated')
      } else {
        await window.api.expenses.create({
          ...form,
          tax_year: yearOf(form.date),
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
    if (!confirm('Delete this expense? This can\'t be undone.')) return
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
            className="fixed top-0 right-0 bottom-0 w-[400px] bg-panel border-l border-line z-50 flex flex-col shadow-pop"
          >
            <div className="flex items-center justify-between px-5 h-[52px] border-b border-line">
              <div>
                <h2 className="text-base font-semibold text-fg">
                  {isEdit ? 'Edit expense' : 'New expense'}
                </h2>
                <p className="text-xs text-fg-3">
                  Counts toward {yearOf(form.date)}
                </p>
              </div>
              <button onClick={onClose} className="btn-icon-sm" aria-label="Close">
                <X />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-5 space-y-4">
              <div>
                <label className="label">Date</label>
                <input
                  className="input"
                  type="date"
                  value={form.date}
                  onChange={e => update('date', e.target.value)}
                />
              </div>

              <div>
                <label className="label">Category</label>
                <select
                  className="input"
                  value={form.category}
                  onChange={e => update('category', e.target.value as ExpenseCategory)}
                >
                  {CATEGORIES.map(c => (
                    <option key={c.value} value={c.value}>{c.label}</option>
                  ))}
                </select>
                {categoryHint && (
                  <p className="hint">{categoryHint}</p>
                )}
              </div>

              <div>
                <label className="label">Description</label>
                <input
                  className="input"
                  value={form.description}
                  onChange={e => update('description', e.target.value)}
                  placeholder="Adobe Creative Cloud, March"
                  autoFocus
                />
              </div>

              <div>
                <label className="label">Amount</label>
                <div className="relative">
                  <span className="absolute left-2.5 top-1/2 -translate-y-1/2 text-sm text-fg-3">$</span>
                  <input
                    className="input pl-6 num"
                    type="number"
                    step="0.01"
                    value={form.amount || ''}
                    onChange={e => update('amount', parseFloat(e.target.value) || 0)}
                    placeholder="0.00"
                  />
                </div>
              </div>

              <div>
                <label className="label">Receipt location</label>
                <input
                  className="input"
                  value={form.receipt_note}
                  onChange={e => update('receipt_note', e.target.value)}
                  placeholder="Dropbox/Receipts/2026/March"
                />
                <p className="hint">Where you keep the receipt, so you can find it at tax time.</p>
              </div>
            </div>

            <div className="px-5 py-3 border-t border-line flex items-center justify-between gap-3">
              {isEdit ? (
                <button onClick={handleDelete} className="btn-ghost hover:!text-red">
                  <Trash2 /> Delete
                </button>
              ) : <div />}
              <div className="flex gap-2">
                <button onClick={onClose} className="btn-secondary">Cancel</button>
                <button onClick={handleSave} disabled={saving} className="btn-primary">
                  {saving ? 'Saving…' : isEdit ? 'Save changes' : 'Add expense'}
                </button>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  )
}
