import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { MoreHorizontal, type LucideIcon } from 'lucide-react'

/**
 * Floating panel anchored to a trigger. Rendered in a portal with fixed
 * positioning so tables and cards never clip it.
 */
export function Popover({ open, anchor, onClose, align = 'end', width, children }: {
  open: boolean
  anchor: HTMLElement | null
  onClose: () => void
  align?: 'start' | 'end'
  width?: number
  children: ReactNode
}) {
  const panelRef = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState<{ top: number; left: number; up: boolean } | null>(null)

  useLayoutEffect(() => {
    if (!open || !anchor) return
    const place = () => {
      const r = anchor.getBoundingClientRect()
      const panel = panelRef.current
      const w = panel?.offsetWidth || width || 200
      const h = panel?.offsetHeight || 0
      let left = align === 'end' ? r.right - w : r.left
      left = Math.max(8, Math.min(left, window.innerWidth - w - 8))
      const up = r.bottom + 6 + h > window.innerHeight - 8 && r.top - 6 - h > 8
      setPos({ top: up ? r.top - 6 - h : r.bottom + 6, left, up })
    }
    place()
    const raf = requestAnimationFrame(place)
    return () => cancelAnimationFrame(raf)
  }, [open, anchor, align, width])

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node
      if (panelRef.current?.contains(t) || anchor?.contains(t)) return
      onClose()
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    const onScroll = (e: Event) => {
      if (panelRef.current && e.target instanceof Node && panelRef.current.contains(e.target)) return
      onClose()
    }
    document.addEventListener('mousedown', onDown)
    document.addEventListener('keydown', onKey)
    window.addEventListener('resize', onClose)
    document.addEventListener('scroll', onScroll, true)
    return () => {
      document.removeEventListener('mousedown', onDown)
      document.removeEventListener('keydown', onKey)
      window.removeEventListener('resize', onClose)
      document.removeEventListener('scroll', onScroll, true)
    }
  }, [open, anchor, onClose])

  if (!open) return null
  return createPortal(
    <div
      ref={panelRef}
      style={{ top: pos?.top ?? -9999, left: pos?.left ?? -9999, width, visibility: pos ? 'visible' : 'hidden' }}
      className="fixed z-[60] rounded-[12px] bg-panel shadow-pop animate-fade-in"
      onClick={e => e.stopPropagation()}
    >
      {children}
    </div>,
    document.body,
  )
}

export type MenuItem =
  | {
      label: string
      icon?: LucideIcon
      onClick: () => void
      danger?: boolean
      disabled?: boolean
      hint?: string
    }
  | 'separator'
  | null
  | false
  | undefined

/** "…" button with a list of actions. */
export default function Menu({ items, trigger, align = 'end', label = 'More actions' }: {
  items: MenuItem[]
  trigger?: (props: { onClick: (e: React.MouseEvent) => void; ref: (el: HTMLElement | null) => void; open: boolean }) => ReactNode
  align?: 'start' | 'end'
  label?: string
}) {
  const [open, setOpen] = useState(false)
  const [anchor, setAnchor] = useState<HTMLElement | null>(null)
  const toggle = (e: React.MouseEvent) => { e.stopPropagation(); setOpen(o => !o) }
  // Drop empty slots, then any separator that isn't between two real items
  const visible = (items.filter(Boolean) as Exclude<MenuItem, null | false | undefined>[])
    .filter((it, i, arr) => it !== 'separator' || (i > 0 && i < arr.length - 1 && arr[i - 1] !== 'separator'))
    .filter((it, i, arr) => it !== 'separator' || i < arr.length - 1)

  return (
    <>
      {trigger ? trigger({ onClick: toggle, ref: setAnchor, open }) : (
        <button
          ref={setAnchor}
          onClick={toggle}
          className={`btn-icon-sm ${open ? 'bg-fg/[0.08] text-fg' : ''}`}
          aria-label={label}
          title={label}
        >
          <MoreHorizontal />
        </button>
      )}
      <Popover open={open} anchor={anchor} onClose={() => setOpen(false)} align={align}>
        <div className="p-[5px] min-w-[190px]">
          {visible.map((it, i) => it === 'separator' ? (
            <div key={`sep-${i}`} className="my-[5px] mx-2 border-t border-line" />
          ) : (
            <button
              key={it.label}
              disabled={it.disabled}
              onClick={e => { e.stopPropagation(); setOpen(false); it.onClick() }}
              className={`w-full flex items-center gap-2 h-[31px] px-2.5 rounded-[8px] text-[13.5px] font-medium text-left transition-colors disabled:opacity-40
                ${it.danger ? 'text-red hover:bg-red/10' : 'text-fg hover:bg-fg/[0.07]'}`}
            >
              {it.icon && <it.icon className={`w-[14px] h-[14px] shrink-0 ${it.danger ? '' : 'text-fg-3'}`} />}
              <span className="flex-1 truncate">{it.label}</span>
              {it.hint && <span className="text-2xs text-fg-4">{it.hint}</span>}
            </button>
          ))}
        </div>
      </Popover>
    </>
  )
}
