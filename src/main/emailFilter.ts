/**
 * emailFilter — scores an email for financial relevance and extraction confidence.
 *
 * Two distinct numbers, intentionally separate:
 *
 *   relevance_score (0-100):  "Is this a financial email at all, vs marketing/noise?"
 *                             Driven by positive financial signals minus negative
 *                             (promo/newsletter/shipping) signals.
 *
 *   confidence_score (0-100): "Given it IS financial, how sure are we the extracted
 *                             fields (vendor/amount/date) are correct?"
 *                             Driven by corroboration — multiple signals agreeing.
 *
 * Auto-approval requires BOTH to be high, plus complete fields and no duplicate.
 * A high confidence on its own (a number next to the word "total") is NOT enough —
 * that was the old bug.
 */

export interface EmailSignalInput {
  sender: string
  subject: string
  body: string
  attachmentNames?: string[]
  /** Vendor domains the user has already approved (from existing bills/subs/expenses). */
  knownDomains?: Set<string>
  /** True if an automation rule matched this email. */
  ruleMatched?: boolean
}

export interface EmailScore {
  relevance_score: number
  confidence_score: number
  /** Short human-readable reasons this was flagged as financial. */
  detected_reason: string
  /** Why confidence may be low / what's missing. */
  low_confidence_reason: string
  positive_signals: string[]
  negative_signals: string[]
}

// Strong subject/body terms that indicate a real financial document.
const STRONG_TERMS: Array<{ re: RegExp; label: string; weight: number }> = [
  { re: /\binvoice\b/i, label: 'mentions invoice', weight: 22 },
  { re: /\breceipt\b/i, label: 'mentions receipt', weight: 20 },
  { re: /\b(bill|billing)\b/i, label: 'mentions bill', weight: 18 },
  { re: /\bstatement\b/i, label: 'mentions statement', weight: 18 },
  { re: /\bamount\s+due\b/i, label: '"amount due"', weight: 26 },
  { re: /\bpayment\s+due\b/i, label: '"payment due"', weight: 26 },
  { re: /\bbalance\s+due\b/i, label: '"balance due"', weight: 24 },
  { re: /\bminimum\s+payment\b/i, label: '"minimum payment"', weight: 24 },
  { re: /\bpayment\s+(received|confirmed|processed|successful)\b/i, label: 'payment confirmation', weight: 24 },
  { re: /\bpayment\s+scheduled\b/i, label: '"payment scheduled"', weight: 20 },
  { re: /\byour\s+bill\s+is\s+ready\b/i, label: '"your bill is ready"', weight: 26 },
  { re: /\bstatement\s+is\s+(ready|available)\b/i, label: 'statement available', weight: 22 },
  { re: /\b(subscription|renewal|renews|auto[\s-]?renew)\b/i, label: 'subscription/renewal', weight: 16 },
  { re: /\bautopay\b/i, label: 'autopay', weight: 18 },
  { re: /\border\s+confirmation\b/i, label: 'order confirmation', weight: 16 },
  { re: /\btax\s+invoice\b/i, label: 'tax invoice', weight: 22 },
  { re: /\binterac\s+e-?transfer\b/i, label: 'Interac e-Transfer', weight: 22 },
  { re: /\b(sent you money|sent you a payment|deposited)\b/i, label: 'money transfer', weight: 18 },
]

