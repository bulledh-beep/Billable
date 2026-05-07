#!/usr/bin/env bash
#
# Billable installer — downloads the latest release from GitHub and installs
# it to /Applications (or $INSTALL_DIR if set).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/bulledh-beep/Billable/main/install.sh | bash
#
# Or, with a custom install location:
#   INSTALL_DIR=~/Applications curl -fsSL ... | bash
#
# What this does:
#   1. Detects your CPU architecture (arm64 / x86_64)
#   2. Asks GitHub for the latest release
#   3. Downloads the matching .dmg to a temp file
#   4. Quits any running Billable
#   5. Mounts the DMG, copies Billable.app into INSTALL_DIR (replacing any
#      existing install at that path)
#   6. Strips the quarantine attribute so Gatekeeper doesn't complain
#   7. Ejects the DMG, deletes the temp file
#   8. Launches the new Billable
#
# This script is safe to re-run — it always installs the latest version.

set -euo pipefail

# ---------- Configuration ----------
REPO_OWNER="bulledh-beep"
REPO_NAME="Billable"
APP_NAME="Billable.app"
INSTALL_DIR="${INSTALL_DIR:-/Applications}"

# ---------- Pretty output ----------
GREEN=$(printf '\033[32m')
RED=$(printf '\033[31m')
YELLOW=$(printf '\033[33m')
BOLD=$(printf '\033[1m')
DIM=$(printf '\033[2m')
RESET=$(printf '\033[0m')

step()  { printf "%s==>%s %s\n" "$GREEN" "$RESET" "$1"; }
warn()  { printf "%s!%s %s\n" "$YELLOW" "$RESET" "$1" >&2; }
fail()  { printf "%s✗%s %s\n" "$RED" "$RESET" "$1" >&2; exit 1; }
note()  { printf "%s   %s%s\n" "$DIM" "$1" "$RESET"; }

# ---------- Pre-flight checks ----------
[[ "$(uname)" == "Darwin" ]] || fail "Billable is macOS only."

ARCH="$(uname -m)"
case "$ARCH" in
  arm64) DMG_PATTERN="arm64" ;;
  x86_64)
    warn "Intel Macs aren't currently supported by the published release (only Apple Silicon arm64 builds are shipped)."
    warn "You can still build from source: https://github.com/$REPO_OWNER/$REPO_NAME#build-from-source"
    fail "No Intel DMG available."
    ;;
  *) fail "Unsupported architecture: $ARCH" ;;
esac

for cmd in curl hdiutil sed grep; do
  command -v "$cmd" >/dev/null 2>&1 || fail "Missing required command: $cmd"
done

# ---------- Find the latest release ----------
step "Looking up the latest Billable release…"

API_URL="https://api.github.com/repos/$REPO_OWNER/$REPO_NAME/releases/latest"
RELEASE_JSON="$(curl -fsSL "$API_URL")" || fail "Couldn't reach GitHub. Check your internet connection."

# Parse without jq dependency. Extracts the browser_download_url for an asset
# whose name contains the arch pattern and ends in .dmg.
DMG_URL="$(printf '%s' "$RELEASE_JSON" | \
  grep -oE '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]+"' | \
  sed -E 's/.*"([^"]+)"/\1/' | \
  grep -E "${DMG_PATTERN}.*\\.dmg\$" | head -n 1)"

[[ -n "$DMG_URL" ]] || fail "No matching DMG found in the latest release."

VERSION="$(printf '%s' "$RELEASE_JSON" | \
  grep -oE '"tag_name"[[:space:]]*:[[:space:]]*"[^"]+"' | \
  head -n 1 | sed -E 's/.*"([^"]+)"/\1/')"

note "Found $VERSION  ·  $(basename "$DMG_URL")"

# ---------- Download ----------
TMP_DMG="$(mktemp -t billable-install).dmg"
trap 'rm -f "$TMP_DMG"' EXIT

step "Downloading…"
curl -fL --progress-bar "$DMG_URL" -o "$TMP_DMG" || fail "Download failed."

# ---------- Quit running Billable ----------
if pgrep -x "Billable" >/dev/null 2>&1; then
  step "Quitting running Billable…"
  osascript -e 'quit app "Billable"' >/dev/null 2>&1 || true
  # Wait up to 5s for it to actually exit
  for _ in 1 2 3 4 5; do
    pgrep -x "Billable" >/dev/null 2>&1 || break
    sleep 1
  done
fi

# ---------- Mount the DMG ----------
step "Mounting DMG…"
MOUNT_OUTPUT="$(hdiutil attach "$TMP_DMG" -nobrowse -noverify -noautoopen 2>&1)" \
  || fail "Failed to mount DMG."
MOUNT_POINT="$(printf '%s' "$MOUNT_OUTPUT" | grep -oE '/Volumes/[^[:space:]]+$' | head -n 1)"
[[ -d "$MOUNT_POINT" ]] || fail "Couldn't determine mount point from hdiutil output."

# Always eject on exit, even if subsequent steps fail
trap 'rm -f "$TMP_DMG"; hdiutil detach "$MOUNT_POINT" -quiet 2>/dev/null || true' EXIT

SRC_APP="$MOUNT_POINT/$APP_NAME"
[[ -d "$SRC_APP" ]] || fail "$APP_NAME not found inside the DMG."

# ---------- Install ----------
mkdir -p "$INSTALL_DIR" 2>/dev/null || true
if [[ ! -w "$INSTALL_DIR" ]]; then
  warn "$INSTALL_DIR isn't writable. Falling back to ~/Applications."
  INSTALL_DIR="$HOME/Applications"
  mkdir -p "$INSTALL_DIR"
fi

DEST_APP="$INSTALL_DIR/$APP_NAME"

if [[ -d "$DEST_APP" ]]; then
  step "Removing existing install at $DEST_APP…"
  rm -rf "$DEST_APP"
fi

step "Copying $APP_NAME to $INSTALL_DIR…"
cp -R "$SRC_APP" "$DEST_APP"

# ---------- Strip quarantine so Gatekeeper doesn't whine ----------
step "Removing quarantine attribute…"
xattr -dr com.apple.quarantine "$DEST_APP" 2>/dev/null || true

# ---------- Re-register with Launch Services so the icon, Spotlight, etc. work ----------
LSREG="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
if [[ -x "$LSREG" ]]; then
  "$LSREG" -f "$DEST_APP" >/dev/null 2>&1 || true
fi

# ---------- Launch ----------
step "Launching Billable $VERSION…"
open "$DEST_APP"

printf "\n%sBillable %s installed to %s%s\n\n" "$GREEN$BOLD" "$VERSION" "$DEST_APP" "$RESET"
note "Your data lives in ~/Library/Application Support/billable/ and is preserved across updates."
note "Future updates: open Billable → look for the update banner in the sidebar."
printf "\n"
