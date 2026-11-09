#!/bin/bash
# Wraps the dev build in an app bundle (so macOS treats it like an app) and
# opens it against the dev data copy, never the real data.
# Usage: scripts/dev-bundle.sh [release] [--no-open]
set -euo pipefail
cd "$(dirname "$0")/.."
PROFILE=debug
[[ "${1:-}" == "release" ]] && PROFILE=release
APP="target/Billable Dev.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$PROFILE/billable" "$APP/Contents/MacOS/billable"
cp ../resources/icon.icns "$APP/Contents/Resources/icon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.billable.native.dev</string>
  <key>CFBundleName</key><string>Billable Dev</string>
  <key>CFBundleExecutable</key><string>billable</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>2.0.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict></plist>
PLIST
codesign --force --sign - "$APP" >/dev/null 2>&1 || true
if [[ " ${*:-} " != *" --no-open "* ]]; then
  pkill -f "Billable Dev.app/Contents/MacOS/billable" 2>/dev/null || true
  open -g -n --env BILLABLE_USER_DATA="$HOME/Library/Application Support/Billable Dev" "$APP"
fi
echo "$APP"