// Marketing / non-financial signals that should pull the score down.
const NEGATIVE_TERMS: Array<{ re: RegExp; label: string; weight: number }> = [
  { re: /\b\d{1,3}%\s*(off|discount)\b/i, label: 'percent-off promo', weight: 28 },
  { re: /\b(sale|clearance|deal of the day|flash sale)\b/i, label: 'sale language', weight: 22 },
  { re: /\blimited[\s-]time\b/i, label: '"limited time"', weight: 20 },
  { re: /\b(coupon|promo code|voucher)\b/i, label: 'coupon/promo', weight: 22 },
  { re: /\bsave up to\b/i, label: '"save up to"', weight: 20 },
  { re: /\b(recommended for you|you may (also )?like|picked for you)\b/i, label: 'recommendations', weight: 24 },
  { re: /\b(shop now|buy now|order now|browse|explore our)\b/i, label: 'shopping CTA', weight: 18 },
  { re: /\b(newsletter|weekly digest|this week at)\b/i, label: 'newsletter', weight: 22 },
  { re: /\b(don'?t miss|last chance|ends (soon|tonight|today))\b/i, label: 'urgency marketing', weight: 18 },
  { re: /\b(out for delivery|has shipped|on its way|tracking number|delivery update)\b/i, label: 'shipping update', weight: 16 },
  { re: /\b(survey|rate your|how did we do|leave a review)\b/i, label: 'survey/feedback', weight: 14 },
]

const AMOUNT_RE = /(?:\$|USD|CAD|US\$|CA\$)?\s?([0-9]{1,6}(?:,[0-9]{3})*(?:\.[0-9]{2}))/
const DATE_LANG_RE = /\b(due\s+date|due\s+by|due\s+on|payment\s+date|paid\s+on|charged\s+on|statement\s+date|invoice\s+date|next\s+billing|renews\s+on|billing\s+date)\b/i
const INVOICE_NUM_RE = /\b(invoice|inv|statement|account|ref(?:erence)?)\s*#?\s*[:.]?\s*([A-Z0-9][A-Z0-9-]{3,})/i
const ACCOUNT_LAST4_RE = /\b(?:account|acct|card|ending(?:\s+in)?)\s*(?:#|no\.?|number)?\s*(?:\*+|x+|ending\s+in)?\s*(\d{4})\b/i

function senderDomain(sender: string): string {
  const m = sender.match(/@([a-z0-9.-]+)/i)
  return m ? m[1].toLowerCase() : ''
}

/**
 * Domains we treat as inherently financial/vendor-ish. Not exhaustive — the
 * user's own approved-vendor domains (knownDomains) carry more weight than this.
 */
const FINANCIAL_DOMAIN_HINTS = [
  'hydro', 'bchydro', 'fortis', 'enmax', 'epcor', 'rogers', 'telus', 'bell', 'shaw', 'fido', 'koodo',
  'adobe', 'apple', 'google', 'microsoft', 'netflix', 'spotify', 'dropbox', 'github', 'notion', 'canva',
  'intuit', 'quickbooks', 'paypal', 'stripe', 'square', 'wealthsimple', 'interac',
  'visa', 'mastercard', 'amex', 'rbc', 'td', 'scotiabank', 'cibc', 'bmo', 'tangerine',
  'insurance', 'icbc', 'billing', 'invoice', 'noreply.', 'statements',
  // Marketplaces / services that send legitimate receipts (their shipping &
  // marketing mail is still filtered out by the negative signals below).
  'amazon', 'walmart', 'costco', 'ebay', 'bestbuy', 'uber', 'lyft', 'doordash', 'skipthedishes',
]

function clamp(n: number, lo = 0, hi = 100): number {
  return Math.max(lo, Math.min(hi, n))
}

export function scoreEmail(input: EmailSignalInput): EmailScore {
  const subject = input.subject || ''
  const body = input.body || ''
  const haystack = `${subject}\n${body}`
  const domain = senderDomain(input.sender)
  const attachments = input.attachmentNames || []

  const positive: string[] = []
  const negative: string[] = []
  let relevance = 0
  let confidence = 0

  // ---- Positive: strong financial terms (subject weighted higher than body) ----
  let strongHits = 0
  for (const t of STRONG_TERMS) {
    const inSubject = t.re.test(subject)
    const inBody = !inSubject && t.re.test(body)
    if (inSubject) {
      relevance += t.weight
      positive.push(`Subject ${t.label}`)
      strongHits++
    } else if (inBody) {
      relevance += Math.round(t.weight * 0.6)
      positive.push(t.label)
      strongHits++
    }
  }

  // ---- Positive: sender domain ----
  if (input.knownDomains && domain && input.knownDomains.has(domain)) {
    relevance += 25
    confidence += 20
    positive.push('Sender already an approved vendor')
  } else if (domain && FINANCIAL_DOMAIN_HINTS.some(h => domain.includes(h))) {
    relevance += 14
    confidence += 8
    positive.push('Known financial/vendor domain')
  }

  // ---- Positive: automation rule ----
  if (input.ruleMatched) {
    relevance += 20
    confidence += 25
    positive.push('Matches an automation rule')
  }

  // ---- Positive: currency amount present ----
  const hasAmount = AMOUNT_RE.test(haystack)
  if (hasAmount) {
    relevance += 14
    confidence += 22
    positive.push('Contains a currency amount')
  }

  // ---- Positive: financial date language ----
  const hasDateLang = DATE_LANG_RE.test(haystack)
  if (hasDateLang) {
    relevance += 12
    confidence += 18
    positive.push('Contains due/payment date language')
  }

  // ---- Positive: invoice / account numbers ----
  if (INVOICE_NUM_RE.test(haystack)) {
    relevance += 10
    confidence += 14
    positive.push('Contains an invoice/reference number')
  }
  if (ACCOUNT_LAST4_RE.test(haystack)) {
    relevance += 6
    confidence += 8
    positive.push('Contains an account number')
  }

  // ---- Positive: attachments ----
  const hasPdf = attachments.some(n => /\.pdf$/i.test(n))
  const hasReceiptAttach = attachments.some(n => /(invoice|receipt|statement|bill)/i.test(n))
  if (hasPdf) {
    relevance += 10
    confidence += 6
    positive.push('Has a PDF attachment')
  }
  if (hasReceiptAttach) {
    relevance += 8
    positive.push('Attachment named like an invoice/receipt')
  }

  // ---- Negative: marketing / noise ----
  let negHits = 0
  for (const t of NEGATIVE_TERMS) {
    if (t.re.test(haystack)) {
      relevance -= t.weight
      negative.push(t.label)
      negHits++
    }
  }

  // ---- Structural negatives ----
  if (!hasAmount && !hasDateLang) {
    relevance -= 20
    negative.push('No amount and no financial date')
  }
  // Heavy unsubscribe presence with no strong financial term → likely a blast
  if (/\bunsubscribe\b/i.test(body) && strongHits === 0) {
    relevance -= 12
    negative.push('Unsubscribe-heavy with no bill terms')
  }
  // If marketing signals dominate and there's only a weak financial hint, suppress
  if (negHits >= 2 && strongHits <= 1) {
    relevance -= 10
    negative.push('Marketing signals outweigh financial signals')
  }

  // Corroboration bonus: an amount + a financial date + a strong financial term
  // all co-occurring is the hallmark of a real bill/receipt. This is what lets a
  // clean utility bill clear the auto-approve bar — while marketing (no amount,
  // no date, strongHits 0) gets nothing.
  if (hasAmount && hasDateLang && strongHits >= 1) {
    confidence += 18
    positive.push('Amount, date, and financial term all present')
  } else if (hasAmount && strongHits >= 2) {
    confidence += 10
  }

  relevance = clamp(relevance)
  confidence = clamp(confidence)

  // Confidence is meaningless if the email probably isn't financial — cap it to
  // relevance so a marketing email can never present as "high confidence".
  confidence = Math.min(confidence, relevance)

  const detected_reason = positive.length
    ? positive.slice(0, 3).join(' · ')
    : 'No clear financial signals'

  const lowReasons: string[] = []
  if (!hasAmount) lowReasons.push('no amount found')
  if (!hasDateLang) lowReasons.push('no due/payment date')
  if (negHits > 0) lowReasons.push('marketing language present')
  if (strongHits === 0) lowReasons.push('no strong financial terms')
  const low_confidence_reason = lowReasons.join(', ')

  return {
    relevance_score: Math.round(relevance),
    confidence_score: Math.round(confidence),
    detected_reason,
    low_confidence_reason,
    positive_signals: positive,
    negative_signals: negative,
  }
}

/** Relevance bands per the product spec. */
export type RelevanceBand = 'high' | 'likely' | 'possible' | 'ignore'

export function relevanceBand(score: number): RelevanceBand {
  if (score >= 80) return 'high'
  if (score >= 60) return 'likely'
  if (score >= 40) return 'possible'
  return 'ignore'
}
