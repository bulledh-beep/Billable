import { app, BrowserWindow, shell, net } from 'electron'
import path from 'path'
import fs from 'fs'
import { spawn } from 'child_process'

const REPO_OWNER = 'bulledh-beep'
const REPO_NAME = 'Billable'
const RELEASES_API = `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest`

export interface UpdateStatus {
  current_version: string
  latest_version: string | null
  update_available: boolean
  release_url: string | null
  release_notes: string | null
  download_url: string | null
  download_size_bytes: number | null
  published_at: string | null
  last_checked_at: string
}

interface CachedStatus {
  data: UpdateStatus
  cached_at: number
}

let lastStatus: CachedStatus | null = null
const CACHE_MS = 5 * 60 * 1000 // 5 minutes — cache so the renderer can ask freely

/**
 * Compare two semver-ish version strings ("1.2.3" or "1.2.3-beta.1").
 * Returns 1 if a > b, -1 if a < b, 0 if equal. Pre-releases sort lower than the
 * matching release (1.2.3-beta < 1.2.3). Good enough for our use.
 */
function compareVersions(a: string, b: string): number {
  const parse = (v: string) => {
    const [main, pre] = v.replace(/^v/i, '').split('-', 2)
    const parts = main.split('.').map(p => parseInt(p, 10) || 0)
    while (parts.length < 3) parts.push(0)
    return { parts, pre: pre ?? '' }
  }
  const A = parse(a)
  const B = parse(b)
  for (let i = 0; i < 3; i++) {
    if (A.parts[i] !== B.parts[i]) return A.parts[i] > B.parts[i] ? 1 : -1
  }
  // Equal main parts — compare pre-release tags (none > any pre)
  if (!A.pre && B.pre) return 1
  if (A.pre && !B.pre) return -1
  if (A.pre === B.pre) return 0
  return A.pre > B.pre ? 1 : -1
}

interface GitHubAsset {
  name: string
  browser_download_url: string
  size: number
}

interface GitHubRelease {
  tag_name: string
  name: string | null
  body: string | null
  html_url: string
  published_at: string
  prerelease: boolean
  draft: boolean
  assets: GitHubAsset[]
}

function fetchLatestRelease(): Promise<GitHubRelease> {
  return new Promise((resolve, reject) => {
    const request = net.request({
      method: 'GET',
      url: RELEASES_API,
      // GitHub requires a User-Agent header; net's defaults are fine but explicit is safer.
      redirect: 'follow',
    })
    request.setHeader('Accept', 'application/vnd.github+json')
    request.setHeader('User-Agent', `Billable/${app.getVersion()}`)

    request.on('response', (response) => {
      const chunks: Buffer[] = []
      response.on('data', (chunk: Buffer) => chunks.push(chunk))
      response.on('end', () => {
        const body = Buffer.concat(chunks).toString('utf-8')
        if (response.statusCode === 404) {
          reject(new Error('No releases published yet'))
          return
        }
        if (response.statusCode !== 200) {
          reject(new Error(`GitHub API ${response.statusCode}: ${body.slice(0, 200)}`))
          return
        }
        try {
          resolve(JSON.parse(body) as GitHubRelease)
        } catch (err) {
          reject(new Error(`Bad JSON from GitHub: ${err}`))
        }
      })
      response.on('error', reject)
    })
    request.on('error', reject)
    request.end()
  })
}

function pickDmgAsset(release: GitHubRelease): GitHubAsset | null {
  // Prefer arch-specific arm64 DMG, fall back to any DMG
  const arm64 = release.assets.find(a => /arm64.*\.dmg$/i.test(a.name))
  if (arm64) return arm64
  return release.assets.find(a => a.name.toLowerCase().endsWith('.dmg')) ?? null
}

export async function checkForUpdates(force = false): Promise<UpdateStatus> {
  if (!force && lastStatus && Date.now() - lastStatus.cached_at < CACHE_MS) {
    return lastStatus.data
  }

  const current = app.getVersion()
  const status: UpdateStatus = {
    current_version: current,
    latest_version: null,
    update_available: false,
    release_url: null,
    release_notes: null,
    download_url: null,
    download_size_bytes: null,
    published_at: null,
    last_checked_at: new Date().toISOString(),
  }

  try {
    const release = await fetchLatestRelease()
    if (release.draft || release.prerelease) {
      // Skip drafts/prereleases for now
    } else {
      const latestVer = release.tag_name.replace(/^v/i, '')
      status.latest_version = latestVer
      status.release_url = release.html_url
      status.release_notes = release.body
      status.published_at = release.published_at
      const asset = pickDmgAsset(release)
      if (asset) {
        status.download_url = asset.browser_download_url
        status.download_size_bytes = asset.size
      }
      status.update_available = compareVersions(latestVer, current) > 0
    }
  } catch (err) {
    // Cache a stale status so the UI can still show "couldn't reach GitHub"
    throw err
  }

  lastStatus = { data: status, cached_at: Date.now() }
  return status
}

