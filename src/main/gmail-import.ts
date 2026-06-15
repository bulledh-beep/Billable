import { getValidAccessToken } from './gmail-oauth'
import * as db from './database'
import { scoreEmail, relevanceBand } from './emailFilter'

/** Extract the bare domain from a "Name <a@b.com>" sender header. */
function extractDomain(sender: string): string {
  const m = sender.match(/@([a-z0-9.-]+)/i)
  return m ? m[1].toLowerCase() : ''
}

/**
 * Build a set of domains the user has already approved — i.e. domains that
 * appear on existing bills/subscriptions/expenses that came from email. These
 * get a relevance boost (and can clear the auto-approve bar more easily).
 * Computed once per sync, not per email.
 */
function buildKnownDomains(): Set<string> {
  const domains = new Set<string>()
  const addFrom = (rows: any[], field: string) => {
    for (const r of rows) {
      const v = (r[field] || '').toString()
      const d = extractDomain(v)
      if (d) domains.add(d)
    }
  }
  try {
    // Approved email imports carry the real sender domain
    const imports = db.listEmailImports() as any[]
    for (const imp of imports) {
      if (imp.status === 'imported') {
        const d = extractDomain(imp.sender || '')
        if (d) domains.add(d)
      }
    }
    addFrom(db.listAutomationRules() as any[], 'sender_contains')
  } catch {
    // best-effort — scoring still works without it
  }
  return domains
}

/** Read the auto-approve config from settings (enabled + threshold 0-100). */
function getAutoApproveConfig(): { enabled: boolean; threshold: number } {
  const s = db.getSettings() as any
  const enabled = (s.bill_auto_approve_enabled ?? '1') !== '0'
  const threshold = Math.max(0, Math.min(100, parseInt(s.bill_auto_approve_threshold ?? '85', 10) || 85))
  return { enabled, threshold }
}

/**
 * Decode Base64Url string to standard UTF-8 string
 */
function decodeBase64Url(str: string): string {
  const base64 = str.replace(/-/g, '+').replace(/_/g, '/')
  return Buffer.from(base64, 'base64').toString('utf8')
}

/**
 * Recursively extract the plain text or HTML body from a Gmail message payload
 */
function getMessageBody(payload: any): string {
  let body = ''
  if (payload.body && payload.body.data) {
    body = decodeBase64Url(payload.body.data)
  }
  if (payload.parts) {
    body += getPartsBody(payload.parts)
  }
  return body
}

function getPartsBody(parts: any[]): string {
  let text = ''
  let html = ''
  for (const part of parts) {
    if (part.mimeType === 'text/plain' && part.body && part.body.data) {
      text += decodeBase64Url(part.body.data)
    } else if (part.mimeType === 'text/html' && part.body && part.body.data) {
      html += decodeBase64Url(part.body.data)
    } else if (part.parts) {
      text += getPartsBody(part.parts)
    }
  }
  if (text) return text
  return html.replace(/<[^>]*>?/gm, ' ') // fallback to stripped HTML
}

/**
 * Clean up sender info to get a readable vendor name
 */
