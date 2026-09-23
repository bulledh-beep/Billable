import { useEffect, type ReactNode } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { X } from 'lucide-react'

interface ModalProps {
  isOpen: boolean
  onClose: () => void
  title: string
  description?: ReactNode
  children: ReactNode
  /** Buttons pinned under the body, right-aligned. */
  footer?: ReactNode
  size?: 'sm' | 'md' | 'lg' | 'xl'
}

const sizes = {
  sm: 'max-w-[420px]',
  md: 'max-w-[520px]',
  lg: 'max-w-[680px]',
  xl: 'max-w-[880px]',
}

export default function Modal({ isOpen, onClose, title, description, children, footer, size = 'md' }: ModalProps) {
  useEffect(() => {
    if (!isOpen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.stopPropagation(); onClose() }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [isOpen, onClose])

  return (
    <AnimatePresence>
      {isOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-6">
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.12 }}
            className="absolute inset-0 bg-black/25 dark:bg-black/50"
            onClick={onClose}
          />
          <motion.div
            role="dialog"
            aria-modal="true"
            aria-label={title}
            initial={{ opacity: 0, scale: 0.985, y: 4 }}
            animate={{ opacity: 1, scale: 1, y: 0, transition: { duration: 0.16, ease: [0.2, 0.8, 0.2, 1] } }}
            exit={{ opacity: 0, scale: 0.985, transition: { duration: 0.1, ease: [0.4, 0, 1, 1] } }}
            className={`relative ${sizes[size]} w-full max-h-[calc(100vh-48px)] flex flex-col bg-panel rounded-[12px] shadow-pop`}
          >
            <div className="flex items-start justify-between gap-4 px-5 pt-4 pb-3">
              <div className="min-w-0">
                <h2 className="text-[15px] leading-5 font-semibold text-fg">{title}</h2>
                {description && <p className="text-xs text-fg-3 mt-1 leading-[17px]">{description}</p>}
              </div>
              <button onClick={onClose} className="btn-icon-sm -mr-1.5 -mt-0.5" aria-label="Close">
                <X />
              </button>
            </div>
            <div className="px-5 pb-5 overflow-y-auto min-h-0">
              {children}
            </div>
            {footer && (
              <div className="flex items-center justify-end gap-2 px-5 py-3 border-t border-line">
                {footer}
              </div>
            )}
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  )
}
