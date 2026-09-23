import { useEffect, useState } from 'react'
import { AlertTriangle } from 'lucide-react'
import Modal from './Modal'
import {
  formatDurationShort, parseDurationInput, toLocalISODate, todayISO,
} from '../utils/format'
import { notifyBillingChanged } from '../utils/events'
import type { Project, TimeEntry } from '@shared/types'
import toast from 'react-hot-toast'

const pad = (n: number) => String(n).padStart(2, '0')
const hhmm = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`

/** Minutes from date+start to date+end; an end at or before start rolls to the next day. */
export function minutesBetween(date: string, start: string, end: string): number {
  const s = new Date(`${date}T${start}:00`).getTime()
  let e = new Date(`${date}T${end}:00`).getTime()
  if (e <= s) e += 86_400_000
  return Math.round((e - s) / 60_000)
}

function endFromDuration(date: string, start: string, mins: number): string {
  const s = new Date(`${date}T${start}:00`)
  s.setMinutes(s.getMinutes() + mins)
  return hhmm(s)
}

/** True when an entry sits on an invoice that has gone out (or been paid). */
export function isLockedEntry(e?: TimeEntry | null) {
  return !!e && (e.billing_state === 'sent' || e.billing_state === 'overdue' || e.billing_state === 'paid' || e.billing_state === 'invoiced')
}

interface EditorForm {
  project_id: number
  description: string
  date: string
  start: string
  end: string
  durationText: string
  is_billable: boolean
}

interface EntryEditorProps {
  mode: 'add' | 'edit' | null
  entry?: TimeEntry | null
  projects: Project[]
  defaultDate?: string
  defaultProjectId?: number
  onClose: () => void
  onSaved: () => void
}

export default function EntryEditor({ mode, entry, projects, defaultDate, defaultProjectId, onClose, onSaved }: EntryEditorProps) {
  const [form, setForm] = useState<EditorForm>({
    project_id: 0, description: '', date: todayISO(), start: '09:00', end: '10:00', durationText: '1h', is_billable: true,
  })
  const [saving, setSaving] = useState(false)

  // Fill the form once each time the editor opens. Keyed on the opening
  // itself, so parent re-renders (like a ticking timer) never wipe your typing.
  const openKey = mode ? `${mode}:${entry?.id ?? ''}` : null
  useEffect(() => {
    if (mode === 'add') {
      setForm({
        project_id: defaultProjectId || projects[0]?.id || 0,
        description: '',
        date: defaultDate || todayISO(),
        start: '09:00',
        end: '10:00',
        durationText: formatDurationShort(60),
        is_billable: true,
      })
    } else if (mode === 'edit' && entry) {
      const s = new Date(entry.start_time)
      const e = entry.end_time ? new Date(entry.end_time) : new Date(s.getTime() + entry.duration_minutes * 60_000)
      // Paused time isn't in the duration, so derive the end from the duration
      const endShown = Math.abs((e.getTime() - s.getTime()) / 60_000 - entry.duration_minutes) > 1
        ? new Date(s.getTime() + entry.duration_minutes * 60_000)
        : e
      setForm({
        project_id: entry.project_id,
        description: entry.description || '',
        date: toLocalISODate(s),
        start: hhmm(s),
        end: hhmm(endShown),
        durationText: formatDurationShort(entry.duration_minutes),
        is_billable: !!entry.is_billable,
      })
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openKey])

  // Projects can arrive after the editor opens; pick one if none is set yet
  useEffect(() => {
    if (mode === 'add' && !form.project_id && projects.length) {
      setForm(f => ({ ...f, project_id: defaultProjectId || projects[0].id }))
    }
  }, [mode, projects, form.project_id, defaultProjectId])

  // Start or end changes → duration follows. Duration changes → end follows.
  const setTimes = (patch: Partial<Pick<EditorForm, 'date' | 'start' | 'end'>>) => setForm(f => {
    const next = { ...f, ...patch }
    return { ...next, durationText: formatDurationShort(minutesBetween(next.date, next.start, next.end)) }
  })
  const setDurationText = (text: string) => setForm(f => {
    const mins = parseDurationInput(text)
    if (mins == null || mins <= 0) return { ...f, durationText: text }
    return { ...f, durationText: text, end: endFromDuration(f.date, f.start, mins) }
  })

  const minutes = minutesBetween(form.date, form.start, form.end)
  const durationInvalid = parseDurationInput(form.durationText) == null
  const invalid = durationInvalid || minutes <= 0 || (mode === 'add' && !form.project_id)

  const save = async () => {
    if (invalid) return
    setSaving(true)
    try {
      const startDate = new Date(`${form.date}T${form.start}:00`)
      const endDate = new Date(startDate.getTime() + minutes * 60_000)
      const payload = {
        description: form.description,
        start_time: startDate.toISOString(),
        end_time: endDate.toISOString(),
        duration_minutes: minutes,
        is_billable: form.is_billable ? 1 : 0,
      }
      if (mode === 'edit' && entry) {
        await window.api.time.update(entry.id, payload)
        toast.success('Entry updated')
      } else {
        await window.api.time.create({ ...payload, project_id: form.project_id })
        toast.success('Time added')
      }
      notifyBillingChanged()
      onSaved()
    } catch (err: any) {
      toast.error(`Couldn't save: ${err.message || err}`)
    } finally {
      setSaving(false)
    }
  }

  const onInvoice = mode === 'edit' && entry?.invoice_id
  const locked = mode === 'edit' && isLockedEntry(entry)

  return (
    <Modal
      isOpen={mode !== null}
      onClose={onClose}
      title={mode === 'edit' ? 'Edit time entry' : 'Add time'}
      footer={
        <>
          <button onClick={onClose} className="btn-secondary">Cancel</button>
          <button onClick={save} disabled={saving || invalid} className="btn-primary">
            {saving ? 'Saving…' : mode === 'edit' ? 'Save changes' : 'Add time'}
          </button>
        </>
      }
    >
      <div className="space-y-4" onKeyDown={e => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) save() }}>
        {onInvoice && (
          <div className={`flex gap-2.5 p-3 rounded-lg text-sm ${locked ? 'bg-amber/10 text-fg-2' : 'bg-fg/[0.04] text-fg-2'}`}>
            <AlertTriangle className={`w-4 h-4 shrink-0 mt-px ${locked ? 'text-amber' : 'text-fg-3'}`} />
            <p>
              {locked
                ? <>This time is on <b className="text-fg">{entry?.invoice_number}</b>, which has been {entry?.billing_state === 'paid' ? 'paid' : 'sent'}. Changing it here won't change the invoice.</>
                : <>This time is on draft <b className="text-fg">{entry?.invoice_number}</b>. After saving, open the draft and update its lines.</>}
            </p>
          </div>
        )}

        {mode === 'add' ? (
          <div>
            <label className="label">Project</label>
            <select className="input" value={form.project_id} onChange={e => setForm(f => ({ ...f, project_id: parseInt(e.target.value) }))}>
              {projects.length === 0 && <option value={0}>No active projects</option>}
              {projects.map(p => <option key={p.id} value={p.id}>{p.name}{p.client_name ? ` · ${p.client_name}` : ''}</option>)}
            </select>
          </div>
        ) : entry && (
          <div className="flex items-center gap-2 text-sm">
            <span className="w-2 h-2 rounded-full" style={{ backgroundColor: entry.project_color }} />
            <span className="font-medium text-fg">{entry.project_name}</span>
            {entry.client_name && <span className="text-fg-3">· {entry.client_name}</span>}
          </div>
        )}

        <div>
          <label className="label">Description</label>
          <input
            className="input"
            value={form.description}
            onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
            placeholder="What did you work on?"
            autoFocus={mode === 'add'}
          />
        </div>

        <div className="grid grid-cols-[1.25fr_1.1fr_1.1fr_0.8fr] gap-3">
          <div>
            <label className="label">Date</label>
            <input className="input" type="date" value={form.date} onChange={e => setTimes({ date: e.target.value })} />
          </div>
          <div>
            <label className="label">Start</label>
            <input className="input" type="time" value={form.start} onChange={e => setTimes({ start: e.target.value })} />
          </div>
          <div>
            <label className="label">End</label>
            <input className="input" type="time" value={form.end} onChange={e => setTimes({ end: e.target.value })} />
          </div>
          <div>
            <label className="label">Duration</label>
            <input
              className={`input num ${durationInvalid ? '!border-red/60' : ''}`}
              value={form.durationText}
              onChange={e => setDurationText(e.target.value)}
              placeholder="2h 30m"
            />
          </div>
        </div>

        <div className="flex items-center justify-between -mt-1">
          <p className="text-xs text-fg-3">
            {durationInvalid ? 'Try 2h 30m, 1.5h, 2:30, or minutes' : 'An end time before the start rolls to the next day'}
          </p>
          <div className="flex gap-1">
            {[15, 30, 60].map(m => (
              <button
                key={m}
                type="button"
                onClick={() => setDurationText(formatDurationShort(minutes + m))}
                className="btn-ghost btn-sm num"
              >
                +{m >= 60 ? '1h' : `${m}m`}
              </button>
            ))}
          </div>
        </div>

        <label className="flex items-center gap-2.5 cursor-pointer select-none w-fit">
          <input
            type="checkbox"
            checked={form.is_billable}
            disabled={!!onInvoice}
            onChange={e => setForm(f => ({ ...f, is_billable: e.target.checked }))}
          />
          <span className="text-sm text-fg">Billable</span>
          {onInvoice && <span className="text-xs text-fg-3">· already on an invoice</span>}
        </label>
      </div>
    </Modal>
  )
}