function cleanVendorName(sender: string): string {
  // Sender format: "Vendor Name <email@domain.com>" or just "email@domain.com"
  const bracketIndex = sender.indexOf('<')
  let name = ''
  if (bracketIndex !== -1) {
    name = sender.slice(0, bracketIndex).trim()
  } else {
    name = sender.trim()
  }

  // Strip enclosing quotes if any
  name = name.replace(/^["']|["']$/g, '').trim()

  if (!name || name.includes('@')) {
    // If name is an email, extract domain mailbox name
    const email = name.includes('@') ? name : sender
    const mailbox = email.split('@')[0]
    name = mailbox.charAt(0).toUpperCase() + mailbox.slice(1)
  }

  // Normalize common names
  const lower = name.toLowerCase()
  if (lower.includes('bchydro') || lower.includes('bc hydro')) return 'BC Hydro'
  if (lower.includes('netflix')) return 'Netflix'
  if (lower.includes('adobe')) return 'Adobe'
  if (lower.includes('google')) return 'Google'
  if (lower.includes('apple')) return 'Apple'
  if (lower.includes('spotify')) return 'Spotify'
  if (lower.includes('amazon')) return 'Amazon'

  return name
}

/**
 * Parse date from raw text to standard YYYY-MM-DD
 */
function parseDateString(text: string): string | null {
  if (!text) return null

  // Match YYYY-MM-DD
  const ymd = text.match(/\b(\d{4})[-/](\d{1,2})[-/](\d{1,2})\b/)
  if (ymd) {
    const y = ymd[1]
    const m = ymd[2].padStart(2, '0')
    const d = ymd[3].padStart(2, '0')
    return `${y}-${m}-${d}`
  }

  // Month Names Map
  const monthMap: Record<string, string> = {
    january: '01', february: '02', march: '03', april: '04', may: '05', june: '06',
    july: '07', august: '08', september: '09', october: '10', november: '11', december: '12',
    jan: '01', feb: '02', mar: '03', apr: '04', jun: '06', jul: '07', aug: '08', sep: '09', oct: '10', nov: '11', dec: '12'
  }

  const monthRegex = new RegExp(`\\b(${Object.keys(monthMap).join('|')})\\b`, 'i')
  const hasMonth = text.match(monthRegex)
  if (hasMonth) {
    const monthName = hasMonth[1].toLowerCase()
    const monthVal = monthMap[monthName]

    // Find YYYY (e.g. 2024 to 2035)
    const yearMatch = text.match(/\b(202\d|203\d)\b/)
    const year = yearMatch ? yearMatch[1] : new Date().getFullYear().toString()

    // Find DD day
    const stripped = text.replace(year, '').replace(hasMonth[1], '')
    const dayMatch = stripped.match(/\b(\d{1,2})\b/)
    const day = dayMatch ? dayMatch[1].padStart(2, '0') : '01'

    return `${year}-${monthVal}-${day}`
  }

  return null
}

/**
 * Extract currency amounts and evaluate confidence based on context keywords
 */
function parseAmount(text: string): { amount: number | null; confidence: number } {
  const decimalRegex = /\b([0-9]{1,6}(?:,[0-9]{3})*(?:\.[0-9]{2}))\b/g
  const matches: Array<{ val: number; index: number }> = []
  let match

  while ((match = decimalRegex.exec(text)) !== null) {
    const cleaned = match[1].replace(/,/g, '')
    const val = parseFloat(cleaned)
    if (!isNaN(val) && val > 0) {
      matches.push({ val, index: match.index })
    }
  }

  if (matches.length === 0) {
    return { amount: null, confidence: 0 }
  }

  const keywords = [/total/i, /amount due/i, /amount paid/i, /charged/i, /payment due/i, /price/i, /subtotal/i, /payment/i]
  let bestVal: number | null = null
  let maxScore = -1

  for (const m of matches) {
    let score = 0
    const start = Math.max(0, m.index - 80)
    const context = text.slice(start, m.index).toLowerCase()

    for (const kw of keywords) {
      if (kw.test(context)) {
        score += 10
        const pos = context.search(kw)
        const dist = 80 - pos
        score += (80 - dist) / 10
      }
    }

    if (m.val > 5000) score -= 15 // penalize very high values for typical bills

    if (score > maxScore) {
      maxScore = score
      bestVal = m.val
    }
  }

  if (maxScore <= 0) {
    const sensible = matches.filter(m => m.val < 1000)
    bestVal = sensible.length > 0 ? sensible[0].val : matches[0].val
  }

  return { amount: bestVal, confidence: maxScore > 0 ? 0.85 : 0.4 }
}

/**
 * Match email headers and body against custom Automation Rules
 */
function matchAutomationRules(sender: string, subject: string): any {
  const rules = db.listAutomationRules() as any[]
  for (const rule of rules) {
    if (!rule.is_active) continue

    const senderMatch = rule.sender_contains
      ? sender.toLowerCase().includes(rule.sender_contains.toLowerCase())
      : true

    const subjectMatch = rule.subject_contains
      ? subject.toLowerCase().includes(rule.subject_contains.toLowerCase())
      : true

    if (senderMatch && subjectMatch && (rule.sender_contains || rule.subject_contains)) {
      return rule
    }
  }
  return null
}

/**
 * Check if the candidate matches any existing bills or expenses (duplicate check)
 */
export function findDuplicateRecord(candidate: {
  vendor: string
  amount: number | null
  date: string | null
}): number | null {
  if (!candidate.amount || !candidate.date) return null

  const targetDate = new Date(candidate.date)
  const marginDays = 7

  // Check bills
  const bills = db.listBills() as any[]
  for (const b of bills) {
    if (b.vendor.toLowerCase() === candidate.vendor.toLowerCase() && Math.abs(b.amount - candidate.amount) < 0.01) {
      if (b.due_date) {
        const diff = Math.abs(new Date(b.due_date).getTime() - targetDate.getTime()) / (1000 * 3600 * 24)
        if (diff <= marginDays) return b.id
      }
    }
  }

  // Check expenses
  const expenses = db.listExpenses() as any[]
  for (const e of expenses) {
    const eVendor = e.vendor || e.description || ''
    if (eVendor.toLowerCase() === candidate.vendor.toLowerCase() && Math.abs(e.amount - candidate.amount) < 0.01) {
      if (e.date) {
        const diff = Math.abs(new Date(e.date).getTime() - targetDate.getTime()) / (1000 * 3600 * 24)
        if (diff <= marginDays) return e.id
      }
    }
  }

  return null
}

/**
 * Local non-AI extraction logic for a single imported email
 */
export function extractCandidateFromEmail(
  emailImportId: number,
  sender: string,
  subject: string,
  bodyText: string,
  opts?: { knownDomains?: Set<string>; attachmentNames?: string[] },
): any {
  const normalizedText = bodyText.replace(/\s+/g, ' ')
  const rule = matchAutomationRules(sender, subject)

  // Match Interac e-transfer or other bank deposit notifications
  const interacMatch = subject.match(/INTERAC e-Transfer:\s*(.*?)\s*(?:sent you money|has sent you a deposit|sent you a payment|has sent you a)/i)
  let vendor = rule?.vendor
  let recordType = rule?.record_type || 'bill'

  // 'payment' here means money RECEIVED (income). Inbound phrases only — an
  // e-Transfer YOU sent is money out (an expense), not income.
  const inboundRe = /(sent you money|sent you a payment|sent you an? e-?transfer|has sent you|direct deposit|deposited to your account|you'?ve received|payment received|e-?transfer received|funds received)/i
  const outboundRe = /(you sent|your e-?transfer to|payment to|sent an? e-?transfer to|transfer to .* (was|is) (sent|completed))/i

  if (interacMatch) {
    vendor = interacMatch[1].trim()
    recordType = 'payment' // received money
  } else {
    vendor = vendor || cleanVendorName(sender)
    if (!rule) {
      const sLower = subject.toLowerCase()
      const bLower = normalizedText.toLowerCase()
      const combined = `${sLower} ${bLower}`
      if (inboundRe.test(combined) && !outboundRe.test(combined)) {
        recordType = 'payment' // money in
      } else if (outboundRe.test(combined) || bLower.includes('e-transfer')) {
        recordType = 'expense' // money out (sent transfer / purchase)
      } else if (sLower.includes('receipt') || sLower.includes('thank you for your payment') || bLower.includes('amount paid')) {
        recordType = 'receipt'
      } else if (sLower.includes('subscription') || sLower.includes('renewal') || sLower.includes('membership') || bLower.includes('recurring charge')) {
        recordType = 'subscription'
      }
    }
  }

  // Category
  let category = rule?.category || 'other'
  if (!rule) {
    const vLower = vendor.toLowerCase()
    if (vLower.includes('hydro') || vLower.includes('electricity') || vLower.includes('power')) category = 'utilities'
    else if (vLower.includes('adobe') || vLower.includes('github') || vLower.includes('zoom') || vLower.includes('slack') || vLower.includes('google')) category = 'software'
    else if (vLower.includes('rogers') || vLower.includes('bell') || vLower.includes('telus') || vLower.includes('internet') || vLower.includes('fido')) category = 'phone_internet'
    else if (vLower.includes('uber') || vLower.includes('taxi') || vLower.includes('flight') || vLower.includes('hotel')) category = 'travel'
    else if (vLower.includes('restaurant') || vLower.includes('meals') || vLower.includes('cafe')) category = 'meals'
  }

  // Amount
  const amtResult = parseAmount(normalizedText)
  const amount = amtResult.amount

  // Currency
  let currency = 'CAD'
  if (normalizedText.includes('USD') || normalizedText.includes('US $')) {
    currency = 'USD'
  }

  // Date Parsing
  let dueDate: string | null = null
  let invoiceDate: string | null = null
  let paymentDate: string | null = null

  // Search context near keywords for dates
  const dueKeywords = ['due date', 'due by', 'payment due']
  const invoiceKeywords = ['invoice date', 'statement date', 'date issued']
  const paymentKeywords = ['payment date', 'paid on', 'charged on']

  const textLower = normalizedText.toLowerCase()

  const findDateNear = (keywords: string[]): string | null => {
    for (const kw of keywords) {
      const idx = textLower.indexOf(kw)
      if (idx !== -1) {
        // Extract 40 characters around/after the keyword to look for dates
        const excerpt = normalizedText.slice(idx + kw.length, idx + kw.length + 50)
        const parsed = parseDateString(excerpt)
        if (parsed) return parsed
      }
    }
    return null
  }

  dueDate = findDateNear(dueKeywords)
  invoiceDate = findDateNear(invoiceKeywords)
  paymentDate = findDateNear(paymentKeywords)

  // Fallbacks
  const fallbackDate = parseDateString(normalizedText) || new Date().toISOString().slice(0, 10)
  if (recordType === 'receipt' && !paymentDate) {
    paymentDate = fallbackDate
  } else if (!dueDate && !paymentDate) {
    dueDate = fallbackDate
  }

  // ---- Relevance + confidence scoring (the real signal, not "number near a word") ----
  const score = scoreEmail({
    sender,
    subject,
    body: bodyText,
    attachmentNames: opts?.attachmentNames,
    knownDomains: opts?.knownDomains,
    ruleMatched: !!rule,
  })
  const relevance = score.relevance_score
  const band = relevanceBand(relevance)

  // Confidence (0-1) blends the scorer's field-corroboration with the legacy
  // amount-context confidence, and a rule match is a strong vote.
  let confidence = score.confidence_score / 100
  if (rule) confidence = Math.max(confidence, 0.9)
  if (amount && (dueDate || paymentDate)) confidence = Math.min(1, confidence + 0.05)
  confidence = Math.min(1, confidence)

  // Light extra extraction for review context
  const last4Match = normalizedText.match(/(?:account|acct|card|ending(?:\s+in)?)\s*(?:#|no\.?|number)?\s*(?:\*+|x+)?\s*(\d{4})\b/i)
  const accountLast4 = last4Match ? last4Match[1] : null
  const pmMatch = normalizedText.match(/\b(visa|mastercard|amex|american express|paypal|interac|e-?transfer|debit|credit card)\b/i)
  const paymentMethod = pmMatch ? pmMatch[1] : null

  // Duplicate Check
  const duplicateId = findDuplicateRecord({ vendor, amount, date: paymentDate || dueDate })

  // ---- Strict auto-approve gate ----
  // A rule that explicitly says auto_approve always wins (the user opted in).
  // Otherwise, EVERY guard must pass: feature on, relevance AND confidence both
  // clear the threshold, all key fields present, and no duplicate. High
  // confidence alone is NOT enough — that was the old footgun.
  const { enabled: autoEnabled, threshold } = getAutoApproveConfig()
  const confidencePct = Math.round(confidence * 100)
  const fieldsComplete = !!vendor && amount !== null && amount > 0 && !!(dueDate || paymentDate)
  const ruleAutoApprove = rule?.auto_approve === 1 && amount !== null && !duplicateId
  const scoreAutoApprove =
    autoEnabled &&
    relevance >= threshold &&
    confidencePct >= threshold &&
    fieldsComplete &&
    !duplicateId
  const isAutoApproved = ruleAutoApprove || scoreAutoApprove

  // Below the "ignore" band, don't even surface it as needs-review.
  const reviewStatus = duplicateId
    ? 'duplicate'
    : isAutoApproved
      ? 'approved'
      : band === 'ignore'
        ? 'ignored'
        : 'needs_review'

  const suggestedAction = duplicateId
    ? 'Possible duplicate — compare'
    : isAutoApproved
      ? (ruleAutoApprove ? 'Auto-added by rule' : 'Auto-added (high confidence)')
      : band === 'ignore'
        ? 'Ignored (low relevance)'
        : recordType === 'payment'
          ? 'Review — try matching to a bill'
          : 'Review & approve'

  // Insert into candidates database
  const candidate = db.createCandidate({
    workspace_id: 1,
    user_id: 1,
    email_import_id: emailImportId,
    extracted_vendor: vendor,
    extracted_amount: amount,
    extracted_currency: currency,
    extracted_due_date: dueDate,
    extracted_invoice_date: invoiceDate,
    extracted_payment_date: paymentDate,
    extracted_status: recordType === 'receipt' ? 'paid' : 'needs_review',
    extracted_category: category,
    extracted_invoice_number: null,
    extracted_frequency: rule?.recurring_frequency || 'one_time',
    extracted_record_type: recordType,
    confidence_score: confidence,
    relevance_score: relevance,
    detected_reason: score.detected_reason,
    low_confidence_reason: score.low_confidence_reason,
    suggested_action: suggestedAction,
    extracted_account_last4: accountLast4,
    extracted_payment_method: paymentMethod,
    auto_approved: isAutoApproved ? 1 : 0,
    duplicate_of_id: duplicateId,
    raw_extraction_json: JSON.stringify({
      parsedVendor: vendor,
      parsedAmount: amount,
      matchedRule: rule?.rule_name || null,
      relevance,
      confidence: confidencePct,
      positive_signals: score.positive_signals,
      negative_signals: score.negative_signals,
    }),
    review_status: reviewStatus,
  }) as any

  // If auto-approved, copy into the main records table
  if (isAutoApproved) {
    try {
      if (recordType === 'bill') {
        db.createBill({
          workspace_id: 1,
          user_id: 1,
          vendor,
          amount,
          currency,
          due_date: dueDate,
          category,
          recurring: rule?.recurring_frequency && rule.recurring_frequency !== 'one_time' ? 1 : 0,
          frequency: rule?.recurring_frequency || 'one_time',
          notes: rule ? `Auto-approved by rule: ${rule.rule_name}` : `Auto-approved with high confidence (${Math.round(confidence * 100)}%)`,
          source: 'email',
        })
      } else if (recordType === 'expense' || recordType === 'receipt') {
        db.createExpense({
          date: paymentDate || dueDate || fallbackDate,
          category,
          description: `Auto-approved from email: ${vendor}`,
          amount,
          vendor,
          currency,
          source: 'email',
        })
      } else if (recordType === 'subscription') {
        db.createSubscription({
          name: vendor,
          vendor,
          amount,
          currency,
          billing_cycle: rule?.recurring_frequency || 'monthly',
          next_billing_date: dueDate || paymentDate,
          category,
          status: 'active',
        })
      } else if (recordType === 'payment') {
        // Money received (e-Transfer, deposit) → income ledger, NOT a fake invoice
        db.createPayment({
          vendor,
          amount,
          currency,
          category: 'income',
          payment_date: paymentDate || fallbackDate,
          payment_method: paymentMethod || 'e-Transfer',
          source: 'email',
          source_email_id: null,
          notes: `Income from ${vendor}`,
        })
      }
    } catch (err) {
      console.error('Auto-approve failed:', err)
      // Fallback: revert status to needs_review in DB so the user doesn't lose the bill
      db.updateCandidate(candidate.id, { review_status: 'needs_review' })
      candidate.review_status = 'needs_review'
    }
  }

  return candidate
}

/**
 * Fetch and scan emails matching query, process them locally through the candidate pipeline
 */
export async function syncGmailEmails(daysRange: number = 30): Promise<{ fetched: number; skipped: number }> {
  const accessToken = await getValidAccessToken()
  if (!accessToken) {
    throw new Error('Gmail account is not connected.')
  }

  // Fetch list of messages
  const query = `newer_than:${daysRange}d (invoice OR receipt OR bill OR statement OR "amount due" OR "payment due" OR "payment received" OR subscription OR renewal OR autopay OR "sent you money" OR "e-Transfer" OR interac)`
  const listUrl = `https://gmail.googleapis.com/gmail/v1/users/me/messages?` + new URLSearchParams({ q: query })

  const response = await fetch(listUrl, {
    headers: { Authorization: `Bearer ${accessToken}` }
  })

  if (!response.ok) {
    throw new Error(`Gmail API returned error listing messages: ${response.statusText}`)
  }

  const data = (await response.json()) as { messages?: Array<{ id: string; threadId: string }> }
  const messages = data.messages || []

  let fetchedCount = 0
  let skippedCount = 0

  // Build lookups ONCE up front instead of re-querying inside the loop (was O(n²)).
  const existingIds = new Set((db.listEmailImports() as any[]).map(e => e.source_email_id))
  const knownDomains = buildKnownDomains()

  for (const msg of messages) {
    // Check if already processed
    if (existingIds.has(msg.id)) {
      skippedCount++
      continue
    }

    // Fetch message detail
    const detailUrl = `https://gmail.googleapis.com/gmail/v1/users/me/messages/${msg.id}?format=full`
    const detailRes = await fetch(detailUrl, {
      headers: { Authorization: `Bearer ${accessToken}` }
    })

    if (!detailRes.ok) continue

    const msgData = await detailRes.json()
    const headers = msgData.payload.headers || []

    const fromHeader = headers.find((h: any) => h.name.toLowerCase() === 'from')?.value || 'Unknown Sender'
    const subjectHeader = headers.find((h: any) => h.name.toLowerCase() === 'subject')?.value || 'No Subject'
    const dateHeader = headers.find((h: any) => h.name.toLowerCase() === 'date')?.value || new Date().toISOString()

    const body = getMessageBody(msgData.payload)
    const preview = msgData.snippet || ''

    // Attachments extraction
    const attachments: string[] = []
    if (msgData.payload.parts) {
      for (const part of msgData.payload.parts) {
        if (part.filename) attachments.push(part.filename)
      }
    }

    // Save email import log
    const emailImport = db.createEmailImport({
      workspace_id: 1,
      user_id: 1,
      provider: 'gmail',
      source_email_id: msg.id,
      sender: fromHeader,
      subject: subjectHeader,
      received_at: new Date(dateHeader).toISOString(),
      body_preview: preview,
      attachment_names: attachments.join(','),
      status: 'detected',
      confidence_score: 1.0
    }) as any

    // Remember it so a duplicate within the same batch is skipped too
    existingIds.add(msg.id)

    // Process extraction immediately (relevance scoring runs inside)
    extractCandidateFromEmail(emailImport.id, fromHeader, subjectHeader, body, {
      knownDomains,
      attachmentNames: attachments,
    })
    fetchedCount++
  }

  return { fetched: fetchedCount, skipped: skippedCount }
}
