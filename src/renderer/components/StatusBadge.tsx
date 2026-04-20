const statusStyles: Record<string, { bg: string; text: string; dot: string }> = {
  active: { bg: 'bg-status-active/10', text: 'text-status-active', dot: 'bg-status-active' },
  paused: { bg: 'bg-status-paused/10', text: 'text-status-paused', dot: 'bg-status-paused' },
  complete: { bg: 'bg-status-complete/10', text: 'text-status-complete', dot: 'bg-status-complete' },
  archived: { bg: 'bg-status-archived/10', text: 'text-status-archived', dot: 'bg-status-archived' },
  draft: { bg: 'bg-status-draft/10', text: 'text-status-draft', dot: 'bg-status-draft' },
  sent: { bg: 'bg-status-sent/10', text: 'text-status-sent', dot: 'bg-status-sent' },
  paid: { bg: 'bg-status-paid/10', text: 'text-status-paid', dot: 'bg-status-paid' },
  overdue: { bg: 'bg-status-overdue/10', text: 'text-status-overdue', dot: 'bg-status-overdue' },
}

export default function StatusBadge({ status }: { status: string }) {
  const style = statusStyles[status] || statusStyles.draft

  return (
    <span className={`status-badge ${style.bg} ${style.text}`}>
      <span className={`w-1.5 h-1.5 rounded-full ${style.dot}`} />
      <span className="capitalize">{status}</span>
    </span>
  )
}
