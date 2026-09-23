import { Clock, AlertCircle, FileText, Timer, Users, type LucideIcon } from 'lucide-react'
import type { AttentionItem } from '@shared/types'

/** The glyph for each kind of billing item that needs a look. */
export const ATTENTION_ICON: Record<AttentionItem['kind'], LucideIcon> = {
  unbilled: Clock,
  overdue: AlertCircle,
  draft: FileText,
  accidental_timer: Timer,
  duplicate_clients: Users,
}

/** Red for money that's late, amber for money that's waiting, gray otherwise. */
export function attentionColor(tone: AttentionItem['tone']) {
  return tone === 'danger' ? 'text-red' : tone === 'warning' ? 'text-amber' : 'text-fg-3'
}
