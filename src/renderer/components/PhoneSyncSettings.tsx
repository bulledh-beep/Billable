import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import toast from 'react-hot-toast'
import { Row, Section } from './SettingsLayout'
import ConfirmDialog from './ConfirmDialog'
import { formatRelative } from '../utils/format'

interface PhoneStatus {
  connected: boolean
  state: 'off' | 'syncing' | 'ok' | 'offline' | 'unavailable' | 'revoked' | 'mismatch' | 'error'
  host: string | null
  profileName: string | null
  connectedAt: string | null
  lastSyncAt: string | null
  message: string | null
}

interface Activity {
  id: string
  kind: string
  status: 'applied' | 'rejected'
  summary: string
  message: string | null
  applied_at: string
}

interface LoginItem { available: boolean; enabled: boolean; needsApproval: boolean }

const DOT: Record<PhoneStatus['state'], string> = {
  ok: 'bg-green',
  syncing: 'bg-blue',
  offline: 'bg-amber',
  unavailable: 'bg-amber',
  error: 'bg-red',
  mismatch: 'bg-red',
  revoked: 'bg-gray',
  off: 'bg-gray',
}

/** Settings → Billable on your phone: the Content HQ connection, open at login, and recent phone changes. */
export default function PhoneSyncSettings() {
  const navigate = useNavigate()
  const [status, setStatus] = useState<PhoneStatus | null>(null)
  const [activity, setActivity] = useState<Activity[]>([])
  const [login, setLogin] = useState<LoginItem | null>(null)
  const [confirmOff, setConfirmOff] = useState(false)
  const [, setNow] = useState(0)

  useEffect(() => {
    window.api.phone.status().then(setStatus)
    window.api.phone.openAtLogin().then(setLogin)
    const loadActivity = () => window.api.phone.activity(8).then(setActivity)
    loadActivity()
    const offStatus = window.api.on('phone:status', (s: PhoneStatus) => setStatus(s))
    const offApplied = window.api.on('phone:applied', loadActivity)
    // Keep "Last synced 12s ago" current
    const tick = setInterval(() => setNow(n => n + 1), 15_000)
    return () => { offStatus?.(); offApplied?.(); clearInterval(tick) }
  }, [])

  if (!status) return null

  const disconnect = async () => {
    const s = await window.api.phone.disconnect()
    setStatus(s)
    if (s.remoteDisconnected) toast.success('Disconnected. Content HQ deleted its copy of your Billable data.')
    else toast('Disconnected on this Mac, but Content HQ couldn’t be reached. Disconnect in Content HQ’s settings too, so it deletes its copy.', { duration: 7000 })
  }

  const toggleLogin = async (enabled: boolean) => {
    setLogin(await window.api.phone.setOpenAtLogin(enabled))
  }

  const line = (() => {
    if (!status.connected) return null
    switch (status.state) {
      case 'ok': return `Syncing with ${status.host}${status.lastSyncAt ? ` · last synced ${formatRelative(status.lastSyncAt).toLowerCase()}` : ''}`
      case 'syncing': return `Connecting to ${status.host}…`
      default: return status.message || `Connected to ${status.host}`
    }
  })()

  return (
    <Section title="Billable on your phone" description="Use Billable from Content HQ on your iPhone. Your Mac keeps the real records.">
      <Row
        label="Connection"
        help={status.connected
          ? 'Changes on your phone reach this Mac within about 20 seconds while Billable is open, or as soon as your Mac wakes up.'
          : 'In Content HQ, open Settings → Billable and click Connect this Mac.'}
      >
        {status.connected ? (
          <div className="space-y-3">
            <div className="flex items-start gap-2 text-sm text-fg">
              <span className={`mt-[7px] w-2 h-2 rounded-full shrink-0 ${DOT[status.state]}`} aria-hidden="true" />
              <span data-testid="phone-status">{line}</span>
            </div>
            {status.profileName && (
              <div className="text-xs text-fg-3">Linked to the {status.profileName} profile.</div>
            )}
            <div className="flex gap-2">
              <button className="btn-secondary" onClick={async () => setStatus(await window.api.phone.syncNow())}>Sync now</button>
              <button className="btn-ghost" onClick={() => setConfirmOff(true)}>Disconnect</button>
            </div>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="flex items-start gap-2 text-sm text-fg">
              <span className={`mt-[7px] w-2 h-2 rounded-full shrink-0 ${DOT[status.state]}`} aria-hidden="true" />
              <span data-testid="phone-status">{status.state === 'revoked' && status.message ? status.message : 'Not connected'}</span>
            </div>
            <button className="btn-secondary" onClick={() => navigate('/content')}>Open Content HQ</button>
          </div>
        )}
      </Row>

      <Row
        label="Open at login"
        help="Billable has to be running to pick up changes from your phone. This starts it when your Mac starts."
      >
        {login?.available ? (
          <div className="space-y-1.5">
            <label className="inline-flex items-center gap-2 text-sm text-fg cursor-pointer">
              <input type="checkbox" checked={login.enabled} onChange={e => toggleLogin(e.target.checked)} />
              Open Billable when I log in
            </label>
            {login.needsApproval && (
              <div className="text-xs text-amber">Allow Billable in System Settings → General → Login Items.</div>
            )}
          </div>
        ) : (
          <div className="text-sm text-fg-3 pt-[5px]">Available in the installed app.</div>
        )}
      </Row>

      <Row label="Recent changes from your phone">
        {activity.length === 0 ? (
          <div className="text-sm text-fg-3 pt-[5px]">Nothing yet.</div>
        ) : (
          <ul className="space-y-2 pt-[3px]" data-testid="phone-activity">
            {activity.map(a => (
              <li key={a.id} className="text-sm">
                <div className="flex items-baseline justify-between gap-3">
                  <span className={a.status === 'rejected' ? 'text-fg-2' : 'text-fg'}>
                    {a.status === 'rejected' ? `Couldn’t apply: ${a.summary.toLowerCase()}` : a.summary}
                  </span>
                  <span className="text-xs text-fg-3 shrink-0">{formatRelative(a.applied_at)}</span>
                </div>
                {a.message && (
                  <div className={`text-xs mt-0.5 ${a.status === 'rejected' ? 'text-red' : 'text-fg-3'}`}>{a.message}</div>
                )}
              </li>
            ))}
          </ul>
        )}
      </Row>

      <ConfirmDialog
        isOpen={confirmOff}
        onClose={() => setConfirmOff(false)}
        onConfirm={disconnect}
        title="Disconnect your phone?"
        message="Content HQ will stop getting updates from Billable and delete its copy. Your records on this Mac aren’t touched."
        confirmText="Disconnect"
      />
    </Section>
  )
}
