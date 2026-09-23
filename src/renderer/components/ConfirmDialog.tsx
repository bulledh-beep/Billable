import type { ReactNode } from 'react'
import Modal from './Modal'

interface ConfirmDialogProps {
  isOpen: boolean
  onClose: () => void
  onConfirm: () => void
  title: string
  message: ReactNode
  confirmText?: string
  danger?: boolean
}

export default function ConfirmDialog({
  isOpen,
  onClose,
  onConfirm,
  title,
  message,
  confirmText = 'Delete',
  danger = true,
}: ConfirmDialogProps) {
  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title={title}
      size="sm"
      footer={
        <>
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button
            autoFocus
            onClick={() => { onConfirm(); onClose() }}
            className={danger ? 'btn bg-[#E0383E] text-white font-semibold hover:bg-[#CC2F35] shadow-[inset_0_0.5px_0_rgb(255_255_255/0.3),0_0.5px_1px_rgb(0_0_0/0.2)]' : 'btn-primary'}
          >
            {confirmText}
          </button>
        </>
      }
    >
      <div className="text-sm text-fg-2 leading-5">{message}</div>
    </Modal>
  )
}
