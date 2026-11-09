#!/bin/bash
# Wraps a built binary in a minimal .app so it can be opened and screenshotted
# like any Mac app. Usage: ./bundle.sh <binary-name> [release|debug]
set -euo pipefail
cd "$(dirname "$0")"
BIN="${1:-billable-gpui}"
PROFILE="${2:-debug}"
APP="target/${BIN}.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "target/${PROFILE}/${BIN}" "$APP/Contents/MacOS/${BIN}"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.billable.gpui-test.${BIN}</string>
  <key>CFBundleName</key><string>${BIN}</string>
  <key>CFBundleExecutable</key><string>${BIN}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$APP" >/dev/null 2>&1 || true
echo "$APP"
