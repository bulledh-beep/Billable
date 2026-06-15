/**
 * subscriptionDetector — decides whether a financial email represents a
 * recurring subscription (vs a one-time purchase or bill) and, if so, at what
 * frequency.
 *
 * Three signal sources, strongest first:
 *   1. History — we've already seen this vendor as a subscription, or charged
 *      repeatedly at a regular cadence. The most reliable signal.
 *   2. Recurrence language — "renews monthly", "auto-renew", "/mo", "annual
 *      plan", "next billing date", etc.
 *   3. Known subscription vendors — Netflix, Adobe, Spotify, iCloud, etc.
 *      almost always bill on a recurring basis.
 */

export type SubFrequency = 'weekly' | 'monthly' | 'quarterly' | 'yearly'

export interface SubscriptionDetectInput {
  vendor: string
  subject: string
  body: string
  amount?: number | null
  /** Vendors already known to recur (existing subscriptions / repeat charges). */
  knownRecurringVendors?: Map<string, SubFrequency>
}

export interface SubscriptionDetectResult {
  isSubscription: boolean
  frequency: SubFrequency | null
  reason: string
}

// Known subscription vendors → their typical default cadence (used only when the
// email text doesn't state one explicitly).
const KNOWN_SUBSCRIPTION_VENDORS: Array<{ re: RegExp; freq: SubFrequency }> = [
  { re: /\bnetflix\b/i, freq: 'monthly' },
  { re: /\bspotify\b/i, freq: 'monthly' },
  { re: /\b(disney\+?|disneyplus)\b/i, freq: 'monthly' },
  { re: /\b(youtube premium|youtube music)\b/i, freq: 'monthly' },
  { re: /\bcrave\b/i, freq: 'monthly' },
  { re: /\b(amazon prime|prime video)\b/i, freq: 'yearly' },
  { re: /\b(apple music|icloud|apple one|apple tv\+?|app store)\b/i, freq: 'monthly' },
  { re: /\badobe\b/i, freq: 'monthly' },
  { re: /\b(microsoft 365|office 365|microsoft365)\b/i, freq: 'monthly' },
  { re: /\b(google (one|workspace|storage)|google one)\b/i, freq: 'monthly' },
  { re: /\bdropbox\b/i, freq: 'monthly' },
  { re: /\b(notion|canva|figma|grammarly)\b/i, freq: 'monthly' },
  { re: /\bgithub\b/i, freq: 'monthly' },
  { re: /\b(openai|chatgpt|claude|anthropic)\b/i, freq: 'monthly' },
  { re: /\b(slack|zoom|asana|trello|linear)\b/i, freq: 'monthly' },
  { re: /\b(1password|lastpass|dashlane|nordvpn|expressvpn)\b/i, freq: 'monthly' },
  { re: /\b(patreon|substack|medium)\b/i, freq: 'monthly' },
  { re: /\b(planet fitness|goodlife|anytime fitness|gym)\b/i, freq: 'monthly' },
]

// Recurrence language — presence of any strongly implies a subscription.
const RECURRENCE_RE = /\b(subscription|subscribed|auto[\s-]?renew(s|al|ing)?|recurring|renews?\s+(on|monthly|annually|yearly|automatically)|membership\s+(renew|fee)|next\s+(billing|payment|charge|renewal)\s+date|billing\s+cycle|monthly\s+plan|annual\s+plan|we'?ll\s+(automatically\s+)?(charge|bill)|every\s+(month|year|week)|per\s+(month|year))\b/i

function senderLikeBlob(input: SubscriptionDetectInput): string {
  return `${input.subject}\n${input.body}`
}

/** Pull an explicit cadence out of the text, if stated. */
function parseFrequency(text: string): SubFrequency | null {
  if (/\b(annual(ly)?|per\s+year|\/\s?(yr|year)|yearly|once\s+a\s+year|12[\s-]month)\b/i.test(text)) return 'yearly'
  if (/\bquarter(ly)?\b/i.test(text)) return 'quarterly'
  if (/\bweek(ly)?\b/i.test(text)) return 'weekly'
  if (/\b(month(ly)?|per\s+month|\/\s?(mo|month)|each\s+month)\b/i.test(text)) return 'monthly'
  return null
}

function matchKnownVendor(vendor: string, blob: string): SubFrequency | null {
  for (const v of KNOWN_SUBSCRIPTION_VENDORS) {
    if (v.re.test(vendor) || v.re.test(blob)) return v.freq
  }
  return null
}

export function detectSubscription(input: SubscriptionDetectInput): SubscriptionDetectResult {
  const blob = senderLikeBlob(input)
  const vendorKey = (input.vendor || '').trim().toLowerCase()

  // 1. History — strongest signal
  if (input.knownRecurringVendors && vendorKey && input.knownRecurringVendors.has(vendorKey)) {
    const freq = parseFrequency(blob) || input.knownRecurringVendors.get(vendorKey) || 'monthly'
    return { isSubscription: true, frequency: freq, reason: 'Recurring charge from a vendor you already subscribe to' }
  }

  const textFreq = parseFrequency(blob)
  const hasRecurrenceLang = RECURRENCE_RE.test(blob)
  const knownVendorFreq = matchKnownVendor(input.vendor || '', blob)

  // 2. Explicit recurrence language
  if (hasRecurrenceLang) {
    return {
      isSubscription: true,
      frequency: textFreq || knownVendorFreq || 'monthly',
      reason: 'Email states it recurs (renewal / auto-renew / billing cycle)',
    }
  }

  // 3. Known subscription vendor sending a charge/receipt
  if (knownVendorFreq) {
    return {
      isSubscription: true,
      frequency: textFreq || knownVendorFreq,
      reason: 'Known subscription vendor',
    }
  }

  return { isSubscription: false, frequency: null, reason: '' }
}
