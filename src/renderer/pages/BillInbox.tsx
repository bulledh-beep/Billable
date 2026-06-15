import { useState, useEffect } from 'react'
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
  FileSpreadsheet,
} from 'lucide-react'
import Modal from '../components/Modal'
import EmptyState from '../components/EmptyState'
import toast from 'react-hot-toast'
import type { BillImportCandidate } from '@shared/types'

const container = { hidden: { opacity: 0 }, show: { opacity: 1, transition: { staggerChildren: 0.04 } } }
const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0 } }

type ActiveTab = 'needs_review' | 'bill' | 'receipt' | 'subscription' | 'payment' | 'ignored' | 'duplicate'

export default function BillInbox() {
  const navigate = useNavigate()
  const [candidates, setCandidates] = useState<BillImportCandidate[]>([])
  const [activeTab, setActiveTab] = useState<ActiveTab>('needs_review')
  const [loading, setLoading] = useState(false)
  const [syncing, setSyncing] = useState(false)
  const [gmailConnected, setGmailConnected] = useState<boolean | null>(null)

  // Modals state
  const [showPasteModal, setShowPasteModal] = useState(false)
  const [showEditModal, setShowEditModal] = useState(false)
  const [showRuleModal, setShowRuleModal] = useState(false)

  // Edit Candidate form state
  const [selectedCandidate, setSelectedCandidate] = useState<BillImportCandidate | null>(null)
  const [editForm, setEditForm] = useState({
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

  useEffect(() => {
    loadCandidates()
    checkGmailStatus()
  }, [])

  const checkGmailStatus = async () => {
    try {
      const status = await window.api.gmail.status()
      setGmailConnected(status.connected)
    } catch (error) {
      setGmailConnected(false)
    }
  }

  const loadCandidates = async () => {
    setLoading(true)
    try {
      const data = await window.api.candidates.list()
      setCandidates(data)
    } catch (err: any) {
      toast.error(`Error loading candidates: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

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
      loadCandidates()
    } catch (err: any) {
      const cleanMsg = err.message
        ? err.message.replace(/^Error invoking remote method '[^']+':\s*/, '')
        : 'Unknown error'
      toast.error(`Sync failed: ${cleanMsg}`, { id: 'gmail-sync' })
    } finally {
      setSyncing(false)
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
      loadCandidates()
    } catch (err: any) {
      toast.error(`Error parsing text: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

  const handleApprove = async (candidate: BillImportCandidate) => {
    toast.loading('Processing approval...', { id: 'approve-candidate' })
    try {
      // 1. Create matching record based on record type
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

      // 2. Set candidate status to approved
      await window.api.candidates.update(candidate.id, { review_status: 'approved' })
      toast.success('Approved and imported successfully!', { id: 'approve-candidate' })
      loadCandidates()
    } catch (err: any) {
      toast.error(`Approval failed: ${err.message}`, { id: 'approve-candidate' })
    }
  }

  const handleIgnore = async (id: number) => {
    try {
      await window.api.candidates.update(id, { review_status: 'ignored' })
      toast.success('Candidate ignored')
      loadCandidates()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  const handleEditClick = (candidate: BillImportCandidate) => {
    setSelectedCandidate(candidate)
    setEditForm({
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
    setShowEditModal(true)
  }

  const handleEditSubmit = async () => {
    if (!selectedCandidate) return
    try {
      await window.api.candidates.update(selectedCandidate.id, editForm)
      toast.success('Candidate updated')
      setShowEditModal(false)
      loadCandidates()
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

  const handleDeleteCandidate = async (id: number) => {
    if (!confirm('Are you sure you want to delete this candidate?')) return
    try {
      await window.api.candidates.delete(id)
      toast.success('Candidate deleted')
      loadCandidates()
    } catch (err: any) {
      toast.error(err.message)
    }
  }

  // Filter candidates based on activeTab
  const filteredCandidates = candidates.filter(c => {
    if (activeTab === 'needs_review') {
      return c.review_status === 'needs_review'
    } else if (activeTab === 'ignored') {
      return c.review_status === 'ignored'
    } else if (activeTab === 'duplicate') {
      return c.review_status === 'duplicate'
    } else {
      // bill, receipt, subscription, payment tabs show approved ones
      return c.review_status === 'approved' && c.extracted_record_type === activeTab
    }
  })

  return (
    <motion.div variants={container} initial="hidden" animate="show" className="p-8">
      <motion.div variants={item} className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Bill Inbox</h1>
          <p className="text-sm text-text-secondary mt-1">
            Incoming bills, receipts, and subscriptions detected from Gmail
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleSyncGmail}
            disabled={syncing}
            className="btn-secondary flex items-center gap-2 text-xs"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${syncing ? 'animate-spin' : ''}`} />
            {syncing ? 'Syncing...' : 'Sync Gmail'}
          </button>
          <button onClick={() => setShowPasteModal(true)} className="btn-primary flex items-center gap-2 text-xs">
            <Plus className="w-3.5 h-3.5" /> Paste Email
          </button>
        </div>
      </motion.div>

      {/* Tabs */}
      <motion.div variants={item} className="border-b border-rim/[0.04] mb-6 flex overflow-x-auto gap-4">
        {[
          { id: 'needs_review', label: 'Needs Review', count: candidates.filter(c => c.review_status === 'needs_review').length },
          { id: 'bill', label: 'Approved Bills', count: candidates.filter(c => c.review_status === 'approved' && c.extracted_record_type === 'bill').length },
          { id: 'receipt', label: 'Approved Receipts', count: candidates.filter(c => c.review_status === 'approved' && c.extracted_record_type === 'receipt').length },
          { id: 'subscription', label: 'Subscriptions', count: candidates.filter(c => c.review_status === 'approved' && c.extracted_record_type === 'subscription').length },
          { id: 'payment', label: 'Approved Payments', count: candidates.filter(c => c.review_status === 'approved' && c.extracted_record_type === 'payment').length },
          { id: 'ignored', label: 'Ignored', count: candidates.filter(c => c.review_status === 'ignored').length },
          { id: 'duplicate', label: 'Duplicates', count: candidates.filter(c => c.review_status === 'duplicate').length },
        ].map(tab => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id as ActiveTab)}
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

      {loading && filteredCandidates.length === 0 ? (
        <div className="flex justify-center py-20">
          <RefreshCw className="w-8 h-8 text-accent animate-spin" />
        </div>
      ) : filteredCandidates.length === 0 ? (
        <EmptyState
          icon={Mail}
          title={gmailConnected === false ? "Gmail account not connected" : "No candidates found"}
          description={
            gmailConnected === false
              ? 'Connect your Google account in Settings to automatically scan for bills, receipts, and e-transfers.'
              : activeTab === 'needs_review'
                ? 'Your inbox is clear! Click "Sync Gmail" to check for new billing emails.'
                : 'No items in this filter.'
          }
          action={
            gmailConnected === false
              ? { label: 'Go to Settings', onClick: () => navigate('/settings') }
              : activeTab === 'needs_review'
                ? { label: 'Sync Gmail Now', onClick: handleSyncGmail }
                : undefined
          }
        />
      ) : (
        <motion.div variants={container} initial="hidden" animate="show" className="space-y-4">
          <AnimatePresence mode="popLayout">
            {filteredCandidates.map(c => (
              <motion.div
                key={c.id}
                layoutId={`candidate-${c.id}`}
                variants={item}
                exit={{ opacity: 0, scale: 0.95 }}
                className="glass-panel p-5 relative border border-rim/[0.04]"
              >
                {/* Duplicate Badge */}
                {c.review_status === 'duplicate' && (
                  <div className="absolute top-4 right-4 flex items-center gap-1.5 px-2.5 py-1 rounded bg-yellow-500/10 border border-yellow-500/20 text-yellow-400 text-[10px] font-semibold">
                    <AlertTriangle className="w-3.5 h-3.5" />
                    Duplicate Warning
                  </div>
                )}

                <div className="flex flex-col lg:flex-row lg:items-start justify-between gap-4">
                  {/* Left info column */}
                  <div className="space-y-3 flex-1">
                    <div className="flex items-center flex-wrap gap-2">
                      <span className="text-base font-bold text-text-primary">{c.extracted_vendor || 'Unknown Vendor'}</span>
                      <span className="px-2 py-0.5 text-[10px] uppercase font-bold tracking-wider rounded-full bg-surface-200 border border-rim/6 text-text-secondary">
                        {c.extracted_record_type}
                      </span>
                      <span className="px-2 py-0.5 text-[10px] rounded-md bg-accent/5 text-accent font-medium capitalize">
                        {c.extracted_category}
                      </span>
                    </div>

                    <div className="text-xs text-text-secondary flex flex-wrap gap-x-4 gap-y-1">
                      <div>
                        Amount:{' '}
                        <strong className="text-text-primary">
                          {c.extracted_amount ? `${c.extracted_amount.toFixed(2)} ${c.extracted_currency}` : 'Unparsed'}
                        </strong>
                      </div>
                      {c.extracted_due_date && (
                        <div>
                          Due Date: <strong className="text-text-primary">{c.extracted_due_date}</strong>
                        </div>
                      )}
                      {c.extracted_payment_date && (
                        <div>
                          Payment Date: <strong className="text-text-primary">{c.extracted_payment_date}</strong>
                        </div>
                      )}
                      <div>
                        Confidence:{' '}
                        <span className={`font-semibold ${c.confidence_score > 0.8 ? 'text-green-400' : 'text-yellow-400'}`}>
                          {Math.round(c.confidence_score * 100)}%
                        </span>
                      </div>
                    </div>

                    {/* Email Context details */}
                    <div className="p-3 rounded-lg bg-surface-200/50 border border-rim/[0.03] space-y-1">
                      <div className="text-[11px] text-text-secondary truncate">
                        Subject: <span className="text-text-primary font-medium">{c.extracted_vendor || 'Email'} - {c.extracted_due_date || 'No Date'}</span>
                      </div>
                      <div className="text-[11px] text-text-secondary flex gap-4">
                        <span>From: {c.extracted_vendor || 'Unknown'}</span>
                      </div>
                    </div>
                  </div>

                  {/* Actions column */}
                  <div className="flex items-center gap-2 flex-wrap self-end lg:self-start">
                    {c.review_status === 'needs_review' && (
                      <>
                        <button
                          onClick={() => handleApprove(c)}
                          className="btn-primary flex items-center gap-1.5 text-xs py-1.5 px-3"
                        >
                          <Check className="w-3.5 h-3.5" /> Approve
                        </button>
                        <button
                          onClick={() => handleEditClick(c)}
                          className="btn-secondary flex items-center gap-1.5 text-xs py-1.5 px-3"
                        >
                          <Pencil className="w-3.5 h-3.5" /> Edit
                        </button>
                        <button
                          onClick={() => handleCreateRuleClick(c)}
                          className="btn-secondary flex items-center gap-1.5 text-xs py-1.5 px-3"
                          title="Create automation rule"
                        >
                          <Sliders className="w-3.5 h-3.5" /> Rule
                        </button>
                        <button
                          onClick={() => handleIgnore(c.id)}
                          className="p-2 hover:bg-surface-300 rounded text-text-tertiary hover:text-text-primary transition-colors"
                          title="Ignore candidate"
                        >
                          <X className="w-4 h-4" />
                        </button>
                      </>
                    )}
                    <button
                      onClick={() => handleDeleteCandidate(c.id)}
                      className="p-2 hover:bg-red-500/10 rounded text-text-tertiary hover:text-red-400 transition-colors"
                      title="Delete candidate history"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </motion.div>
            ))}
          </AnimatePresence>
        </motion.div>
      )}

      {/* Modal 1: Paste Email Text */}
      <Modal isOpen={showPasteModal} onClose={() => setShowPasteModal(false)} title="Paste Email Text" size="lg">
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">Sender / Vendor Name</label>
              <input
                className="input-field"
                value={pasteForm.sender}
                onChange={e => setPasteForm({ ...pasteForm, sender: e.target.value })}
                placeholder="e.g. BC Hydro"
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">Email Subject</label>
              <input
                className="input-field"
                value={pasteForm.subject}
                onChange={e => setPasteForm({ ...pasteForm, subject: e.target.value })}
                placeholder="e.g. Your invoice for the period..."
              />
            </div>
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1 block">Email Content (Raw text)</label>
            <textarea
              className="input-field font-mono text-xs"
              rows={10}
              value={pasteForm.text}
              onChange={e => setPasteForm({ ...pasteForm, text: e.target.value })}
              placeholder="Paste the raw text of the bill or receipt email here..."
            />
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <button onClick={() => setShowPasteModal(false)} className="btn-secondary text-xs">
              Cancel
            </button>
            <button onClick={handlePasteSubmit} className="btn-primary text-xs flex items-center gap-1">
              Parse and Stage
            </button>
          </div>
        </div>
      </Modal>

      {/* Modal 2: Edit Candidate Details */}
      <Modal isOpen={showEditModal} onClose={() => setShowEditModal(false)} title="Edit Candidate Details">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1 block">Vendor Name</label>
            <input
              className="input-field"
              value={editForm.extracted_vendor}
              onChange={e => setEditForm({ ...editForm, extracted_vendor: e.target.value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">Amount</label>
              <input
                className="input-field"
                type="number"
                value={editForm.extracted_amount || ''}
                onChange={e => setEditForm({ ...editForm, extracted_amount: parseFloat(e.target.value) || 0 })}
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Currency</label>
              <select
                className="input-field"
                value={editForm.extracted_currency}
                onChange={e => setEditForm({ ...editForm, extracted_currency: e.target.value })}
              >
                <option value="CAD">CAD ($)</option>
                <option value="USD">USD ($)</option>
              </select>
            </div>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Record Type</label>
              <select
                className="input-field"
                value={editForm.extracted_record_type}
                onChange={e => setEditForm({ ...editForm, extracted_record_type: e.target.value as any })}
              >
                <option value="bill">Bill</option>
                <option value="expense">Expense</option>
                <option value="receipt">Receipt</option>
                <option value="subscription">Subscription</option>
                <option value="payment">Payment</option>
              </select>
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1.5 block">Category</label>
              <select
                className="input-field"
                value={editForm.extracted_category}
                onChange={e => setEditForm({ ...editForm, extracted_category: e.target.value })}
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
          <div className="grid grid-cols-3 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">Due Date</label>
              <input
                className="input-field"
                type="date"
                value={editForm.extracted_due_date}
                onChange={e => setEditForm({ ...editForm, extracted_due_date: e.target.value })}
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">Payment Date</label>
              <input
                className="input-field"
                type="date"
                value={editForm.extracted_payment_date}
                onChange={e => setEditForm({ ...editForm, extracted_payment_date: e.target.value })}
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">Invoice Date</label>
              <input
                className="input-field"
                type="date"
                value={editForm.extracted_invoice_date}
                onChange={e => setEditForm({ ...editForm, extracted_invoice_date: e.target.value })}
              />
            </div>
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <button onClick={() => setShowEditModal(false)} className="btn-secondary text-xs">
              Cancel
            </button>
            <button onClick={handleEditSubmit} className="btn-primary text-xs">
              Save changes
            </button>
          </div>
        </div>
      </Modal>

      {/* Modal 3: Create Automation Rule */}
      <Modal isOpen={showRuleModal} onClose={() => setShowRuleModal(false)} title="Create Automation Rule">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1 block">Rule Name</label>
            <input
              className="input-field"
              value={ruleForm.rule_name}
              onChange={e => setRuleForm({ ...ruleForm, rule_name: e.target.value })}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">If Sender contains</label>
              <input
                className="input-field"
                value={ruleForm.sender_contains}
                onChange={e => setRuleForm({ ...ruleForm, sender_contains: e.target.value })}
                placeholder="e.g. hydro"
              />
            </div>
            <div>
              <label className="text-xs font-medium text-text-secondary mb-1 block">If Subject contains</label>
              <input
                className="input-field"
                value={ruleForm.subject_contains}
                onChange={e => setRuleForm({ ...ruleForm, subject_contains: e.target.value })}
                placeholder="e.g. statement"
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
    </motion.div>
  )
}
