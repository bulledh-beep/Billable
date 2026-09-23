// Lightweight app-wide signal: something changed that affects billing totals
// (invoice created, time edited, payment recorded). The sidebar badge and any
// page showing money listen for it and refetch.

const EVENT = 'billable:billing-changed'

export function notifyBillingChanged() {
  window.dispatchEvent(new Event(EVENT))
}

export function onBillingChanged(cb: () => void): () => void {
  window.addEventListener(EVENT, cb)
  return () => window.removeEventListener(EVENT, cb)
}
