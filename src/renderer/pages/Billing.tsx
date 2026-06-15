import { useState, useEffect, useMemo } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { useNavigate } from 'react-router-dom'
import {
  RefreshCw,
  Plus,
  Check,
  X,
  Pencil,
  Trash2,
  Mail,
  AlertTriangle,
  Clipboard,
  Sliders,
  CheckCircle2,
  DollarSign,
  TrendingUp,
  CreditCard,
  ArrowUpRight,
  ArrowDownRight,
  Calendar,
  ListFilter,
  Eye,
} from 'lucide-react'
import Modal from '../components/Modal'
import EmptyState from '../components/EmptyState'
import toast from 'react-hot-toast'
import { formatMoney, formatRelative } from '../utils/format'
import type { BillImportCandidate } from '@shared/types'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

type BillingTab = 'transactions' | 'pending_bills' | 'subscriptions' | 'inbox'
type InboxTab = 'needs_review' | 'ignored' | 'duplicate'

export default function Billing() {
  const navigate = useNavigate()
  const [activeTab, setActiveTab] = useState<BillingTab>('transactions')
  const [loading, setLoading] = useState(false)

  // Data states
  const [invoices, setInvoices] = useState<any[]>([])
  const [expenses, setExpenses] = useState<any[]>([])
  const [bills, setBills] = useState<any[]>([])
  const [subscriptions, setSubscriptions] = useState<any[]>([])
  const [candidates, setCandidates] = useState<BillImportCandidate[]>([])

  // Inbox specific states
  const [inboxTab, setInboxTab] = useState<InboxTab>('needs_review')
  const [syncing, setSyncing] = useState(false)
  const [gmailConnected, setGmailConnected] = useState<boolean | null>(null)
  const [selectedInboxIds, setSelectedInboxIds] = useState<number[]>([])

  // Filters for Transactions
  const [txFilterDate, setTxFilterDate] = useState<'this_month' | 'last_month' | 'this_year' | 'all'>('this_month')
  const [txFilterFlow, setTxFilterFlow] = useState<'all' | 'inflow' | 'outflow'>('all')

  // Modals state
  const [showLogExpenseModal, setShowLogExpenseModal] = useState(false)
  const [showLogPaymentModal, setShowLogPaymentModal] = useState(false)
  const [showLogBillModal, setShowLogBillModal] = useState(false)
  const [showPayBillModal, setShowPayBillModal] = useState(false)
  const [showAddSubscriptionModal, setShowAddSubscriptionModal] = useState(false)
  
  // Inbox modals
  const [showPasteModal, setShowPasteModal] = useState(false)
  const [showEditCandidateModal, setShowEditCandidateModal] = useState(false)
  const [showRuleModal, setShowRuleModal] = useState(false)

  // Forms state
  const [selectedBillToPay, setSelectedBillToPay] = useState<any | null>(null)
  const [payBillForm, setPayBillForm] = useState({
    payment_date: new Date().toISOString().slice(0, 10),
    payment_method: 'e-transfer',
    notes: '',
  })

  const [expenseForm, setExpenseForm] = useState({
    vendor: '',
    amount: '',
    category: 'other',
    date: new Date().toISOString().slice(0, 10),
    description: '',
    is_deductible: 1,
    currency: 'CAD',
  })

  const [paymentForm, setPaymentForm] = useState({
    vendor: '',
    amount: '',
    payment_date: new Date().toISOString().slice(0, 10),
    payment_method: 'e-transfer',
    notes: '',
    currency: 'CAD',
  })

  const [billForm, setBillForm] = useState({
    vendor: '',
    amount: '',
    due_date: new Date().toISOString().slice(0, 10),
    category: 'other',
    recurring: 0,
    frequency: 'one_time',
    currency: 'CAD',
    notes: '',
  })

  const [subscriptionForm, setSubscriptionForm] = useState({
    name: '',
    vendor: '',
    amount: '',
    billing_cycle: 'monthly',
    next_billing_date: new Date().toISOString().slice(0, 10),
    category: 'software',
    currency: 'CAD',
  })

  // Edit Candidate form state
  const [selectedCandidate, setSelectedCandidate] = useState<BillImportCandidate | null>(null)
  const [editCandidateForm, setEditCandidateForm] = useState({
    extracted_vendor: '',
    extracted_amount: 0,
    extracted_currency: 'CAD',
    extracted_category: 'other',
    extracted_record_type: 'bill' as any,
    extracted_due_date: '',
    extracted_payment_date: '',
    extracted_invoice_date: '',
    extracted_frequency: 'one_time',
    extracted_status: 'needs_review',
  })

  // Paste Text Form State
  const [pasteForm, setPasteForm] = useState({
    text: '',
    subject: '',
    sender: '',
  })

  // Automation Rule Form State
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

  // Load everything
  useEffect(() => {
    loadAllData()
    checkGmailStatus()
  }, [])

  useEffect(() => {
    setSelectedInboxIds([])
  }, [inboxTab])

  const checkGmailStatus = async () => {
    try {
      const status = await window.api.gmail.status()
      setGmailConnected(status.connected)
    } catch {
      setGmailConnected(false)
    }
  }

  const loadAllData = async () => {
    setLoading(true)
    try {
      const [invs, exps, bls, subs, cands] = await Promise.all([
        window.api.invoices.list(),
        window.api.expenses.list(),
        window.api.bills.list(),
        window.api.subscriptions.list(),
        window.api.candidates.list(),
      ])
      setInvoices(invs)
      setExpenses(exps)
      setBills(bls)
      setSubscriptions(subs)
      setCandidates(cands)
    } catch (err: any) {
      toast.error(`Error loading data: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

  // --- Transactions Map & Filters ---
  const allTransactions = useMemo(() => {
    const list: any[] = []

    // 1. Paid Invoices (Inflow)
    invoices.forEach(inv => {
      if (inv.status === 'paid') {
        list.push({
          id: `inflow-inv-${inv.id}`,
          date: inv.payment_date || inv.issue_date || inv.created_at?.slice(0, 10),
          type: 'inflow',
          sourceType: 'invoice',
          description: `Invoice #${inv.invoice_number} - Client: ${inv.client_name || 'General Client'}`,
          category: 'Income',
          amount: inv.total || 0,
          currency: inv.currency || 'CAD',
          originalItem: inv,
        })
      }
    })

    // 2. Expenses (Outflow)
    expenses.forEach(exp => {
      list.push({
        id: `outflow-exp-${exp.id}`,
        date: exp.date,
        type: 'outflow',
        sourceType: 'expense',
        description: `${exp.vendor || 'Expense'} ${exp.description ? `- ${exp.description}` : ''}`,
        category: exp.category || 'other',
        amount: exp.amount || 0,
        currency: exp.currency || 'CAD',
        originalItem: exp,
      })
    })

    // 3. Paid Bills (Outflow)
    bills.forEach(bill => {
      if (bill.status === 'paid') {
        list.push({
          id: `outflow-bill-${bill.id}`,
          date: bill.due_date || bill.updated_at?.slice(0, 10),
          type: 'outflow',
          sourceType: 'bill',
          description: `${bill.vendor || 'Bill'} ${bill.notes ? `- ${bill.notes}` : ''}`,
          category: bill.category || 'other',
          amount: bill.amount || 0,
          currency: bill.currency || 'CAD',
          originalItem: bill,
        })
      }
    })

    // Sort chronologically (newest first)
    return list.sort((a, b) => b.date.localeCompare(a.date))
  }, [invoices, expenses, bills])

  const filteredTransactions = useMemo(() => {
    let result = allTransactions

    // Filter by date range
    if (txFilterDate !== 'all') {
      const now = new Date()
      let startStr = ''
      if (txFilterDate === 'this_month') {
        startStr = new Date(now.getFullYear(), now.getMonth(), 1).toISOString().slice(0, 10)
      } else if (txFilterDate === 'last_month') {
        startStr = new Date(now.getFullYear(), now.getMonth() - 1, 1).toISOString().slice(0, 10)
      } else if (txFilterDate === 'this_year') {
        startStr = new Date(now.getFullYear(), 0, 1).toISOString().slice(0, 10)
      }
      result = result.filter(tx => tx.date >= startStr)
    }

    // Filter by flow
    if (txFilterFlow !== 'all') {
      result = result.filter(tx => tx.type === txFilterFlow)
    }

    return result;
  }, [allTransactions, txFilterDate, txFilterFlow])

  // --- Financial KPI Summary Metrics ---
  const summary = useMemo(() => {
    const now = new Date()
    const thisMonthStart = new Date(now.getFullYear(), now.getMonth(), 1).toISOString().slice(0, 10)

    // realized inflows/outflows this month
    const inflowThisMonth = allTransactions
      .filter(tx => tx.type === 'inflow' && tx.date >= thisMonthStart)
      .reduce((sum, tx) => sum + tx.amount, 0)

    const outflowThisMonth = allTransactions
      .filter(tx => tx.type === 'outflow' && tx.date >= thisMonthStart)
      .reduce((sum, tx) => sum + tx.amount, 0)

    // Outstanding Invoices (Inflow Pending)
    const pendingInflow = invoices
      .filter(inv => ['sent', 'overdue'].includes(inv.status))
      .reduce((sum, inv) => sum + (inv.total || 0), 0)

    // Unpaid Bills (Outflow Pending)
    const pendingOutflow = bills
      .filter(bill => bill.status !== 'paid')
      .reduce((sum, bill) => sum + (bill.amount || 0), 0)

    return {
      inflowThisMonth,
      outflowThisMonth,
      netThisMonth: inflowThisMonth - outflowThisMonth,
      pendingInflow,
      pendingOutflow,
    }
  }, [allTransactions, invoices, bills])

  // --- Email Sync & Candidates Filter ---
  const filteredCandidates = useMemo(() => {
    return candidates.filter(c => {
      if (inboxTab === 'needs_review') {
        return c.review_status === 'needs_review'
      } else if (inboxTab === 'ignored') {
        return c.review_status === 'ignored'
      } else {
        return c.review_status === 'duplicate'
      }
    })
  }, [candidates, inboxTab])

  // --- Gmail sync & Candidate Action Helpers ---
  const handleSyncGmail = async () => {
    if (gmailConnected === false) {
      toast.error('Gmail account is not connected. Please connect it in Settings first.')
      navigate('/settings')
      return
    }
    setSyncing(true)
    toast.loading('Scanning Gmail inbox (last 30 days)...', { id: 'gmail-sync' })
    try {
      const res = await window.api.gmail.sync(30)
      toast.success(`Sync complete! Fetched ${res.fetched} new bills, skipped ${res.skipped} duplicates.`, { id: 'gmail-sync' })
      loadAllData()
    } catch (err: any) {
      toast.error(`Sync failed: ${err.message}`, { id: 'gmail-sync' })
    } finally {
      setSyncing(false)
    }
  }

  const approveCandidateHelper = async (candidate: BillImportCandidate) => {
    if (candidate.extracted_record_type === 'bill') {
      await window.api.bills.create({
        vendor: candidate.extracted_vendor || 'Unknown Vendor',
        amount: candidate.extracted_amount || 0,
        currency: candidate.extracted_currency || 'CAD',
        due_date: candidate.extracted_due_date,
        category: candidate.extracted_category || 'other',
        recurring: candidate.extracted_frequency !== 'one_time' ? 1 : 0,
        frequency: candidate.extracted_frequency || 'one_time',
        notes: `Imported candidate from email.`,
        source: 'email',
      })
    } else if (candidate.extracted_record_type === 'expense' || candidate.extracted_record_type === 'receipt') {
      await window.api.expenses.create({
        date: candidate.extracted_payment_date || candidate.extracted_due_date || new Date().toISOString().slice(0, 10),
        category: (candidate.extracted_category || 'other') as any,
        description: `Imported from email: ${candidate.extracted_vendor}`,
        amount: candidate.extracted_amount || 0,
        vendor: candidate.extracted_vendor || 'Unknown Vendor',
        currency: candidate.extracted_currency || 'CAD',
        source: 'email',
      })
    } else if (candidate.extracted_record_type === 'subscription') {
      await window.api.subscriptions.create({
        name: candidate.extracted_vendor || 'Subscription Service',
        vendor: candidate.extracted_vendor || 'Unknown Vendor',
        amount: candidate.extracted_amount || 0,
        currency: candidate.extracted_currency || 'CAD',
        billing_cycle: candidate.extracted_frequency || 'monthly',
        next_billing_date: candidate.extracted_due_date || candidate.extracted_payment_date,
        category: candidate.extracted_category || 'software',
        status: 'active',
      })
    } else if (candidate.extracted_record_type === 'payment') {
      await window.api.payments.create({
        amount: candidate.extracted_amount || 0,
        currency: candidate.extracted_currency || 'CAD',
        payment_date: candidate.extracted_payment_date || new Date().toISOString().slice(0, 10),
        notes: `Imported payment for ${candidate.extracted_vendor}`,
        vendor: candidate.extracted_vendor || 'Unknown Vendor',
      })
    }

    await window.api.candidates.update(candidate.id, { review_status: 'approved' })
  }

  const handleApproveCandidate = async (candidate: BillImportCandidate) => {
    toast.loading('Processing approval...', { id: 'approve-candidate' })
    try {
      await approveCandidateHelper(candidate)
      toast.success('Approved and imported successfully!', { id: 'approve-candidate' })
      loadAllData()
    } catch (err: any) {
      toast.error(`Approval failed: ${err.message}`, { id: 'approve-candidate' })
    }
  }

  const handleBulkApprove = async () => {
    if (selectedInboxIds.length === 0) return
    const candidatesToApprove = candidates.filter(c => selectedInboxIds.includes(c.id))
    toast.loading(`Approving ${candidatesToApprove.length} candidates...`, { id: 'bulk-approve' })
    try {
      let successCount = 0
      for (const c of candidatesToApprove) {
        try {
          await approveCandidateHelper(c)
          successCount++
        } catch (err: any) {
          console.error(`Failed to approve candidate ${c.id}: ${err.message}`)
        }
      }
      toast.success(`Successfully approved ${successCount} candidates.`, { id: 'bulk-approve' })
      setSelectedInboxIds([])
      loadAllData()
    } catch (err: any) {
      toast.error(`Bulk approval failed: ${err.message}`, { id: 'bulk-approve' })
    }
  }

  const handleIgnoreCandidate = async (id: number) => {
    try {
      await window.api.candidates.update(id, { review_status: 'ignored' })
      toast.success('Candidate ignored')
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleBulkIgnore = async () => {
    if (selectedInboxIds.length === 0) return
    if (!confirm(`Are you sure you want to ignore the ${selectedInboxIds.length} selected candidates?`)) return
    toast.loading(`Ignoring candidates...`, { id: 'bulk-ignore' })
    try {
      await Promise.all(selectedInboxIds.map(id => window.api.candidates.update(id, { review_status: 'ignored' })))
      toast.success(`Successfully ignored ${selectedInboxIds.length} candidates.`, { id: 'bulk-ignore' })
      setSelectedInboxIds([])
      loadAllData()
    } catch (err: any) {
      toast.error(`Bulk ignore failed: ${err.message}`, { id: 'bulk-ignore' })
    }
  }

  const handleBulkDelete = async () => {
    if (selectedInboxIds.length === 0) return
    if (!confirm(`Are you sure you want to permanently delete the ${selectedInboxIds.length} selected candidates?`)) return
    toast.loading(`Deleting candidates...`, { id: 'bulk-delete' })
    try {
      await Promise.all(selectedInboxIds.map(id => window.api.candidates.delete(id)))
      toast.success(`Successfully deleted ${selectedInboxIds.length} candidates.`, { id: 'bulk-delete' })
      setSelectedInboxIds([])
      loadAllData()
    } catch (err: any) {
      toast.error(`Bulk delete failed: ${err.message}`, { id: 'bulk-delete' })
    }
  }

  const toggleSelectInbox = (id: number) => {
    setSelectedInboxIds(prev =>
      prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]
    )
  }

  const toggleSelectAllInbox = () => {
    const visibleIds = filteredCandidates.map(c => c.id)
    const allSelected = visibleIds.every(id => selectedInboxIds.includes(id))
    if (allSelected) {
      setSelectedInboxIds(prev => prev.filter(id => !visibleIds.includes(id)))
    } else {
      setSelectedInboxIds(prev => Array.from(new Set([...prev, ...visibleIds])))
    }
  }

  // --- Manual Transaction Submissions ---
  const handleLogExpenseSubmit = async () => {
    if (!expenseForm.vendor.trim() || !expenseForm.amount) {
      return toast.error('Please enter a vendor and amount.')
    }
    try {
      await window.api.expenses.create({
        vendor: expenseForm.vendor,
        amount: parseFloat(expenseForm.amount),
        category: expenseForm.category,
        date: expenseForm.date,
        description: expenseForm.description,
        is_deductible: expenseForm.is_deductible ? 1 : 0,
        currency: expenseForm.currency,
      })
      toast.success('Expense logged successfully!')
      setShowLogExpenseModal(false)
      setExpenseForm({
        vendor: '',
        amount: '',
        category: 'other',
        date: new Date().toISOString().slice(0, 10),
        description: '',
        is_deductible: 1,
        currency: 'CAD',
      })
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleLogPaymentSubmit = async () => {
    if (!paymentForm.vendor.trim() || !paymentForm.amount) {
      return toast.error('Please enter client/vendor name and amount.')
    }
    try {
      await window.api.payments.create({
        vendor: paymentForm.vendor,
        amount: parseFloat(paymentForm.amount),
        payment_date: paymentForm.payment_date,
        payment_method: paymentForm.payment_method,
        notes: paymentForm.notes,
        currency: paymentForm.currency,
      })
      toast.success('Inflow payment logged successfully!')
      setShowLogPaymentModal(false)
      setPaymentForm({
        vendor: '',
        amount: '',
        payment_date: new Date().toISOString().slice(0, 10),
        payment_method: 'e-transfer',
        notes: '',
        currency: 'CAD',
      })
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleLogBillSubmit = async () => {
    if (!billForm.vendor.trim() || !billForm.amount) {
      return toast.error('Please enter vendor and amount.')
    }
    try {
      await window.api.bills.create({
        vendor: billForm.vendor,
        amount: parseFloat(billForm.amount),
        due_date: billForm.due_date,
        category: billForm.category,
        recurring: billForm.recurring ? 1 : 0,
        frequency: billForm.frequency,
        currency: billForm.currency,
        notes: billForm.notes,
        status: 'upcoming',
      })
      toast.success('Pending bill logged successfully!')
      setShowLogBillModal(false)
      setBillForm({
        vendor: '',
        amount: '',
        due_date: new Date().toISOString().slice(0, 10),
        category: 'other',
        recurring: 0,
        frequency: 'one_time',
        currency: 'CAD',
        notes: '',
      })
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handlePayBillClick = (bill: any) => {
    setSelectedBillToPay(bill)
    setPayBillForm({
      payment_date: new Date().toISOString().slice(0, 10),
      payment_method: 'e-transfer',
      notes: '',
    })
    setShowPayBillModal(true)
  }

  const handlePayBillSubmit = async () => {
    if (!selectedBillToPay) return
    try {
      await window.api.payments.create({
        bill_id: selectedBillToPay.id,
        amount: selectedBillToPay.amount,
        currency: selectedBillToPay.currency || 'CAD',
        payment_date: payBillForm.payment_date,
        payment_method: payBillForm.payment_method,
        notes: `Paid bill: ${payBillForm.notes || ''}`.trim(),
      })
      toast.success('Bill marked as Paid!')
      setShowPayBillModal(false)
      setSelectedBillToPay(null)
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleDeleteBill = async (id: number) => {
    if (!confirm('Are you sure you want to delete this bill?')) return
    try {
      await window.api.bills.delete(id)
      toast.success('Bill deleted')
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  // --- Subscriptions ---
  const handleAddSubscriptionSubmit = async () => {
    if (!subscriptionForm.name.trim() || !subscriptionForm.amount) {
      return toast.error('Please enter name and amount.')
    }
    try {
      await window.api.subscriptions.create({
        name: subscriptionForm.name,
        vendor: subscriptionForm.vendor || subscriptionForm.name,
        amount: parseFloat(subscriptionForm.amount),
        billing_cycle: subscriptionForm.billing_cycle,
        next_billing_date: subscriptionForm.next_billing_date,
        category: subscriptionForm.category,
        currency: subscriptionForm.currency,
        status: 'active',
      })
      toast.success('Subscription added!')
      setShowAddSubscriptionModal(false)
      setSubscriptionForm({
        name: '',
        vendor: '',
        amount: '',
        billing_cycle: 'monthly',
        next_billing_date: new Date().toISOString().slice(0, 10),
        category: 'software',
        currency: 'CAD',
      })
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleDeleteSubscription = async (id: number) => {
    if (!confirm('Are you sure you want to delete this subscription?')) return
    try {
      await window.api.subscriptions.delete(id)
      toast.success('Subscription deleted')
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const toggleSubscriptionStatus = async (sub: any) => {
    const nextStatus = sub.status === 'active' ? 'paused' : 'active'
    try {
      await window.api.subscriptions.update(sub.id, { status: nextStatus })
      toast.success(`Subscription ${nextStatus === 'active' ? 'resumed' : 'paused'}`)
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  // --- Candidate Specific Modal Trigger Helpers ---
  const handleEditCandidateClick = (candidate: BillImportCandidate) => {
    setSelectedCandidate(candidate)
    setEditCandidateForm({
      extracted_vendor: candidate.extracted_vendor || '',
      extracted_amount: candidate.extracted_amount || 0,
      extracted_currency: candidate.extracted_currency || 'CAD',
      extracted_category: candidate.extracted_category || 'other',
      extracted_record_type: candidate.extracted_record_type || 'bill',
      extracted_due_date: candidate.extracted_due_date || '',
      extracted_payment_date: candidate.extracted_payment_date || '',
      extracted_invoice_date: candidate.extracted_invoice_date || '',
      extracted_frequency: candidate.extracted_frequency || 'one_time',
      extracted_status: candidate.extracted_status || 'needs_review',
    })
    setShowEditCandidateModal(true)
  }

  const handleEditCandidateSubmit = async () => {
    if (!selectedCandidate) return
    try {
      await window.api.candidates.update(selectedCandidate.id, editCandidateForm)
      toast.success('Candidate updated')
      setShowEditCandidateModal(false)
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleCreateRuleClick = (candidate: BillImportCandidate) => {
    setRuleForm({
      rule_name: `Auto ${candidate.extracted_vendor || 'Vendor'}`,
      sender_contains: candidate.extracted_vendor || '',
      subject_contains: '',
      vendor: candidate.extracted_vendor || '',
      category: candidate.extracted_category || 'other',
      record_type: candidate.extracted_record_type || 'bill',
      recurring_frequency: candidate.extracted_frequency || 'monthly',
      auto_approve: 0,
    })
    setShowRuleModal(true)
  }

  const handleRuleSubmit = async () => {
    if (!ruleForm.rule_name.trim()) return toast.error('Rule name is required.')
    try {
      await window.api.automationRules.create(ruleForm)
      toast.success('Automation rule created!')
      setShowRuleModal(false)
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handlePasteSubmit = async () => {
    if (!pasteForm.text.trim()) {
      return toast.error('Please paste email text content.')
    }
    setLoading(true)
    try {
      await window.api.candidates.parseText(pasteForm.text, pasteForm.subject, pasteForm.sender)
      toast.success('Email parsed successfully!')
      setShowPasteModal(false)
      setPasteForm({ text: '', subject: '', sender: '' })
      loadAllData()
    } catch (err: any) {
      toast.error(`Error parsing text: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

  const handleDeleteCandidate = async (id: number) => {
    if (!confirm('Are you sure you want to delete this candidate?')) return
    try {
      await window.api.candidates.delete(id)
      toast.success('Candidate deleted')
      loadAllData()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  // --- Transaction log item delete helper ---
  const handleTxItemDelete = async (tx: any) => {
    if (!confirm(`Are you sure you want to delete this ${tx.sourceType} transaction?`)) return
    try {
      if (tx.sourceType === 'expense') {
        await window.api.expenses.delete(tx.originalItem.id)
      } else if (tx.sourceType === 'bill') {
        await window.api.bills.delete(tx.originalItem.id)
      } else if (tx.sourceType === 'invoice') {
        const paymentsList = await window.api.payments.list()
        const relatedPayment = paymentsList.find((p: any) => p.invoice_id === tx.originalItem.id)
        if (relatedPayment) {
          await window.api.payments.delete(relatedPayment.id)
          await window.api.invoices.update(tx.originalItem.id, { status: 'sent', payment_date: null, payment_method: null })
        } else {
          await window.api.invoices.delete(tx.originalItem.id)
        }
      }
      toast.success('Transaction removed')
      loadAllData()
    } catch (err: any) {
      toast.error(`Failed to delete transaction: ${err.message}`)
    }
  }

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8 font-sans">
      {/* Header section */}
      <motion.div variants={item} className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Billing</h1>
          <p className="text-sm text-text-secondary mt-1">
            Track money in and money out, manage subscriptions, and sync billing invoices.
          </p>
        </div>
        
        {/* Quick Log Buttons */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => setShowLogPaymentModal(true)}
            className="btn-secondary flex items-center gap-1 text-xs text-green-400 border-green-500/10 hover:bg-green-500/5 py-1.5 px-3 rounded-lg"
          >
            <Plus className="w-3.5 h-3.5" /> Log Money In
          </button>
          <button
            onClick={() => setShowLogExpenseModal(true)}
            className="btn-secondary flex items-center gap-1 text-xs text-red-400 border-red-500/10 hover:bg-red-500/5 py-1.5 px-3 rounded-lg"
          >
            <Plus className="w-3.5 h-3.5" /> Log Money Out
          </button>
          <button
            onClick={() => setShowLogBillModal(true)}
            className="btn-primary flex items-center gap-1 text-xs py-1.5 px-3 rounded-lg"
          >
            <Plus className="w-3.5 h-3.5" /> Log Bill
          </button>
        </div>
      </motion.div>

      {/* Tabs */}
      <motion.div variants={item} className="border-b border-rim/[0.04] mb-6 flex overflow-x-auto gap-6">
        {[
          { id: 'transactions', label: 'Cash Flow & Transactions', count: 0 },
          { id: 'pending_bills', label: 'Pending Bills', count: bills.filter(b => b.status !== 'paid').length },
          { id: 'subscriptions', label: 'Subscriptions', count: subscriptions.filter(s => s.status === 'active').length },
          { id: 'inbox', label: 'Import Inbox', count: candidates.filter(c => c.review_status === 'needs_review').length },
        ].map(tab => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id as BillingTab)}
            className={`pb-3 text-xs font-semibold relative transition-colors ${
              activeTab === tab.id ? 'text-accent border-b-2 border-accent' : 'text-text-secondary hover:text-text-primary'
            }`}
          >
            {tab.label}
            {tab.count > 0 && (
              <span className="ml-1.5 px-1.5 py-0.5 rounded-full text-[10px] bg-accent/10 text-accent font-mono font-bold">
                {tab.count}
              </span>
            )}
          </button>
        ))}
      </motion.div>

      {/* Tab Contents */}
      <div className="space-y-6">
        {/* TAB 1: TRANSACTIONS */}
        {activeTab === 'transactions' && (
          <motion.div variants={container} initial="hidden" animate="show" className="space-y-6">
            
            {/* KPI Cards Grid */}
            <div className="grid grid-cols-5 gap-4">
              <div className="glass-panel p-4 relative overflow-hidden">
                <div className="flex items-center justify-between mb-2">
                  <div className="w-8 h-8 rounded-lg bg-green-500/10 flex items-center justify-center">
                    <ArrowUpRight className="w-4 h-4 text-green-400" />
                  </div>
                  <span className="text-[10px] text-text-tertiary font-medium">Realized</span>
                </div>
                <div className="font-mono text-xl font-bold text-text-primary">
                  {formatMoney(summary.inflowThisMonth)}
                </div>
                <div className="text-xs text-text-tertiary mt-1">Inflow (This Month)</div>
              </div>

              <div className="glass-panel p-4 relative overflow-hidden">
                <div className="flex items-center justify-between mb-2">
                  <div className="w-8 h-8 rounded-lg bg-red-500/10 flex items-center justify-center">
                    <ArrowDownRight className="w-4 h-4 text-red-400" />
                  </div>
                  <span className="text-[10px] text-text-tertiary font-medium">Realized</span>
                </div>
                <div className="font-mono text-xl font-bold text-red-400">
                  -{formatMoney(summary.outflowThisMonth)}
                </div>
                <div className="text-xs text-text-tertiary mt-1">Outflow (This Month)</div>
              </div>

              <div className="glass-panel p-4 relative overflow-hidden">
                <div className="flex items-center justify-between mb-2">
                  <div className="w-8 h-8 rounded-lg bg-accent/10 flex items-center justify-center">
                    <TrendingUp className="w-4 h-4 text-accent" />
                  </div>
                  <span className="text-[10px] text-text-tertiary font-medium">Net realized</span>
                </div>
                <div className={`font-mono text-xl font-bold ${summary.netThisMonth >= 0 ? 'text-green-400' : 'text-red-400'}`}>
                  {summary.netThisMonth >= 0 ? '+' : ''}{formatMoney(summary.netThisMonth)}
                </div>
                <div className="text-xs text-text-tertiary mt-1">Net Cash Flow (Month)</div>
              </div>

              <div className="glass-panel p-4 relative overflow-hidden">
                <div className="flex items-center justify-between mb-2">
                  <div className="w-8 h-8 rounded-lg bg-yellow-500/10 flex items-center justify-center">
                    <DollarSign className="w-4 h-4 text-yellow-400" />
                  </div>
                  <span className="text-[10px] text-text-tertiary font-medium">Unrealized Inflow</span>
                </div>
                <div className="font-mono text-xl font-bold text-text-primary">
                  {formatMoney(summary.pendingInflow)}
                </div>
                <div className="text-xs text-text-tertiary mt-1">Outstanding Invoices</div>
              </div>

              <div className="glass-panel p-4 relative overflow-hidden">
                <div className="flex items-center justify-between mb-2">
                  <div className="w-8 h-8 rounded-lg bg-purple-500/10 flex items-center justify-center">
                    <CreditCard className="w-4 h-4 text-purple-400" />
                  </div>
                  <span className="text-[10px] text-text-tertiary font-medium">Unrealized Outflow</span>
                </div>
                <div className="font-mono text-xl font-bold text-yellow-500">
                  {formatMoney(summary.pendingOutflow)}
                </div>
                <div className="text-xs text-text-tertiary mt-1">Pending Bills</div>
              </div>
            </div>

            {/* Filters bar */}
            <div className="flex justify-between items-center bg-surface-200 border border-rim/6 p-3 rounded-xl">
              <div className="flex items-center gap-4 text-xs">
                <div className="flex items-center gap-1.5 text-text-secondary">
                  <Calendar className="w-3.5 h-3.5" />
                  <span>Period:</span>
                </div>
                <div className="flex bg-surface-100 p-0.5 rounded-lg border border-rim/4">
                  {[
                    { id: 'this_month', label: 'This Month' },
                    { id: 'last_month', label: 'Last Month' },
                    { id: 'this_year', label: 'This Year' },
                    { id: 'all', label: 'All Time' },
                  ].map(opt => (
                    <button
                      key={opt.id}
                      onClick={() => setTxFilterDate(opt.id as any)}
                      className={`px-3 py-1 rounded-md transition-colors ${
                        txFilterDate === opt.id ? 'bg-surface-200 font-semibold text-text-primary' : 'text-text-secondary hover:text-text-primary'
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="flex items-center gap-4 text-xs">
                <div className="flex items-center gap-1.5 text-text-secondary">
                  <ListFilter className="w-3.5 h-3.5" />
                  <span>Filter Flow:</span>
                </div>
                <div className="flex bg-surface-100 p-0.5 rounded-lg border border-rim/4">
                  {[
                    { id: 'all', label: 'All Transactions' },
                    { id: 'inflow', label: 'Money In (Inflow)' },
                    { id: 'outflow', label: 'Money Out (Outflow)' },
                  ].map(opt => (
                    <button
                      key={opt.id}
                      onClick={() => setTxFilterFlow(opt.id as any)}
                      className={`px-3 py-1 rounded-md transition-colors ${
                        txFilterFlow === opt.id ? 'bg-surface-200 font-semibold text-text-primary' : 'text-text-secondary hover:text-text-primary'
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>

            {/* Transactions Log */}
            <div className="glass-panel p-5">
              <h2 className="text-sm font-semibold text-text-primary mb-4">Transaction History Log</h2>

              {loading && filteredTransactions.length === 0 ? (
                <div className="flex justify-center py-12">
                  <RefreshCw className="w-6 h-6 text-accent animate-spin" />
                </div>
              ) : filteredTransactions.length === 0 ? (
                <EmptyState
                  icon={CreditCard}
                  title="No Transactions Logged"
                  description="There are no cash events in this filter period. Log an expense or payment to begin tracking."
                />
              ) : (
                <div className="overflow-x-auto">
                  <table className="w-full text-left border-collapse">
                    <thead>
                      <tr className="border-b border-rim/6 text-[10px] uppercase font-bold tracking-wider text-text-tertiary">
                        <th className="pb-3 pl-3">Date</th>
                        <th className="pb-3">Flow Type</th>
                        <th className="pb-3">Source</th>
                        <th className="pb-3">Description</th>
                        <th className="pb-3">Category</th>
                        <th className="pb-3 text-right pr-3">Amount</th>
                        <th className="pb-3 text-center">Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredTransactions.map(tx => (
                        <tr key={tx.id} className="border-b border-rim/[0.04] text-xs hover:bg-surface-200/40 transition-colors">
                          <td className="py-3 pl-3 font-mono text-text-secondary">{tx.date}</td>
                          <td className="py-3">
                            <span className={`px-2 py-0.5 rounded-full text-[9px] uppercase tracking-wide font-mono font-bold ${
                              tx.type === 'inflow' ? 'bg-green-500/10 text-green-400' : 'bg-red-500/10 text-red-400'
                            }`}>
                              {tx.type === 'inflow' ? 'Money In' : 'Money Out'}
                            </span>
                          </td>
                          <td className="py-3 text-text-secondary capitalize">{tx.sourceType}</td>
                          <td className="py-3 font-medium text-text-primary truncate max-w-[280px]" title={tx.description}>
                            {tx.description}
                          </td>
                          <td className="py-3 text-text-secondary capitalize">{tx.category.replace('_', ' ')}</td>
                          <td className={`py-3 text-right pr-3 font-mono font-semibold ${
                            tx.type === 'inflow' ? 'text-green-400' : 'text-text-primary'
                          }`}>
                            {tx.type === 'inflow' ? '+' : '-'}{formatMoney(tx.amount)}
                          </td>
                          <td className="py-3 text-center">
                            <div className="flex justify-center items-center gap-1.5">
                              {tx.sourceType === 'invoice' ? (
                                <button
                                  onClick={() => navigate(`/invoices/${tx.originalItem.id}`)}
                                  className="p-1 text-text-secondary hover:text-accent rounded transition-colors"
                                  title="View Invoice Detail"
                                >
                                  <Eye className="w-3.5 h-3.5" />
                                </button>
                              ) : (
                                <button
                                  onClick={() => handleTxItemDelete(tx)}
                                  className="p-1 text-text-secondary hover:text-red-400 rounded transition-colors"
                                  title="Delete transaction record"
                                >
                                  <Trash2 className="w-3.5 h-3.5" />
                                </button>
                              )}
                            </div>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          </motion.div>
        )}

        {/* TAB 2: PENDING BILLS */}
        {activeTab === 'pending_bills' && (
          <motion.div variants={container} initial="hidden" animate="show" className="space-y-6">
            <div className="flex justify-between items-center">
              <div>
                <h2 className="text-sm font-semibold text-text-primary">Unpaid Pending Bills</h2>
                <p className="text-xs text-text-secondary mt-0.5">Bills scheduled to go out that have not been paid yet.</p>
              </div>
              <div className="text-right">
                <span className="text-[10px] text-text-tertiary uppercase font-bold tracking-wider">Total Pending Cost</span>
                <div className="text-xl font-bold font-mono text-yellow-500 mt-0.5">
                  {formatMoney(summary.pendingOutflow)}
                </div>
              </div>
            </div>

            <div className="glass-panel p-5">
              {bills.filter(b => b.status !== 'paid').length === 0 ? (
                <EmptyState
                  icon={CheckCircle2}
                  title="No Pending Bills"
                  description="All caught up! There are no unpaid bills logged in the system."
                  action={{ label: 'Log New Bill', onClick: () => setShowLogBillModal(true) }}
                />
              ) : (
                <div className="space-y-3">
                  {bills.filter(b => b.status !== 'paid').map((bill: any) => (
                    <div key={bill.id} className="flex justify-between items-center p-4 rounded-xl bg-surface-200 border border-rim/6 hover:border-rim/10 transition-colors">
                      <div className="min-w-0">
                        <div className="text-sm font-semibold text-text-primary truncate">{bill.vendor}</div>
                        <div className="text-xs text-text-tertiary mt-1 flex items-center gap-2">
                          <span className="capitalize">{bill.category}</span>
                          <span>·</span>
                          <span>Due {bill.due_date || 'No due date'}</span>
                          {bill.recurring === 1 && (
                            <>
                              <span>·</span>
                              <span className="px-1 py-0.5 rounded bg-purple-500/10 text-purple-400 font-mono text-[9px] uppercase">{bill.frequency}</span>
                            </>
                          )}
                        </div>
                        {bill.notes && (
                          <div className="text-[11px] text-text-secondary italic mt-1.5 truncate max-w-[400px]">
                            "{bill.notes}"
                          </div>
                        )}
                      </div>
                      
                      <div className="text-right flex-shrink-0 flex items-center gap-4 pl-4">
                        <div>
                          <div className="font-mono text-base font-bold text-text-primary">
                            {formatMoney(bill.amount)}
                          </div>
                          <span className="text-[10px] font-bold text-yellow-500 uppercase tracking-wide">Pending</span>
                        </div>
                        
                        <div className="flex items-center gap-1.5">
                          <button
                            onClick={() => handlePayBillClick(bill)}
                            className="btn-primary py-1 px-3 text-xs flex items-center gap-1 rounded-lg"
                          >
                            <Check className="w-3.5 h-3.5" /> Mark Paid
                          </button>
                          <button
                            onClick={() => handleDeleteBill(bill.id)}
                            className="p-2 text-text-secondary hover:text-red-400 hover:bg-surface-100 rounded-lg transition-colors"
                            title="Delete Bill"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </motion.div>
        )}

        {/* TAB 3: SUBSCRIPTIONS */}
        {activeTab === 'subscriptions' && (
          <motion.div variants={container} initial="hidden" animate="show" className="space-y-6">
            <div className="flex justify-between items-center">
              <div>
                <h2 className="text-sm font-semibold text-text-primary">Active Subscriptions</h2>
                <p className="text-xs text-text-secondary mt-0.5">Recurring software, utility, and family streaming services.</p>
              </div>
              <div className="flex items-center gap-4">
                <div className="text-right">
                  <span className="text-[10px] text-text-tertiary uppercase font-bold tracking-wider font-mono">Monthly Cost</span>
                  <div className="text-xl font-bold font-mono text-text-primary mt-0.5">
                    {formatMoney(subscriptions.filter(s => s.status === 'active').reduce((sum, s) => {
                      const amount = s.amount || 0
                      if (s.billing_cycle === 'yearly') return sum + (amount / 12)
                      if (s.billing_cycle === 'weekly') return sum + (amount * 4.33)
                      return sum + amount
                    }, 0))}
                  </div>
                </div>
                <button
                  onClick={() => setShowAddSubscriptionModal(true)}
                  className="btn-primary text-xs flex items-center gap-1 py-1.5 px-3 rounded-lg"
                >
                  <Plus className="w-3.5 h-3.5" /> Add Subscription
                </button>
              </div>
            </div>

            <div className="glass-panel p-5">
              {subscriptions.length === 0 ? (
                <EmptyState
                  icon={CreditCard}
                  title="No Subscriptions Logged"
                  description="Keep track of recurring streaming, utilities, and software bills automatically."
                  action={{ label: 'Add Subscription Now', onClick: () => setShowAddSubscriptionModal(true) }}
                />
              ) : (
                <div className="space-y-3">
                  {subscriptions.map((sub: any) => (
                    <div key={sub.id} className="flex justify-between items-center p-4 rounded-xl bg-surface-200 border border-rim/6 hover:border-rim/10 transition-colors">
                      <div className="min-w-0">
                        <div className="text-sm font-semibold text-text-primary truncate">{sub.name}</div>
                        <div className="text-xs text-text-tertiary mt-1 flex items-center gap-2">
                          <span className="capitalize">{sub.category}</span>
                          <span>·</span>
                          <span className="capitalize">Vendor: {sub.vendor}</span>
                          <span>·</span>
                          <span className="capitalize">Cycle: {sub.billing_cycle}</span>
                          {sub.next_billing_date && (
                            <>
                              <span>·</span>
                              <span>Next Billing: {sub.next_billing_date}</span>
                            </>
                          )}
                        </div>
                      </div>

                      <div className="text-right flex-shrink-0 flex items-center gap-4 pl-4">
                        <div>
                          <div className="font-mono text-base font-bold text-text-primary">
                            {formatMoney(sub.amount)}
                          </div>
                          <span className={`inline-block text-[9px] uppercase tracking-wide font-mono font-bold px-1.5 py-0.5 rounded ${
                            sub.status === 'active' ? 'bg-green-500/10 text-green-400' : 'bg-surface-300 text-text-tertiary'
                          }`}>
                            {sub.status}
                          </span>
                        </div>
                        
                        <div className="flex items-center gap-1">
                          <button
                            onClick={() => toggleSubscriptionStatus(sub)}
                            className="btn-secondary py-1 px-2.5 text-xs font-medium rounded-lg"
                          >
                            {sub.status === 'active' ? 'Pause' : 'Resume'}
                          </button>
                          <button
                            onClick={() => handleDeleteSubscription(sub.id)}
                            className="p-2 text-text-secondary hover:text-red-400 hover:bg-surface-100 rounded-lg transition-colors"
                            title="Delete Subscription"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </motion.div>
        )}

        {/* TAB 4: IMPORT INBOX (Gmail Scans) */}
        {activeTab === 'inbox' && (
          <motion.div variants={container} initial="hidden" animate="show" className="space-y-6">
            <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
              <div>
                <h2 className="text-sm font-semibold text-text-primary">Scanned Email Import Inbox</h2>
                <p className="text-xs text-text-secondary mt-0.5">
                  Analyze billing info scanned from email body text and PDF statements automatically.
                </p>
              </div>
              
              <div className="flex items-center gap-2">
                <button
                  onClick={handleSyncGmail}
                  disabled={syncing}
                  className="btn-secondary flex items-center gap-2 text-xs py-1.5 px-3 rounded-lg"
                >
                  <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
                  {syncing ? 'Syncing...' : 'Sync Gmail'}
                </button>
                <button onClick={() => setShowPasteModal(true)} className="btn-primary flex items-center gap-2 text-xs py-1.5 px-3 rounded-lg">
                  <Plus className="w-3.5 h-3.5" /> Paste Email
                </button>
              </div>
            </div>

            {/* Sub-tabs for Inbox */}
            <div className="flex border-b border-rim/[0.04] gap-4 mb-4">
              {[
                { id: 'needs_review', label: 'Needs Review', count: candidates.filter(c => c.review_status === 'needs_review').length },
                { id: 'ignored', label: 'Ignored', count: candidates.filter(c => c.review_status === 'ignored').length },
                { id: 'duplicate', label: 'Duplicates', count: candidates.filter(c => c.review_status === 'duplicate').length },
              ].map(subTab => (
                <button
                  key={subTab.id}
                  onClick={() => setInboxTab(subTab.id as InboxTab)}
                  className={`pb-2 text-xs font-semibold relative transition-colors ${
                    inboxTab === subTab.id ? 'text-accent border-b-2 border-accent' : 'text-text-secondary hover:text-text-primary'
                  }`}
                >
                  {subTab.label}
                  {subTab.count > 0 && (
                    <span className="ml-1 px-1.5 py-0.5 rounded-full text-[9px] bg-accent/10 text-accent font-mono font-bold">
                      {subTab.count}
                    </span>
                  )}
                </button>
              ))}
            </div>

            {/* Bulk Actions Bar */}
            {filteredCandidates.length > 0 && (
              <div className="flex items-center justify-between p-3 rounded-xl bg-surface-200 border border-rim/6 text-xs flex-wrap gap-3">
                <div className="flex items-center gap-3">
                  <input
                    type="checkbox"
                    checked={filteredCandidates.length > 0 && filteredCandidates.every(c => selectedInboxIds.includes(c.id))}
                    ref={el => {
                      if (el) {
                        const visibleIds = filteredCandidates.map(c => c.id)
                        const someSelected = visibleIds.some(id => selectedInboxIds.includes(id))
                        const allSelected = visibleIds.every(id => selectedInboxIds.includes(id))
                        el.indeterminate = someSelected && !allSelected
                      }
                    }}
                    onChange={toggleSelectAllInbox}
                    className="w-4 h-4 text-accent border-rim/6 rounded focus:ring-accent bg-surface-100"
                  />
                  <span className="font-semibold text-text-secondary">
                    {selectedInboxIds.length > 0
                      ? `${selectedInboxIds.length} of ${filteredCandidates.length} selected`
                      : 'Select All'}
                  </span>
                </div>

                {selectedInboxIds.length > 0 && (
                  <div className="flex items-center gap-2 flex-wrap text-[11px]">
                    {(inboxTab === 'needs_review' || inboxTab === 'ignored' || inboxTab === 'duplicate') && (
                      <button
                        onClick={handleBulkApprove}
                        className="btn-primary flex items-center gap-1 py-1.5 px-3 text-xs rounded-lg"
                      >
                        <Check className="w-3 h-3" /> Approve Selected
                      </button>
                    )}
                    {(inboxTab === 'needs_review' || inboxTab === 'duplicate') && (
                      <button
                        onClick={handleBulkIgnore}
                        className="btn-secondary flex items-center gap-1 py-1.5 px-3 text-xs rounded-lg"
                      >
                        <X className="w-3 h-3" /> Ignore Selected
                      </button>
                    )}
                    <button
                      onClick={handleBulkDelete}
                      className="btn-secondary flex items-center gap-1 py-1.5 px-3 text-xs text-red-400 border-red-500/20 hover:bg-red-500/10 rounded-lg"
                    >
                      <Trash2 className="w-3 h-3" /> Delete Selected
                    </button>
                    <button
                      onClick={() => setSelectedInboxIds([])}
                      className="text-text-tertiary hover:text-text-primary px-1 font-semibold"
                    >
                      Clear
                    </button>
                  </div>
                )}
              </div>
            )}

            {/* Candidates Log */}
            <div className="glass-panel p-5">
              {filteredCandidates.length === 0 ? (
                <EmptyState
                  icon={Mail}
                  title={gmailConnected === false ? "Gmail account not connected" : "Import Inbox Clear"}
                  description={
                    gmailConnected === false
                      ? 'Connect your Google account in Settings to scan emails automatically.'
                      : inboxTab === 'needs_review'
                        ? 'No new invoices, bills, or receipts are currently pending review.'
                        : 'No items in this sub-tab.'
                  }
                  action={
                    gmailConnected === false
                      ? { label: 'Go to Settings', onClick: () => navigate('/settings') }
                      : inboxTab === 'needs_review'
                        ? { label: 'Sync Gmail Now', onClick: handleSyncGmail }
                        : undefined
                  }
                />
              ) : (
                <div className="space-y-4">
                  {filteredCandidates.map(c => (
                    <div
                      key={c.id}
                      className={`p-4 rounded-xl border transition-all flex flex-col md:flex-row md:items-center justify-between gap-4 ${
                        selectedInboxIds.includes(c.id) ? 'bg-accent/[0.04] border-accent/30' : 'bg-surface-200 border-rim/6'
                      }`}
                    >
                      <div className="flex items-start gap-3 min-w-0">
                        <input
                          type="checkbox"
                          checked={selectedInboxIds.includes(c.id)}
                          onChange={() => toggleSelectInbox(c.id)}
                          className="w-4 h-4 mt-1 text-accent border-rim/6 rounded focus:ring-accent bg-surface-100 flex-shrink-0"
                        />
                        <div className="min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span className="text-sm font-semibold text-text-primary truncate">
                              {c.extracted_vendor || 'Unknown Vendor'}
                            </span>
                            <span className="px-1.5 py-0.5 rounded bg-surface-300 text-text-secondary font-mono text-[9px] uppercase tracking-wider">
                              {c.extracted_record_type}
                            </span>
                            {c.confidence_score !== undefined && (
                              <span className={`px-1.5 py-0.5 rounded font-mono text-[9px] font-semibold ${
                                c.confidence_score >= 0.8
                                  ? 'bg-green-500/10 text-green-400'
                                  : c.confidence_score >= 0.5
                                    ? 'bg-yellow-500/10 text-yellow-400'
                                    : 'bg-red-500/10 text-red-400'
                              }`}>
                                {Math.round(c.confidence_score * 100)}% Match
                              </span>
                            )}
                          </div>
                          
                          <div className="text-xs text-text-tertiary mt-1 flex flex-wrap items-center gap-2">
                            <span className="capitalize">{c.extracted_category || 'Other'}</span>
                            <span>·</span>
                            <span>
                              {c.extracted_record_type === 'bill'
                                ? `Due: ${c.extracted_due_date || 'N/A'}`
                                : `Date: ${c.extracted_payment_date || c.extracted_invoice_date || 'N/A'}`}
                            </span>
                            {c.extracted_frequency && c.extracted_frequency !== 'one_time' && (
                              <>
                                <span>·</span>
                                <span className="capitalize">{c.extracted_frequency}</span>
                              </>
                            )}
                          </div>
                          {c.raw_extraction_json && (
                            <div className="text-[10px] text-text-secondary italic mt-1.5 truncate max-w-[420px]">
                              Extracted subject: "{c.raw_extraction_json.match(/"subject":\s*"([^"]+)"/)?.[1] || 'Billing statement'}"
                            </div>
                          )}
                        </div>
                      </div>

                      <div className="text-right flex-shrink-0 flex items-center gap-4 pl-8 md:pl-0">
                        <div>
                          <div className="font-mono text-base font-bold text-text-primary">
                            {formatMoney(c.extracted_amount || 0)}
                          </div>
                          <span className="text-[10px] text-text-tertiary font-medium">CAD</span>
                        </div>

                        <div className="flex items-center gap-1">
                          {inboxTab === 'needs_review' && (
                            <>
                              <button
                                onClick={() => handleApproveCandidate(c)}
                                className="btn-primary py-1 px-3 text-xs flex items-center gap-0.5 rounded-lg"
                              >
                                <Check className="w-3.5 h-3.5" /> Approve
                              </button>
                              <button
                                onClick={() => handleCreateRuleClick(c)}
                                className="p-2 text-text-secondary hover:text-accent hover:bg-surface-100 rounded-lg transition-colors"
                                title="Create Auto-Approve Rule"
                              >
                                <Sliders className="w-3.5 h-3.5" />
                              </button>
                            </>
                          )}
                          
                          <button
                            onClick={() => handleEditCandidateClick(c)}
                            className="p-2 text-text-secondary hover:text-text-primary hover:bg-surface-100 rounded-lg transition-colors"
                            title="Edit Details"
                          >
                            <Pencil className="w-3.5 h-3.5" />
                          </button>
                          
                          {inboxTab === 'needs_review' ? (
                            <button
                              onClick={() => handleIgnoreCandidate(c.id)}
                              className="p-2 text-text-secondary hover:text-yellow-500 hover:bg-surface-100 rounded-lg transition-colors"
                              title="Ignore Candidate"
                            >
                              <X className="w-3.5 h-3.5" />
                            </button>
                          ) : (
                            <button
                              onClick={() => handleDeleteCandidate(c.id)}
                              className="p-2 text-text-secondary hover:text-red-400 hover:bg-surface-100 rounded-lg transition-colors"
                              title="Delete Permanently"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                            </button>
                          )}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </motion.div>
        )}
      </div>

      {/* --- MODALS --- */}
      
      {/* 1. Modal: Log Expense (Money Out) */}
      <Modal
        isOpen={showLogExpenseModal}
        onClose={() => setShowLogExpenseModal(false)}
        title="Log Outflow (Expense)"
      >
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Vendor Name</label>
            <input
              type="text"
              placeholder="e.g. BC Hydro, Netflix, Shell"
              value={expenseForm.vendor}
              onChange={e => setExpenseForm(prev => ({ ...prev, vendor: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Amount</label>
              <input
                type="number"
                placeholder="0.00"
                step="0.01"
                value={expenseForm.amount}
                onChange={e => setExpenseForm(prev => ({ ...prev, amount: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Currency</label>
              <select
                value={expenseForm.currency}
                onChange={e => setExpenseForm(prev => ({ ...prev, currency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="CAD">CAD ($)</option>
                <option value="USD">USD ($)</option>
                <option value="EUR">EUR (€)</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Date</label>
              <input
                type="date"
                value={expenseForm.date}
                onChange={e => setExpenseForm(prev => ({ ...prev, date: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Category</label>
              <select
                value={expenseForm.category}
                onChange={e => setExpenseForm(prev => ({ ...prev, category: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="office">Office Supplies</option>
                <option value="software">Software / SaaS</option>
                <option value="travel">Travel & Meals</option>
                <option value="utilities">Utilities & Internet</option>
                <option value="hardware">Hardware / Equipment</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>

          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Description / Notes</label>
            <input
              type="text"
              placeholder="Subtle details..."
              value={expenseForm.description}
              onChange={e => setExpenseForm(prev => ({ ...prev, description: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="flex items-center gap-2 pt-2">
            <input
              type="checkbox"
              id="is_deductible"
              checked={expenseForm.is_deductible === 1}
              onChange={e => setExpenseForm(prev => ({ ...prev, is_deductible: e.target.checked ? 1 : 0 }))}
              className="w-4 h-4 text-accent border-rim/6 rounded focus:ring-accent bg-surface-100"
            />
            <label htmlFor="is_deductible" className="text-xs text-text-secondary font-medium select-none">
              Tax Deductible Business Expense
            </label>
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowLogExpenseModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">
              Cancel
            </button>
            <button onClick={handleLogExpenseSubmit} className="btn-primary text-xs py-1.5 px-3 rounded-lg">
              Log Expense
            </button>
          </div>
        </div>
      </Modal>

      {/* 2. Modal: Log Payment Received (Money In) */}
      <Modal
        isOpen={showLogPaymentModal}
        onClose={() => setShowLogPaymentModal(false)}
        title="Log Inflow (Payment Received)"
      >
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Client Name</label>
            <input
              type="text"
              placeholder="e.g. Acme Corp, John Doe"
              value={paymentForm.vendor}
              onChange={e => setPaymentForm(prev => ({ ...prev, vendor: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Amount Received</label>
              <input
                type="number"
                placeholder="0.00"
                step="0.01"
                value={paymentForm.amount}
                onChange={e => setPaymentForm(prev => ({ ...prev, amount: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Currency</label>
              <select
                value={paymentForm.currency}
                onChange={e => setPaymentForm(prev => ({ ...prev, currency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="CAD">CAD ($)</option>
                <option value="USD">USD ($)</option>
                <option value="EUR">EUR (€)</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Payment Date</label>
              <input
                type="date"
                value={paymentForm.payment_date}
                onChange={e => setPaymentForm(prev => ({ ...prev, payment_date: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Payment Method</label>
              <select
                value={paymentForm.payment_method}
                onChange={e => setPaymentForm(prev => ({ ...prev, payment_method: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="e-transfer">Interac E-Transfer</option>
                <option value="wire">Wire Transfer</option>
                <option value="check">Check</option>
                <option value="credit">Credit Card</option>
                <option value="cash">Cash</option>
              </select>
            </div>
          </div>

          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Notes</label>
            <input
              type="text"
              placeholder="Invoice link reference, details..."
              value={paymentForm.notes}
              onChange={e => setPaymentForm(prev => ({ ...prev, notes: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <p className="text-[10px] text-text-tertiary">
            Note: Logging an inflow payment will auto-link to any outstanding matching invoice or create a paid invoice record.
          </p>

          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowLogPaymentModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">
              Cancel
            </button>
            <button onClick={handleLogPaymentSubmit} className="btn-primary text-xs py-1.5 px-3 rounded-lg">
              Log Income
            </button>
          </div>
        </div>
      </Modal>

      {/* 3. Modal: Log Bill (Pending) */}
      <Modal
        isOpen={showLogBillModal}
        onClose={() => setShowLogBillModal(false)}
        title="Log Pending Bill"
      >
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Vendor Name</label>
            <input
              type="text"
              placeholder="e.g. Telus, City of Vancouver, Amazon Web Services"
              value={billForm.vendor}
              onChange={e => setBillForm(prev => ({ ...prev, vendor: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Amount</label>
              <input
                type="number"
                placeholder="0.00"
                step="0.01"
                value={billForm.amount}
                onChange={e => setBillForm(prev => ({ ...prev, amount: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Currency</label>
              <select
                value={billForm.currency}
                onChange={e => setBillForm(prev => ({ ...prev, currency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="CAD">CAD ($)</option>
                <option value="USD">USD ($)</option>
                <option value="EUR">EUR (€)</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Due Date</label>
              <input
                type="date"
                value={billForm.due_date}
                onChange={e => setBillForm(prev => ({ ...prev, due_date: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Category</label>
              <select
                value={billForm.category}
                onChange={e => setBillForm(prev => ({ ...prev, category: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="utilities">Utilities & Internet</option>
                <option value="software">Software / SaaS</option>
                <option value="rent">Rent / Office space</option>
                <option value="insurance">Insurance</option>
                <option value="taxes">Taxes</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="flex items-center gap-2 pt-5">
              <input
                type="checkbox"
                id="bill_recurring"
                checked={billForm.recurring === 1}
                onChange={e => setBillForm(prev => ({ ...prev, recurring: e.target.checked ? 1 : 0 }))}
                className="w-4 h-4 text-accent border-rim/6 rounded focus:ring-accent bg-surface-100"
              />
              <label htmlFor="bill_recurring" className="text-xs text-text-secondary font-medium select-none">
                Recurring Bill
              </label>
            </div>
            {billForm.recurring === 1 && (
              <div>
                <label className="block text-xs font-semibold text-text-secondary mb-1">Frequency</label>
                <select
                  value={billForm.frequency}
                  onChange={e => setBillForm(prev => ({ ...prev, frequency: e.target.value }))}
                  className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
                >
                  <option value="weekly">Weekly</option>
                  <option value="monthly">Monthly</option>
                  <option value="quarterly">Quarterly</option>
                  <option value="yearly">Yearly</option>
                </select>
              </div>
            )}
          </div>

          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Notes</label>
            <input
              type="text"
              placeholder="Specific details..."
              value={billForm.notes}
              onChange={e => setBillForm(prev => ({ ...prev, notes: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowLogBillModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">
              Cancel
            </button>
            <button onClick={handleLogBillSubmit} className="btn-primary text-xs py-1.5 px-3 rounded-lg">
              Log Bill
            </button>
          </div>
        </div>
      </Modal>

      {/* 4. Modal: Pay Pending Bill */}
      <Modal
        isOpen={showPayBillModal}
        onClose={() => setShowPayBillModal(false)}
        title="Mark Bill as Paid"
      >
        {selectedBillToPay && (
          <div className="space-y-4 pt-2">
            <div className="p-3.5 rounded-xl bg-surface-200 border border-rim/6 text-xs">
              <div className="flex justify-between items-center font-semibold mb-1">
                <span className="text-text-primary">{selectedBillToPay.vendor}</span>
                <span className="font-mono text-accent">{formatMoney(selectedBillToPay.amount)} {selectedBillToPay.currency}</span>
              </div>
              <p className="text-text-tertiary">Category: {selectedBillToPay.category} · Due: {selectedBillToPay.due_date}</p>
            </div>

            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="block text-xs font-semibold text-text-secondary mb-1">Payment Date</label>
                <input
                  type="date"
                  value={payBillForm.payment_date}
                  onChange={e => setPayBillForm(prev => ({ ...prev, payment_date: e.target.value }))}
                  className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
                />
              </div>
              <div>
                <label className="block text-xs font-semibold text-text-secondary mb-1">Payment Method</label>
                <select
                  value={payBillForm.payment_method}
                  onChange={e => setPayBillForm(prev => ({ ...prev, payment_method: e.target.value }))}
                  className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
                >
                  <option value="e-transfer">Interac E-Transfer</option>
                  <option value="wire">Wire Transfer</option>
                  <option value="check">Check</option>
                  <option value="credit">Credit Card</option>
                  <option value="cash">Cash</option>
                </select>
              </div>
            </div>

            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Reference or Notes</label>
              <input
                type="text"
                placeholder="Confirmation number, details..."
                value={payBillForm.notes}
                onChange={e => setPayBillForm(prev => ({ ...prev, notes: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>

            <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
              <button onClick={() => setShowPayBillModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">
                Cancel
              </button>
              <button onClick={handlePayBillSubmit} className="btn-primary text-xs py-1.5 px-3 rounded-lg">
                Confirm Payment
              </button>
            </div>
          </div>
        )}
      </Modal>

      {/* 5. Modal: Add Subscription */}
      <Modal
        isOpen={showAddSubscriptionModal}
        onClose={() => setShowAddSubscriptionModal(false)}
        title="Add Active Subscription"
      >
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Service Name</label>
            <input
              type="text"
              placeholder="e.g. Adobe Creative Cloud, Spotify Family"
              value={subscriptionForm.name}
              onChange={e => setSubscriptionForm(prev => ({ ...prev, name: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Vendor (Billing Name)</label>
              <input
                type="text"
                placeholder="e.g. Adobe Inc."
                value={subscriptionForm.vendor}
                onChange={e => setSubscriptionForm(prev => ({ ...prev, vendor: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Monthly/Cycle Amount</label>
              <input
                type="number"
                placeholder="0.00"
                step="0.01"
                value={subscriptionForm.amount}
                onChange={e => setSubscriptionForm(prev => ({ ...prev, amount: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Billing Cycle</label>
              <select
                value={subscriptionForm.billing_cycle}
                onChange={e => setSubscriptionForm(prev => ({ ...prev, billing_cycle: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="weekly">Weekly</option>
                <option value="monthly">Monthly</option>
                <option value="quarterly">Quarterly</option>
                <option value="yearly">Yearly</option>
              </select>
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Currency</label>
              <select
                value={subscriptionForm.currency}
                onChange={e => setSubscriptionForm(prev => ({ ...prev, currency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="CAD">CAD ($)</option>
                <option value="USD">USD ($)</option>
                <option value="EUR">EUR (€)</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Next Billing Date</label>
              <input
                type="date"
                value={subscriptionForm.next_billing_date}
                onChange={e => setSubscriptionForm(prev => ({ ...prev, next_billing_date: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Category</label>
              <select
                value={subscriptionForm.category}
                onChange={e => setSubscriptionForm(prev => ({ ...prev, category: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="software">Software / SaaS</option>
                <option value="utilities">Utilities & Phone</option>
                <option value="entertainment">Entertainment / Media</option>
                <option value="membership">Memberships / Subscriptions</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowAddSubscriptionModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">
              Cancel
            </button>
            <button onClick={handleAddSubscriptionSubmit} className="btn-primary text-xs py-1.5 px-3 rounded-lg">
              Add Subscription
            </button>
          </div>
        </div>
      </Modal>

      {/* 6. Modal: Paste Email (Inbox) */}
      <Modal isOpen={showPasteModal} onClose={() => setShowPasteModal(false)} title="Paste Billing Email Text">
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Sender Email / Name (Optional)</label>
            <input
              type="text"
              placeholder="e.g. netflix@netflix.com"
              value={pasteForm.sender}
              onChange={e => setPasteForm(prev => ({ ...prev, sender: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Subject Line (Optional)</label>
            <input
              type="text"
              placeholder="e.g. Your subscription receipt"
              value={pasteForm.subject}
              onChange={e => setPasteForm(prev => ({ ...prev, subject: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Email Body Content</label>
            <textarea
              rows={6}
              placeholder="Paste the raw text of the invoice or billing email here..."
              value={pasteForm.text}
              onChange={e => setPasteForm(prev => ({ ...prev, text: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent font-mono"
            />
          </div>
          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowPasteModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">Cancel</button>
            <button onClick={handlePasteSubmit} className="btn-primary text-xs py-1.5 px-3 rounded-lg">Parse and Import</button>
          </div>
        </div>
      </Modal>

      {/* 7. Modal: Edit Inbox Candidate */}
      <Modal
        isOpen={showEditCandidateModal}
        onClose={() => setShowEditCandidateModal(false)}
        title="Edit Import Candidate Details"
      >
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Vendor Name</label>
            <input
              type="text"
              value={editCandidateForm.extracted_vendor}
              onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_vendor: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="grid grid-cols-3 gap-4">
            <div className="col-span-2">
              <label className="block text-xs font-semibold text-text-secondary mb-1">Amount</label>
              <input
                type="number"
                step="0.01"
                value={editCandidateForm.extracted_amount}
                onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_amount: parseFloat(e.target.value) || 0 }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Currency</label>
              <select
                value={editCandidateForm.extracted_currency}
                onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_currency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="CAD">CAD</option>
                <option value="USD">USD</option>
                <option value="EUR">EUR</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Record Type</label>
              <select
                value={editCandidateForm.extracted_record_type}
                onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_record_type: e.target.value as any }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="bill">Bill (Pending Outflow)</option>
                <option value="expense">Expense (Outflow)</option>
                <option value="subscription">Subscription (Recurring)</option>
                <option value="payment">Payment (Inflow)</option>
              </select>
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Category</label>
              <select
                value={editCandidateForm.extracted_category}
                onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_category: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="utilities">Utilities & Internet</option>
                <option value="software">Software / SaaS</option>
                <option value="office">Office supplies</option>
                <option value="travel">Travel & meals</option>
                <option value="membership">Memberships</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            {editCandidateForm.extracted_record_type === 'bill' ? (
              <div>
                <label className="block text-xs font-semibold text-text-secondary mb-1">Due Date</label>
                <input
                  type="date"
                  value={editCandidateForm.extracted_due_date || ''}
                  onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_due_date: e.target.value }))}
                  className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
                />
              </div>
            ) : (
              <div>
                <label className="block text-xs font-semibold text-text-secondary mb-1">Payment Date</label>
                <input
                  type="date"
                  value={editCandidateForm.extracted_payment_date || ''}
                  onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_payment_date: e.target.value }))}
                  className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
                />
              </div>
            )}
            
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Billing Frequency</label>
              <select
                value={editCandidateForm.extracted_frequency}
                onChange={e => setEditCandidateForm(prev => ({ ...prev, extracted_frequency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="one_time">One Time</option>
                <option value="weekly">Weekly</option>
                <option value="monthly">Monthly</option>
                <option value="quarterly">Quarterly</option>
                <option value="yearly">Yearly</option>
              </select>
            </div>
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowEditCandidateModal(false)} className="btn-secondary text-xs py-1.5 px-3 rounded-lg">
              Cancel
            </button>
            <button onClick={handleEditCandidateSubmit} className="btn-primary text-xs font-medium py-1.5 px-3 rounded-lg">
              Save Changes
            </button>
          </div>
        </div>
      </Modal>

      {/* 8. Modal: Create Automation Rule */}
      <Modal isOpen={showRuleModal} onClose={() => setShowRuleModal(false)} title="Create Automation Rule">
        <div className="space-y-4 pt-2">
          <div>
            <label className="block text-xs font-semibold text-text-secondary mb-1">Rule Name</label>
            <input
              type="text"
              placeholder="e.g. Adobe Subscription Auto-Import"
              value={ruleForm.rule_name}
              onChange={e => setRuleForm(prev => ({ ...prev, rule_name: e.target.value }))}
              className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">If Sender Contains</label>
              <input
                type="text"
                placeholder="e.g. adobe.com"
                value={ruleForm.sender_contains}
                onChange={e => setRuleForm(prev => ({ ...prev, sender_contains: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">If Subject Contains</label>
              <input
                type="text"
                placeholder="e.g. Invoice"
                value={ruleForm.subject_contains}
                onChange={e => setRuleForm(prev => ({ ...prev, subject_contains: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Assign Vendor</label>
              <input
                type="text"
                placeholder="e.g. Adobe"
                value={ruleForm.vendor}
                onChange={e => setRuleForm(prev => ({ ...prev, vendor: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Assign Category</label>
              <select
                value={ruleForm.category}
                onChange={e => setRuleForm(prev => ({ ...prev, category: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="software">Software / SaaS</option>
                <option value="utilities">Utilities & Internet</option>
                <option value="office">Office supplies</option>
                <option value="other">Other</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Record Type</label>
              <select
                value={ruleForm.record_type}
                onChange={e => setRuleForm(prev => ({ ...prev, record_type: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="bill">Bill (Pending Outflow)</option>
                <option value="expense">Expense (Outflow)</option>
                <option value="subscription">Subscription (Recurring)</option>
              </select>
            </div>
            <div>
              <label className="block text-xs font-semibold text-text-secondary mb-1">Recurring Frequency</label>
              <select
                value={ruleForm.recurring_frequency}
                onChange={e => setRuleForm(prev => ({ ...prev, recurring_frequency: e.target.value }))}
                className="w-full text-sm py-2 px-3 rounded-lg border border-rim/6 bg-surface-100 text-text-primary focus:outline-none focus:ring-1 focus:ring-accent"
              >
                <option value="one_time">One-time / Variable</option>
                <option value="weekly">Weekly</option>
                <option value="monthly">Monthly</option>
                <option value="quarterly">Quarterly</option>
                <option value="yearly">Yearly</option>
              </select>
            </div>
          </div>

          <div className="flex items-center gap-2 pt-2">
            <input
              type="checkbox"
              id="auto_approve"
              checked={ruleForm.auto_approve === 1}
              onChange={e => setRuleForm(prev => ({ ...prev, auto_approve: e.target.checked ? 1 : 0 }))}
              className="w-4 h-4 text-accent border-rim/6 rounded focus:ring-accent bg-surface-100"
            />
            <label htmlFor="auto_approve" className="text-xs text-text-secondary font-medium select-none">
              Auto-Approve future scanned bills matching this rule
            </label>
          </div>

          <div className="flex justify-end gap-2 pt-4 border-t border-rim/6">
            <button onClick={() => setShowRuleModal(false)} className="btn-secondary text-xs font-medium py-1.5 px-3 rounded-lg">Cancel</button>
            <button onClick={handleRuleSubmit} className="btn-primary text-xs font-medium py-1.5 px-3 rounded-lg">Create Rule</button>
          </div>
        </div>
      </Modal>
    </motion.div>
  )
}
