#!/bin/bash
# Launches a built Billable.app against a fresh, throwaway data folder and
# checks that it starts, opens its database, and keeps running.
# Usage: scripts/smoke-test.sh <path/to/Billable.app> [arm64|x86_64]
# With an architecture, that slice of the universal binary is run
# (x86_64 on an Apple Silicon Mac runs under Rosetta).
set -euo pipefail
APP="$1"
ARCH="${2:-}"
BIN="$APP/Contents/MacOS/billable"
SECONDS_UP="${SMOKE_SECONDS:-20}"

[[ -x "$BIN" ]] || { echo "No executable at $BIN"; exit 1; }
echo "Binary slices: $(lipo -archs "$BIN")"

WORK="$(mktemp -d)"
export BILLABLE_USER_DATA="$WORK/data"
LOG="$WORK/billable.log"

if [[ -n "$ARCH" ]]; then
  arch "-$ARCH" "$BIN" >"$LOG" 2>&1 &
else
  "$BIN" >"$LOG" 2>&1 &
fi
PID=$!
cleanup() { kill "$PID" 2>/dev/null || true; }
trap cleanup EXIT

# The running process's architecture, as the kernel sees it.
sleep 3
if kill -0 "$PID" 2>/dev/null; then
  KIND="$(ps -o pid=,comm= -p "$PID" || true)"
  echo "Running: $KIND"
  TRANSLATED="$(sysctl -in sysctl.proc_translated 2>/dev/null || echo 0)"
  echo "This shell under Rosetta: $TRANSLATED, machine: $(uname -m)"
fi

sleep "$SECONDS_UP"
if ! kill -0 "$PID" 2>/dev/null; then
  wait "$PID" && STATUS=0 || STATUS=$?
  echo "FAIL: Billable exited after launch (status $STATUS). Its output:"
  cat "$LOG"
  exit 1
fi

DB="$(find "$BILLABLE_USER_DATA" -name 'billable.db' -print -quit 2>/dev/null || true)"
echo "Data folder after ${SECONDS_UP}s:"
find "$BILLABLE_USER_DATA" -maxdepth 3 -print 2>/dev/null | sed "s|$WORK/||"
kill "$PID"
wait "$PID" 2>/dev/null || true
trap - EXIT

echo "--- Billable's output ---"
cat "$LOG"
echo "-------------------------"
if [[ -z "$DB" ]]; then
  echo "FAIL: Billable kept running but never created its database."
  exit 1
fi
echo "PASS: Billable ${ARCH:-(native)} started, created $DB, and kept running for ${SECONDS_UP}s."
