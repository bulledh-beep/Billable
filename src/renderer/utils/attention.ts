import type { ElementType } from 'react'
import { IconHourglass, IconAlert, IconReceipt, IconTime, IconClients } from '../components/Illustrations'
import type { AttentionItem } from '@shared/types'

/** The illustration for each kind of billing item that needs a look. */
export const ATTENTION_ICON: Record<AttentionItem['kind'], ElementType> = {
  unbilled: IconHourglass,
  overdue: IconAlert,
  draft: IconReceipt,
  accidental_timer: IconTime,
  duplicate_clients: IconClients,
}

/** Kept for callers that tint text by urgency. */
export function attentionColor(tone: AttentionItem['tone']) {
  return tone === 'danger' ? 'text-red' : tone === 'warning' ? 'text-amber' : 'text-fg-3'
}
