import { useEffect, useRef, useState } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { Plus, Check, Pencil, Trash2, Camera, ImageOff, ChevronsUpDown } from 'lucide-react'
import Modal from './Modal'
import ConfirmDialog from './ConfirmDialog'
import ProfileAvatar from './ProfileAvatar'
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
    <div ref={containerRef} className="relative px-2.5 shrink-0">
      <button
        onClick={() => setOpen(v => !v)}
        className={`w-full flex items-center gap-2 h-[30px] px-2 rounded-[6px] transition-colors ${open ? 'bg-fg/[0.08]' : 'hover:bg-fg/[0.05]'}`}
        title="Switch profile"
      >
        <ProfileAvatar profile={active} size="xs" className="!w-[18px] !h-[18px] !text-[9px]" />
        <span className="flex-1 min-w-0 text-left text-[13px] text-fg/85 truncate">{active.name}</span>
        <ChevronsUpDown className="w-3.5 h-3.5 text-fg-4" />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            transition={{ duration: 0.12 }}
            className="absolute left-2.5 right-2.5 bottom-full mb-1 z-30 rounded-[8px] bg-panel shadow-pop py-1 overflow-hidden"
          >
            <div className="px-3 pt-1 pb-1 text-2xs font-semibold text-fg-3">Profiles</div>
            {profiles.map(p => (
              <button
                key={p.id}
                onClick={() => requestSwitch(p.id)}
                className="w-full flex items-center gap-2.5 px-3 h-[26px] hover:bg-fg/[0.06] transition-colors"
              >
                <ProfileAvatar profile={p} size="xs" />
                <span className="text-sm text-fg flex-1 text-left truncate">{p.name}</span>
                {p.id === active.id && <Check className="w-3.5 h-3.5 text-accent-text" />}
              </button>
            ))}
            <div className="border-t border-line my-1 mx-2" />
            <button
              onClick={openCreate}
              className="w-full flex items-center gap-2.5 px-3 h-[26px] hover:bg-fg/[0.06] transition-colors"
            >
              <Plus className="w-3.5 h-3.5 text-fg-3" />
              <span className="text-sm text-fg">New profile…</span>
            </button>
            <button
              onClick={() => { setOpen(false); setShowManage(true) }}
              className="w-full flex items-center gap-2.5 px-3 h-[26px] hover:bg-fg/[0.06] transition-colors"
            >
              <Pencil className="w-3.5 h-3.5 text-fg-3" />
              <span className="text-sm text-fg">Manage profiles…</span>
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Create modal */}
      <Modal
        isOpen={showCreate}
        onClose={() => setShowCreate(false)}
        title="New profile"
        description="Each profile has its own clients, projects, time, invoices, expenses, and tax settings."
        size="sm"
        footer={
          <>
            <button onClick={() => setShowCreate(false)} className="btn-secondary">Cancel</button>
            <button onClick={handleCreate} className="btn-primary">Create and switch</button>
          </>
        }
      >
        <div className="space-y-4">
          <div>
            <label className="label">Name</label>
            <input
              className="input"
              value={newName}
              onChange={e => setNewName(e.target.value)}
              placeholder="Side business"
              autoFocus
              onKeyDown={e => { if (e.key === 'Enter') handleCreate() }}
            />
          </div>
          <div>
            <label className="label">Color</label>
            <div className="flex gap-2">
              {PROFILE_COLORS.map(c => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setNewColor(c)}
                  className={`w-7 h-7 rounded-full transition-transform ${newColor === c ? 'ring-2 ring-offset-2 ring-offset-panel ring-fg/40 scale-105' : 'hover:scale-105'}`}
                  style={{ backgroundColor: c }}
                  aria-label={`Color ${c}`}
                />
              ))}
            </div>
          </div>
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

  const handlePickAvatar = async (p: Profile) => {
    try {
      const updated = await window.api.profile.pickAvatar(p.id)
      if (updated) {
        await refresh()
        toast.success('Profile photo updated')
      }
    } catch (err: any) {
      toast.error(`Failed: ${err.message || err}`)
    }
  }

  const handleClearAvatar = async (p: Profile) => {
    try {
      await window.api.profile.clearAvatar(p.id)
      await refresh()
      toast.success('Profile photo removed')
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
      <Modal
        isOpen={open}
        onClose={onClose}
        title="Manage profiles"
        description="Deleting a profile permanently removes its data. The active profile can't be deleted."
        footer={<button onClick={onClose} className="btn-primary">Done</button>}
      >
        <div className="space-y-2">
          {localProfiles.map(p => (
            <div
              key={p.id}
              className="flex items-center gap-3 p-3 rounded-lg border border-line bg-panel-2/60"
            >
              {/* Avatar with hover overlay for change/remove */}
              <div className="relative group flex-shrink-0">
                <ProfileAvatar profile={p} size="md" />
                <button
                  onClick={() => handlePickAvatar(p)}
                  className="absolute inset-0 rounded-full bg-black/55 opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center"
                  title={p.avatar ? 'Change photo' : 'Add photo'}
                >
                  <Camera className="w-3.5 h-3.5 text-white" />
                </button>
                {p.avatar && (
                  <button
                    onClick={() => handleClearAvatar(p)}
                    className="absolute -top-1 -right-1 p-0.5 rounded-full bg-panel border border-line opacity-0 group-hover:opacity-100 transition-opacity hover:bg-red/15"
                    title="Remove photo"
                  >
                    <ImageOff className="w-2.5 h-2.5 text-fg-3" />
                  </button>
                )}
              </div>
              {editingId === p.id ? (
                <>
                  <input
                    className="input flex-1"
                    value={editName}
                    autoFocus
                    onChange={e => setEditName(e.target.value)}
                    onKeyDown={e => {
                      if (e.key === 'Enter') saveEdit()
                      if (e.key === 'Escape') setEditingId(null)
                    }}
                  />
                  <button onClick={saveEdit} className="btn-primary btn-sm">Save</button>
                  <button onClick={() => setEditingId(null)} className="btn-secondary btn-sm">Cancel</button>
                </>
              ) : (
                <>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-fg truncate flex items-center gap-2">
                      {p.name}
                      {p.id === active.id && (
                        <span className="badge bg-accent/12 text-accent-text">Active</span>
                      )}
                    </div>
                    <div className="flex gap-1 mt-1.5">
                      {PROFILE_COLORS.map(c => (
                        <button
                          key={c}
                          type="button"
                          onClick={() => handleColor(p, c)}
                          className={`w-3.5 h-3.5 rounded-full transition-all ${p.color === c ? 'ring-2 ring-offset-1 ring-offset-panel ring-fg/40' : 'opacity-60 hover:opacity-100'}`}
                          style={{ backgroundColor: c }}
                          title={c}
                        />
                      ))}
                    </div>
                  </div>
                  <button onClick={() => startEdit(p)} className="btn-icon-sm" title="Rename">
                    <Pencil />
                  </button>
                  <button
                    onClick={() => setDeleteId(p.id)}
                    disabled={p.id === active.id || localProfiles.length <= 1}
                    className="btn-icon-sm hover:text-red"
                    title={p.id === active.id ? 'Switch away from this profile to delete it' : 'Delete profile'}
                  >
                    <Trash2 />
                  </button>
                </>
              )}
            </div>
          ))}

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
