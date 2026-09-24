import { createContext, useContext, useMemo, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { Link } from 'react-router-dom'
import { ChevronLeft } from 'lucide-react'

/**
 * The window toolbar lives in App, outside the scrolling page. Pages describe
 * their title and actions with <PageHeader>, which portals into the toolbar.
 */
interface Slots {
  title: HTMLElement | null
  actions: HTMLElement | null
}

const HeaderSlotsContext = createContext<Slots>({ title: null, actions: null })
export const HeaderSlotsProvider = HeaderSlotsContext.Provider

export function useHeaderSlots() {
  const [title, setTitle] = useState<HTMLElement | null>(null)
  const [actions, setActions] = useState<HTMLElement | null>(null)
  // Stable object so the ticking timer doesn't re-render every page header
  const slots = useMemo(() => ({ title, actions }), [title, actions])
  return { slots, titleRef: setTitle, actionsRef: setActions }
}

export interface Crumb {
  label: string
  to: string
}

interface PageHeaderProps {
  title: ReactNode
  /** Parent pages. The nearest one becomes the back button. */
  crumbs?: Crumb[]
  /** Small inline content after the title, such as a status badge. */
  meta?: ReactNode
  /** A quiet second line under the title, like Mail's message count. */
  subtitle?: ReactNode
  actions?: ReactNode
}

export default function PageHeader({ title, crumbs, meta, subtitle, actions }: PageHeaderProps) {
  const slots = useContext(HeaderSlotsContext)
  // A back button to the parent page, the way Mac apps do it, instead of breadcrumbs
  const back = crumbs?.length ? crumbs[crumbs.length - 1] : null
  return (
    <>
      {slots.title && createPortal(
        <div className="flex items-center gap-1.5 min-w-0">
          {back && (
            <Link
              to={back.to}
              className="no-drag btn-icon -ml-2 shrink-0"
              title={`Back to ${back.label}`}
              aria-label={`Back to ${back.label}`}
            >
              <ChevronLeft className="!w-[18px] !h-[18px]" strokeWidth={2} />
            </Link>
          )}
          <div className="min-w-0">
            <h1 className="text-[20px] leading-[24px] font-bold tracking-[-0.015em] text-fg truncate">{title}</h1>
            {(subtitle || back) && <div className="text-[12px] font-semibold text-fg-3 truncate">{subtitle || back?.label}</div>}
          </div>
          {meta && <div className="no-drag flex items-center gap-2 ml-2 shrink-0">{meta}</div>}
        </div>,
        slots.title,
      )}
      {slots.actions && actions && createPortal(
        <div className="no-drag flex items-center gap-2">{actions}</div>,
        slots.actions,
      )}
    </>
  )
}
