import { app } from 'electron'
import path from 'path'
import fs from 'fs'

export interface Profile {
  id: string
  name: string
  color: string
  created_at: string
}

interface ProfilesFile {
  version: 1
  active: string
  profiles: Profile[]
}

const PROFILES_DIRNAME = 'profiles'
const PROFILES_INDEX_FILENAME = 'profiles.json'
const DB_FILENAME = 'billable.db'
const LEGACY_DB_BASENAME = 'billable.db'

const DEFAULT_PROFILE_ID = 'default'
const DEFAULT_PROFILE_NAME = 'Default'
const DEFAULT_PROFILE_COLOR = '#F5A623'

function userDataDir() {
  return app.getPath('userData')
}

function profilesIndexPath() {
  return path.join(userDataDir(), PROFILES_INDEX_FILENAME)
}

function profilesRoot() {
  return path.join(userDataDir(), PROFILES_DIRNAME)
}

export function getProfileDir(profileId: string) {
  return path.join(profilesRoot(), profileId)
}

export function getProfileDbPath(profileId: string) {
  return path.join(getProfileDir(profileId), DB_FILENAME)
}

function ensureDir(p: string) {
  if (!fs.existsSync(p)) fs.mkdirSync(p, { recursive: true })
}

function readProfilesIndex(): ProfilesFile | null {
  try {
    const raw = fs.readFileSync(profilesIndexPath(), 'utf-8')
    return JSON.parse(raw) as ProfilesFile
  } catch {
    return null
  }
}

function writeProfilesIndex(data: ProfilesFile) {
  ensureDir(userDataDir())
  fs.writeFileSync(profilesIndexPath(), JSON.stringify(data, null, 2))
}

/**
 * One-time migration of the original single-DB layout into a Default profile.
 * Runs at startup before any DB is opened.
 */
export function migrateLegacyIfNeeded() {
  const indexPath = profilesIndexPath()
  if (fs.existsSync(indexPath)) return // already migrated

  const legacyDbPath = path.join(userDataDir(), LEGACY_DB_BASENAME)
  ensureDir(profilesRoot())
  ensureDir(getProfileDir(DEFAULT_PROFILE_ID))

  const targetDbPath = getProfileDbPath(DEFAULT_PROFILE_ID)

  if (fs.existsSync(legacyDbPath) && !fs.existsSync(targetDbPath)) {
    // Move the existing DB and any WAL/SHM siblings
    fs.renameSync(legacyDbPath, targetDbPath)
    for (const ext of ['-wal', '-shm']) {
      const sib = legacyDbPath + ext
      if (fs.existsSync(sib)) fs.renameSync(sib, targetDbPath + ext)
    }
  }

  const initial: ProfilesFile = {
    version: 1,
    active: DEFAULT_PROFILE_ID,
    profiles: [
      {
        id: DEFAULT_PROFILE_ID,
        name: DEFAULT_PROFILE_NAME,
        color: DEFAULT_PROFILE_COLOR,
        created_at: new Date().toISOString(),
      },
    ],
  }
  writeProfilesIndex(initial)
}

export function listProfiles(): Profile[] {
  const idx = readProfilesIndex()
  return idx?.profiles ?? []
}

export function getActiveProfileId(): string {
  const idx = readProfilesIndex()
  return idx?.active ?? DEFAULT_PROFILE_ID
}

export function getActiveProfile(): Profile | null {
  const idx = readProfilesIndex()
  if (!idx) return null
  return idx.profiles.find(p => p.id === idx.active) ?? null
}

export function setActiveProfileId(id: string) {
  const idx = readProfilesIndex()
  if (!idx) throw new Error('Profiles index missing')
  if (!idx.profiles.find(p => p.id === id)) {
    throw new Error(`Profile ${id} not found`)
  }
  idx.active = id
  writeProfilesIndex(idx)
}

function slugify(name: string) {
  return (name || 'profile')
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 32) || 'profile'
}

function randomSuffix() {
  return Math.random().toString(36).slice(2, 6)
}

function generateProfileId(name: string, existingIds: Set<string>) {
  const base = slugify(name)
  let id = base
  while (existingIds.has(id) || id === DEFAULT_PROFILE_ID) {
    id = `${base}-${randomSuffix()}`
  }
  return id
}

export function createProfile(name: string, color?: string): Profile {
  if (!name?.trim()) throw new Error('Name is required')
  const idx = readProfilesIndex()
  if (!idx) throw new Error('Profiles index missing')

  const existingIds = new Set(idx.profiles.map(p => p.id))
  const id = generateProfileId(name, existingIds)
  const profile: Profile = {
    id,
    name: name.trim(),
    color: color || DEFAULT_PROFILE_COLOR,
    created_at: new Date().toISOString(),
  }
  ensureDir(getProfileDir(id))
  idx.profiles.push(profile)
  writeProfilesIndex(idx)
  return profile
}

export function renameProfile(id: string, name: string): Profile {
  if (!name?.trim()) throw new Error('Name is required')
  const idx = readProfilesIndex()
  if (!idx) throw new Error('Profiles index missing')
  const profile = idx.profiles.find(p => p.id === id)
  if (!profile) throw new Error(`Profile ${id} not found`)
  profile.name = name.trim()
  writeProfilesIndex(idx)
  return profile
}

export function updateProfileColor(id: string, color: string): Profile {
  const idx = readProfilesIndex()
  if (!idx) throw new Error('Profiles index missing')
  const profile = idx.profiles.find(p => p.id === id)
  if (!profile) throw new Error(`Profile ${id} not found`)
  profile.color = color
  writeProfilesIndex(idx)
  return profile
}

export function deleteProfile(id: string) {
  const idx = readProfilesIndex()
  if (!idx) throw new Error('Profiles index missing')
  if (idx.profiles.length <= 1) {
    throw new Error('Cannot delete the last profile')
  }
  if (idx.active === id) {
    throw new Error('Switch to another profile before deleting this one')
  }
  idx.profiles = idx.profiles.filter(p => p.id !== id)
  writeProfilesIndex(idx)

  const dir = getProfileDir(id)
  if (fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true })
  }
}
