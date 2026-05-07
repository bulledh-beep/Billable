import { useEffect, useRef, useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { ChevronDown, Plus, Check, Pencil, Trash2 } from 'lucide-react'
import Modal from './Modal'
import ConfirmDialog from './ConfirmDialog'
import type { Profile } from '@shared/types'
import toast from 'react-hot-toast'

const PROFILE_COLORS = [
  '#F5A623', '#3498DB', '#2ECC71', '#9B59B6', '#E74C3C', '#1ABC9C', '#E67E22', '#EC407A',
]

export default function ProfileSwitcher({ isTimerRunning, onStopTimer }: {
  isTimerRunning: boolean
  onStopTimer: () => Promise<unknown>
}) {
  const [profiles, setProfiles] = useState<Profile[]>([])
  const [active, setActive] = useState<Profile | null>(null)
  const [open, setOpen] = useState(false)
  const [showCreate, setShowCreate] = useState(false)
  const [showManage, setShowManage] = useState(false)
  const [pendingSwitchId, setPendingSwitchId] = useState<string | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)

  // Create-form state
  const [newName, setNewName] = useState('')
  const [newColor, setNewColor] = useState(PROFILE_COLORS[0])

  useEffect(() => { load() }, [])

  // Close dropdown on outside click
  useEffect(() => {
    if (!open) return
    const onClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', onClick)
    return () => document.removeEventListener('mousedown', onClick)
  }, [open])

  const load = async () => {
    try {
      const res = await window.api.profile.list()
      setProfiles(res.profiles)
      setActive(res.active)
    } catch (err: any) {
      toast.error(`Failed to load profiles: ${err.message || err}`)
    }
  }

  const requestSwitch = (id: string) => {
    setOpen(false)
    if (id === active?.id) return
    if (isTimerRunning) {
      setPendingSwitchId(id)
      return
    }
    doSwitch(id)
  }

  const doSwitch = async (id: string) => {
    try {
      await window.api.profile.switch(id)
      // Window will reload — no further state work needed
    } catch (err: any) {
      toast.error(`Failed to switch: ${err.message || err}`)
    }
  }

  const confirmSwitchWithStop = async () => {
    const id = pendingSwitchId
    setPendingSwitchId(null)
    if (!id) return
    try {
      await onStopTimer()
    } catch {
      // continue anyway — switch handler also stops on the main side
    }
    await doSwitch(id)
  }

  const openCreate = () => {
    setOpen(false)
    setNewName('')
    setNewColor(PROFILE_COLORS[Math.floor(Math.random() * PROFILE_COLORS.length)])
    setShowCreate(true)
  }

  const handleCreate = async () => {
    if (!newName.trim()) return toast.error('Name is required')
    try {
      const created = await window.api.profile.create(newName.trim(), newColor)
      setShowCreate(false)
      // Switch immediately into the new profile
      await doSwitch(created.id)
    } catch (err: any) {
      toast.error(`Failed to create: ${err.message || err}`)
    }
  }

  if (!active) return null

  return (
    <div ref={containerRef} className="relative px-3 mb-2">
      <button
        onClick={() => setOpen(v => !v)}
        className="w-full flex items-center gap-2 p-2 rounded-lg hover:bg-surface-200/60 transition-colors"
      >
        <div
          className="w-2.5 h-2.5 rounded-full flex-shrink-0"
          style={{ backgroundColor: active.color }}
        />
        <div className="flex-1 min-w-0 text-left">
          <div className="text-[10px] uppercase tracking-wider text-text-tertiary leading-none">Profile</div>
          <div className="text-xs font-medium text-text-primary truncate mt-0.5">{active.name}</div>
        </div>
        <ChevronDown className={`w-3.5 h-3.5 text-text-tertiary transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.12 }}
            className="absolute left-3 right-3 top-full mt-1 z-30 rounded-lg border border-white/[0.08] bg-surface-100 shadow-xl py-1 overflow-hidden"
          >
            {profiles.map(p => (
              <button
                key={p.id}
                onClick={() => requestSwitch(p.id)}
                className="w-full flex items-center gap-2 px-3 py-2 hover:bg-surface-200/60 transition-colors"
              >
                <div className="w-2.5 h-2.5 rounded-full flex-shrink-0" style={{ backgroundColor: p.color }} />
                <span className="text-xs text-text-primary flex-1 text-left truncate">{p.name}</span>
                {p.id === active.id && <Check className="w-3.5 h-3.5 text-accent" />}
              </button>
            ))}
            <div className="border-t border-white/[0.04] my-1" />
            <button
              onClick={openCreate}
              className="w-full flex items-center gap-2 px-3 py-2 hover:bg-surface-200/60 transition-colors"
            >
              <Plus className="w-3.5 h-3.5 text-accent" />
              <span className="text-xs text-text-secondary">New Profile…</span>
            </button>
            <button
              onClick={() => { setOpen(false); setShowManage(true) }}
              className="w-full flex items-center gap-2 px-3 py-2 hover:bg-surface-200/60 transition-colors"
            >
              <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
              <span className="text-xs text-text-secondary">Manage Profiles…</span>
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Create modal */}
      <Modal isOpen={showCreate} onClose={() => setShowCreate(false)} title="New Profile" size="sm">
        <div className="space-y-4">
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Name</label>
            <input
              className="input-field"
              value={newName}
              onChange={e => setNewName(e.target.value)}
              placeholder="e.g. Side Business"
              autoFocus
              onKeyDown={e => { if (e.key === 'Enter') handleCreate() }}
            />
          </div>
          <div>
            <label className="text-xs font-medium text-text-secondary mb-1.5 block">Color</label>
            <div className="flex gap-2">
              {PROFILE_COLORS.map(c => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setNewColor(c)}
                  className={`w-8 h-8 rounded-lg transition-all ${newColor === c ? 'ring-2 ring-white/40 scale-110' : 'hover:scale-105'}`}
                  style={{ backgroundColor: c }}
                />
              ))}
            </div>
          </div>
          <div className="flex justify-end gap-3 pt-2">
            <button onClick={() => setShowCreate(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleCreate} className="btn-primary">
              Create & Switch
            </button>
          </div>
          <p className="text-[10px] text-text-tertiary leading-relaxed">
            Each profile is a fully isolated database — its own clients, projects, time entries, invoices, expenses, and tax settings. You can switch between them anytime.
          </p>
        </div>
      </Modal>

      {/* Manage modal */}
      <ManageProfilesModal
        open={showManage}
        profiles={profiles}
        active={active}
        onClose={() => { setShowManage(false); load() }}
      />

      {/* Confirm-stop-timer dialog */}
      <ConfirmDialog
        isOpen={pendingSwitchId !== null}
        onClose={() => setPendingSwitchId(null)}
        onConfirm={confirmSwitchWithStop}
        title="Stop timer and switch profile?"
        message="The active timer belongs to this profile. It will be stopped before switching. You can resume tracking after switching back."
      />
    </div>
  )
}

function ManageProfilesModal({
  open, profiles, active, onClose,
}: {
  open: boolean
  profiles: Profile[]
  active: Profile
  onClose: () => void
}) {
  const [editingId, setEditingId] = useState<string | null>(null)
  const [editName, setEditName] = useState('')
  const [deleteId, setDeleteId] = useState<string | null>(null)
  const [localProfiles, setLocalProfiles] = useState<Profile[]>(profiles)

  useEffect(() => { setLocalProfiles(profiles) }, [profiles, open])

  const refresh = async () => {
    const res = await window.api.profile.list()
    setLocalProfiles(res.profiles)
  }

  const startEdit = (p: Profile) => {
    setEditingId(p.id)
    setEditName(p.name)
  }

  const saveEdit = async () => {
    if (!editingId) return
    if (!editName.trim()) return toast.error('Name is required')
    try {
      await window.api.profile.rename(editingId, editName.trim())
      setEditingId(null)
      await refresh()
      toast.success('Profile renamed')
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    }
  }

  const handleColor = async (p: Profile, color: string) => {
    try {
      await window.api.profile.setColor(p.id, color)
      await refresh()
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    }
  }

  const confirmDelete = async () => {
    if (!deleteId) return
    try {
      await window.api.profile.delete(deleteId)
      setDeleteId(null)
      await refresh()
      toast.success('Profile deleted')
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
      setDeleteId(null)
    }
  }

  return (
    <>
      <Modal isOpen={open} onClose={onClose} title="Manage Profiles">
        <div className="space-y-2">
          {localProfiles.map(p => (
            <div
              key={p.id}
              className="flex items-center gap-3 p-3 rounded-lg bg-surface-200/40 border border-white/[0.04]"
            >
              <div className="flex flex-col gap-1">
                <div className="w-3 h-3 rounded-full" style={{ backgroundColor: p.color }} />
              </div>
              {editingId === p.id ? (
                <>
                  <input
                    className="input-field flex-1"
                    value={editName}
                    autoFocus
                    onChange={e => setEditName(e.target.value)}
                    onKeyDown={e => {
                      if (e.key === 'Enter') saveEdit()
                      if (e.key === 'Escape') setEditingId(null)
                    }}
                  />
                  <button onClick={saveEdit} className="btn-primary text-xs py-1.5">Save</button>
                  <button onClick={() => setEditingId(null)} className="btn-secondary text-xs py-1.5">Cancel</button>
                </>
              ) : (
                <>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm text-text-primary truncate flex items-center gap-2">
                      {p.name}
                      {p.id === active.id && (
                        <span className="text-[10px] uppercase tracking-wider text-accent px-1.5 py-0.5 rounded bg-accent/10">
                          active
                        </span>
                      )}
                    </div>
                    <div className="flex gap-1 mt-1.5">
                      {PROFILE_COLORS.map(c => (
                        <button
                          key={c}
                          type="button"
                          onClick={() => handleColor(p, c)}
                          className={`w-3.5 h-3.5 rounded-full transition-all ${p.color === c ? 'ring-1 ring-white/60 scale-110' : 'opacity-60 hover:opacity-100'}`}
                          style={{ backgroundColor: c }}
                          title={c}
                        />
                      ))}
                    </div>
                  </div>
                  <button
                    onClick={() => startEdit(p)}
                    className="p-1.5 hover:bg-surface-300 rounded-lg transition-colors"
                    title="Rename"
                  >
                    <Pencil className="w-3.5 h-3.5 text-text-tertiary" />
                  </button>
                  <button
                    onClick={() => setDeleteId(p.id)}
                    disabled={p.id === active.id || localProfiles.length <= 1}
                    className="p-1.5 hover:bg-red-500/10 rounded-lg transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                    title={p.id === active.id ? 'Switch away from this profile to delete it' : 'Delete profile'}
                  >
                    <Trash2 className="w-3.5 h-3.5 text-red-400" />
                  </button>
                </>
              )}
            </div>
          ))}

          <div className="flex justify-end gap-3 pt-3">
            <button onClick={onClose} className="btn-secondary">Done</button>
          </div>

          <p className="text-[10px] text-text-tertiary leading-relaxed pt-1">
            Deleting a profile permanently removes its database — all clients, projects, time entries, invoices, and expenses. This cannot be undone.
          </p>
        </div>
      </Modal>

      <ConfirmDialog
        isOpen={deleteId !== null}
        onClose={() => setDeleteId(null)}
        onConfirm={confirmDelete}
        title="Delete profile?"
        message="This permanently deletes all data in this profile. This cannot be undone."
      />
    </>
  )
}
