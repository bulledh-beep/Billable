import type { ReactNode } from 'react'

/** A labelled settings row: label and help on the left, control on the right. */
export function Row({ label, help, children }: { label: string; help?: ReactNode; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[220px_minmax(0,1fr)] gap-6 px-5 py-4 border-b border-line last:border-b-0">
      <div className="pt-[5px]">
        <div className="text-[13px] font-medium text-fg">{label}</div>
        {help && <div className="text-xs text-fg-3 mt-1 leading-4">{help}</div>}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  )
}

export function Section({ title, description, action, children }: {
  title: string
  description?: ReactNode
  action?: ReactNode
  children: ReactNode
}) {
  return (
    <section className="mb-7">
      <div className="flex items-end justify-between gap-4 mb-2.5">
        <div>
          <h2 className="section-title">{title}</h2>
          {description && <p className="text-xs text-fg-3 mt-0.5">{description}</p>}
        </div>
        {action}
      </div>
      <div className="card">{children}</div>
    </section>
  )
}
