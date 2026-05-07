#!/usr/bin/env bash
#
# Billable uninstaller — removes the app and (optionally) all your data.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/bulledh-beep/Billable/main/uninstall.sh | bash
#
# This script:
#   1. Quits any running Billable
#   2. Finds and offers to remove every Billable.app it can locate
#   3. Asks (y/N) before deleting your local data
#      (~/Library/Application Support/billable)

set -euo pipefail

GREEN=$(printf '\033[32m'); RED=$(printf '\033[31m')
YELLOW=$(printf '\033[33m'); BOLD=$(printf '\033[1m')
DIM=$(printf '\033[2m'); RESET=$(printf '\033[0m')

step()  { printf "%s==>%s %s\n" "$GREEN" "$RESET" "$1"; }
warn()  { printf "%s!%s %s\n" "$YELLOW" "$RESET" "$1" >&2; }
note()  { printf "%s   %s%s\n" "$DIM" "$1" "$RESET"; }

confirm() {
  # Read confirmation from /dev/tty so it works under `curl … | bash`
  printf "%s%s%s [y/N] " "$BOLD" "$1" "$RESET"
  local answer
  if [[ -t 0 ]]; then
    read -r answer
  else
    read -r answer </dev/tty
  fi
  [[ "$answer" =~ ^[Yy]([Ee][Ss])?$ ]]
}

[[ "$(uname)" == "Darwin" ]] || { warn "Billable is macOS only — nothing to uninstall."; exit 0; }

# ---------- Quit running Billable ----------
if pgrep -x "Billable" >/dev/null 2>&1; then
  step "Quitting running Billable…"
  osascript -e 'quit app "Billable"' >/dev/null 2>&1 || true
  for _ in 1 2 3 4 5; do
    pgrep -x "Billable" >/dev/null 2>&1 || break
    sleep 1
  done
fi

# ---------- Locate Billable.app instances ----------
LSREG="/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister"
BUNDLES=()

if [[ -x "$LSREG" ]]; then
  while IFS= read -r line; do
    BUNDLES+=("$line")
  done < <("$LSREG" -dump 2>/dev/null \
    | grep -E "^[[:space:]]*path:[[:space:]]+.*Billable\.app$" \
    | sed -E 's/^[[:space:]]*path:[[:space:]]+//' \
    | sort -u)
fi

# Fallback to common spots if Launch Services lookup found nothing
if [[ ${#BUNDLES[@]} -eq 0 ]]; then
  for candidate in \
    "/Applications/Billable.app" \
    "$HOME/Applications/Billable.app" \
    "$HOME/Desktop/Billable.app" \
    "$HOME/Desktop/Projects/Billable.app"; do
    [[ -d "$candidate" ]] && BUNDLES+=("$candidate")
  done
fi

if [[ ${#BUNDLES[@]} -eq 0 ]]; then
  warn "No Billable.app found on this Mac."
else
  printf "\nFound %d Billable install(s):\n" "${#BUNDLES[@]}"
  for b in "${BUNDLES[@]}"; do
    note "$b"
  done
  printf "\n"
  if confirm "Remove these app bundles?"; then
    for b in "${BUNDLES[@]}"; do
      step "Removing $b…"
      rm -rf "$b" || warn "Couldn't remove $b (permission denied?)"
    done
  else
    note "Skipped app removal."
  fi
fi

# ---------- Offer to remove user data ----------
DATA_DIR="$HOME/Library/Application Support/billable"
if [[ -d "$DATA_DIR" ]]; then
  printf "\n"
  warn "Your local data lives at:"
  note "$DATA_DIR"
  warn "This contains all your clients, projects, time entries, invoices, and expenses for every profile."
  if confirm "Permanently delete this data? (cannot be undone)"; then
    step "Removing $DATA_DIR…"
    rm -rf "$DATA_DIR"
    note "Data deleted."
  else
    note "Data preserved — you can reinstall and it'll be there waiting."
  fi
fi

printf "\n%sUninstall complete.%s\n\n" "$GREEN$BOLD" "$RESET"
