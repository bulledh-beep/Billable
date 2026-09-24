import { useEffect, useState } from 'react'
import toast from 'react-hot-toast'
import Modal from './Modal'
import { Mascot } from './Illustrations'

interface ConnectRequest {
  requestId: string
  host: string
  /** The Content HQ account the key belongs to, when Content HQ says. */
  account: string | null
  profileName: string | null
}

/**
 * Content HQ (inside Billable's window) asked to connect for phone sync.
 * Nothing is stored until you say yes here.
 */
export default function PhoneConnectModal() {
  const [request, setRequest] = useState<ConnectRequest | null>(null)

  useEffect(() => {
    const off = window.api.on('phone:connect-request', (r: ConnectRequest) => setRequest(r))
    const offError = window.api.on('phone:connect-error', (message: string) => toast.error(message))
    return () => { off?.(); offError?.() }
  }, [])

  const answer = async (accept: boolean) => {
    const r = request
    setRequest(null)
    if (!r) return
    const status = await window.api.phone.confirmConnect(r.requestId, accept)
    if (!accept) return
    if (status?.connected) {
      toast('Connected. Billable will show up in Content HQ on your phone in a moment.', {
        icon: <Mascot size={26} mood="happy" motion="hop" />,
        duration: 4000,
      })
    } else {
      toast.error(status?.message || 'Billable couldn’t connect. Try again from Content HQ.')
    }
  }

  return (
    <Modal
      isOpen={!!request}
      onClose={() => answer(false)}
      title="Use Billable on your phone?"
      size="sm"
      footer={
        <>
          <button onClick={() => answer(false)} className="btn-secondary">Cancel</button>
          <button autoFocus onClick={() => answer(true)} className="btn-primary">Connect</button>
        </>
      }
    >
      <div className="space-y-3 text-sm text-fg-2 leading-5">
        {request?.account ? (
          <p>
            Connect Billable{request.profileName ? <> ({request.profileName} profile)</> : null} to the
            Content HQ account <span className="font-semibold text-fg break-all">{request.account}</span>?
            Only continue if that’s your account.
          </p>
        ) : (
          <p>
            Content HQ at <span className="font-semibold text-fg">{request?.host}</span> wants to connect
            to Billable{request?.profileName ? <> ({request.profileName} profile)</> : null}.
          </p>
        )}
        <p>
          Billable will send a summary of your jobs, hours, invoices and expenses to
          {request?.account ? ' that account' : ' the Content HQ account signed in here'}. From your phone you can then track time, log hours, add expenses
          and record payments.
        </p>
        <p className="text-fg-3 text-xs">
          Client emails, addresses, invoice lines and payment details stay on this Mac. You can
          disconnect any time in Settings.
        </p>
      </div>
    </Modal>
  )
}