export function getCachedStatus(): UpdateStatus | null {
  return lastStatus?.data ?? null
}

/**
 * Fetch release notes for a specific version tag. Used by the "What's New"
 * modal that appears the first time the user launches a newer version.
 */
export function getReleaseNotesForTag(version: string): Promise<{ name: string; body: string; html_url: string } | null> {
  return new Promise((resolve, reject) => {
    const tag = version.startsWith('v') ? version : `v${version}`
    const url = `https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases/tags/${tag}`
    const request = net.request({ method: 'GET', url, redirect: 'follow' })
    request.setHeader('Accept', 'application/vnd.github+json')
    request.setHeader('User-Agent', `Billable/${app.getVersion()}`)

    request.on('response', (response) => {
      const chunks: Buffer[] = []
      response.on('data', (chunk: Buffer) => chunks.push(chunk))
      response.on('end', () => {
        if (response.statusCode === 404) return resolve(null)
        if (response.statusCode !== 200) {
          return reject(new Error(`GitHub API ${response.statusCode}`))
        }
        try {
          const json = JSON.parse(Buffer.concat(chunks).toString('utf-8')) as GitHubRelease
          resolve({
            name: json.name || tag,
            body: json.body || '',
            html_url: json.html_url,
          })
        } catch (err) {
          reject(err)
        }
      })
      response.on('error', reject)
    })
    request.on('error', reject)
    request.end()
  })
}

/**
 * Download the DMG to ~/Downloads/ and open it (mounts in Finder).
 * Reports progress to the requesting window so the UI can show a progress bar.
 */
export async function downloadAndOpenUpdate(
  url: string,
  win?: BrowserWindow,
): Promise<string> {
  const filename = path.basename(new URL(url).pathname) || 'Billable-update.dmg'
  const downloadsDir = app.getPath('downloads')
  const filepath = path.join(downloadsDir, filename)

  await new Promise<void>((resolve, reject) => {
    const request = net.request({ method: 'GET', url, redirect: 'follow' })
    request.setHeader('User-Agent', `Billable/${app.getVersion()}`)

    request.on('response', (response) => {
      if (response.statusCode !== 200) {
        reject(new Error(`Download failed: HTTP ${response.statusCode}`))
        return
      }

      const total = parseInt(String(response.headers['content-length'] || '0'), 10) || 0
      let received = 0
      const stream = fs.createWriteStream(filepath)

      response.on('data', (chunk: Buffer) => {
        received += chunk.length
        stream.write(chunk)
        if (win && total > 0) {
          win.webContents.send('updater:progress', {
            received,
            total,
            percent: Math.round((received / total) * 100),
          })
        }
      })
      response.on('end', () => {
        stream.end()
        stream.on('finish', () => resolve())
        stream.on('error', reject)
      })
      response.on('error', err => {
        stream.destroy()
        reject(err)
      })
    })
    request.on('error', reject)
    request.end()
  })

  // Open the DMG so the user can drag-replace
  await shell.openPath(filepath)
  return filepath
}

/**
 * Locate the running .app bundle on disk. Returns null in dev (where we
 * don't run from a packaged bundle and shouldn't try to self-update).
 *
 * process.execPath in production: /path/to/Billable.app/Contents/MacOS/Billable
 * We walk up to the nearest *.app ancestor.
 */
export function getAppBundlePath(): string | null {
  if (!app.isPackaged) return null
  let p = process.execPath
  while (p && p !== '/' && p !== path.dirname(p)) {
    if (p.endsWith('.app')) return p
    p = path.dirname(p)
  }
  return null
}

/**
 * Download the DMG quietly to a temp file (no shell.openPath).
 * Same progress events as downloadAndOpenUpdate.
 */
async function downloadDmgQuiet(url: string, win?: BrowserWindow): Promise<string> {
  const filename = path.basename(new URL(url).pathname) || 'Billable-update.dmg'
  // Use temp dir so the file gets cleaned up by the helper after install
  const tmpDir = app.getPath('temp')
  const filepath = path.join(tmpDir, filename)

  await new Promise<void>((resolve, reject) => {
    const request = net.request({ method: 'GET', url, redirect: 'follow' })
    request.setHeader('User-Agent', `Billable/${app.getVersion()}`)

    request.on('response', (response) => {
      if (response.statusCode !== 200) {
        reject(new Error(`Download failed: HTTP ${response.statusCode}`))
        return
      }
      const total = parseInt(String(response.headers['content-length'] || '0'), 10) || 0
      let received = 0
      const stream = fs.createWriteStream(filepath)

      response.on('data', (chunk: Buffer) => {
        received += chunk.length
        stream.write(chunk)
        if (win && total > 0) {
          win.webContents.send('updater:progress', {
            received,
            total,
            percent: Math.round((received / total) * 100),
          })
        }
      })
      response.on('end', () => {
        stream.end()
        stream.on('finish', () => resolve())
        stream.on('error', reject)
      })
      response.on('error', err => {
        stream.destroy()
        reject(err)
      })
    })
    request.on('error', reject)
    request.end()
  })

  return filepath
}

