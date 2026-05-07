#!/usr/bin/env node
/**
 * Release script for Billable.
 *
 * Usage:
 *   npm run release            # builds + uploads the current package.json version
 *   npm run release -- patch   # bumps patch version first, then builds + uploads
 *   npm run release -- minor
 *   npm run release -- major
 *
 * Steps:
 *  1. Optionally bump version (npm version <bump>) — also creates a git tag
 *  2. Run `npm run build` to produce a fresh DMG
 *  3. Use `gh release create` to publish the DMG with auto-generated notes
 *  4. Push commits + tag to origin
 *
 * Pre-reqs: gh CLI authenticated, working tree clean.
 */

import { execSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const ROOT = path.resolve(__dirname, '..')

function run(cmd, opts = {}) {
  console.log(`\n$ ${cmd}`)
  return execSync(cmd, { stdio: 'inherit', cwd: ROOT, ...opts })
}

function runQuiet(cmd) {
  return execSync(cmd, { cwd: ROOT, stdio: ['ignore', 'pipe', 'pipe'] }).toString().trim()
}

function fail(msg) {
  console.error(`\n❌ ${msg}\n`)
  process.exit(1)
}

// ----- 0. Sanity checks -----
const bump = process.argv[2] // 'patch' | 'minor' | 'major' | undefined

try {
  runQuiet('git rev-parse --is-inside-work-tree')
} catch {
  fail('Not inside a git repo')
}

const dirty = runQuiet('git status --porcelain')
if (dirty && !bump) {
  fail('Working tree is dirty. Commit or stash before releasing (or pass a bump arg to bump version automatically).')
}

try {
  runQuiet('gh auth status')
} catch {
  fail('gh CLI is not authenticated. Run `gh auth login` first.')
}

// ----- 1. Optional version bump -----
if (bump) {
  if (!['patch', 'minor', 'major'].includes(bump)) {
    fail(`Unknown version bump: ${bump}. Use patch | minor | major.`)
  }
  // npm version creates a git tag and commit by default
  run(`npm version ${bump} -m "chore: release v%s"`)
}

const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf-8'))
const version = pkg.version
const tag = `v${version}`

console.log(`\n📦 Releasing Billable ${tag}…`)

// ----- 2. Verify the tag exists locally; create one if needed -----
try {
  runQuiet(`git rev-parse ${tag}`)
} catch {
  console.log(`\nTag ${tag} doesn't exist yet — creating it from HEAD.`)
  run(`git tag ${tag}`)
}

// ----- 3. Build -----
run('npm run build')

// ----- 4. Locate the DMG -----
const releaseDir = path.join(ROOT, 'release')
const dmgs = fs.readdirSync(releaseDir).filter(f => f.endsWith('.dmg'))
if (dmgs.length === 0) {
  fail('No .dmg found in release/ — build may have failed silently.')
}

// Prefer arch-specific arm64 DMG if present
const armDmg = dmgs.find(f => /arm64/i.test(f))
const dmgFile = armDmg || dmgs[0]
const dmgPath = path.join(releaseDir, dmgFile)
console.log(`\n📦 Built ${dmgFile} (${(fs.statSync(dmgPath).size / 1024 / 1024).toFixed(1)} MB)`)

// ----- 5. Push commits + tags so the release ref exists on origin -----
run('git push')
run('git push --tags')

// ----- 6. Create the GitHub release with the DMG attached -----
// --generate-notes uses GitHub's auto-generated notes from commit history.
// If a release already exists for this tag (e.g. partial run), upload the asset instead.
let releaseExists = false
try {
  runQuiet(`gh release view ${tag}`)
  releaseExists = true
} catch {
  // not yet published
}

if (releaseExists) {
  console.log(`\n📤 Release ${tag} already exists — uploading DMG.`)
  run(`gh release upload ${tag} "${dmgPath}" --clobber`)
} else {
  run(`gh release create ${tag} "${dmgPath}" --title "Billable ${tag}" --generate-notes`)
}

console.log(`\n✅ Released ${tag}`)
console.log(`   https://github.com/bulledh-beep/Billable/releases/tag/${tag}\n`)