/**
 * Helper script written to a temp file, then spawned detached just before
 * the parent app quits. It waits for Billable to fully exit, mounts the DMG,
 * replaces the running .app bundle in place, ejects, and relaunches.
 *
 * Args (passed positionally to bash):
 *   $1 = path to downloaded DMG
 *   $2 = path to Billable.app to replace
 */
const HELPER_SCRIPT = `#!/bin/bash
set +e

DMG="$1"
APP="$2"
LOG="\${TMPDIR:-/tmp}/billable-update.log"

echo "[$(date)] starting install helper" > "$LOG"
echo "DMG=$DMG" >> "$LOG"
echo "APP=$APP" >> "$LOG"

# Wait for any running Billable to exit (max ~15s)
for i in $(seq 1 50); do
  pgrep -x Billable >/dev/null 2>&1 || break
  sleep 0.3
done

# If still running, force-quit
if pgrep -x Billable >/dev/null 2>&1; then
  echo "force-quitting still-running Billable" >> "$LOG"
  pkill -x Billable 2>/dev/null
  sleep 1
fi

# Mount the DMG
MOUNT_OUT=$(hdiutil attach "$DMG" -nobrowse -noverify -noautoopen 2>&1)
echo "$MOUNT_OUT" >> "$LOG"
MOUNT=$(echo "$MOUNT_OUT" | grep -Eo "/Volumes/[^[:space:]]+" | tail -n1)

if [ -z "$MOUNT" ] || [ ! -d "$MOUNT" ]; then
  echo "mount failed" >> "$LOG"
  open "$APP"
  exit 1
fi

# Replace the .app bundle
if ! rm -rf "$APP"; then
  echo "could not remove old app" >> "$LOG"
  hdiutil detach "$MOUNT" -quiet 2>/dev/null
  open "$APP" 2>/dev/null
  exit 1
fi

if ! cp -R "$MOUNT/Billable.app" "$APP"; then
  echo "copy failed" >> "$LOG"
  hdiutil detach "$MOUNT" -quiet 2>/dev/null
  exit 1
fi

# Strip quarantine so Gatekeeper doesn't whine on relaunch
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

# Re-register with Launch Services so Spotlight/icon caches update
LSREG=/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister
if [ -x "$LSREG" ]; then
  "$LSREG" -f "$APP" >/dev/null 2>&1 || true
fi

# Eject DMG and clean up
hdiutil detach "$MOUNT" -quiet 2>/dev/null || true
rm -f "$DMG"

# Relaunch
open "$APP"
echo "[$(date)] install complete, relaunched" >> "$LOG"

# Self-cleanup
rm -f "$0"
`

/**
 * Silent install: downloads the DMG, writes a detached helper script that
 * waits for the app to exit, replaces the bundle, and relaunches. Caller
 * should app.quit() after this resolves so the helper can take over.
 */
export async function installUpdate(url: string, win?: BrowserWindow): Promise<void> {
  const appBundle = getAppBundlePath()
  if (!appBundle) {
    throw new Error('Could not locate the Billable.app bundle. Use Download instead.')
  }
  if (!fs.existsSync(appBundle)) {
    throw new Error(`App bundle not found at ${appBundle}`)
  }

  // Sanity: the parent of the bundle should be writable so we can replace it.
  // /Applications is writable by the current user on personal Macs.
  const parentDir = path.dirname(appBundle)
  try {
    fs.accessSync(parentDir, fs.constants.W_OK)
  } catch {
    throw new Error(`Cannot write to ${parentDir} — install location requires admin access. Use Download instead.`)
  }

  // 1. Download DMG quietly to temp
  const dmgPath = await downloadDmgQuiet(url, win)

  // 2. Write the helper script to a temp file
  const helperPath = path.join(app.getPath('temp'), `billable-update-${Date.now()}.sh`)
  fs.writeFileSync(helperPath, HELPER_SCRIPT, { mode: 0o755 })

  // 3. Spawn the helper completely detached so it survives our exit
  const child = spawn('/bin/bash', [helperPath, dmgPath, appBundle], {
    detached: true,
    stdio: 'ignore',
  })
  child.unref()
}
