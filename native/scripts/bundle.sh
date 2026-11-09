#!/bin/bash
# Builds the release app and its disk image, matching what electron-builder
# made: release/Billable.app and release/Billable-<version>-arm64.dmg.
# Usage: scripts/bundle.sh [--no-dmg]
# It never opens the app (the release build uses the real data folder).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release -p billable

OUT=release
APP="$OUT/Billable.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/billable "$APP/Contents/MacOS/billable"
cp ../resources/icon.icns "$APP/Contents/Resources/icon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.billable.app</string>
  <key>CFBundleName</key><string>Billable</string>
  <key>CFBundleDisplayName</key><string>Billable</string>
  <key>CFBundleExecutable</key><string>billable</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>CFBundleURLTypes</key>
  <array><dict>
    <key>CFBundleURLName</key><string>Billable</string>
    <key>CFBundleURLSchemes</key><array><string>billable</string></array>
  </dict></array>
</dict></plist>
PLIST
codesign --force --deep --sign - "$APP"
echo "$APP ($(du -sh "$APP" | cut -f1))"

if [[ " ${*:-} " != *" --no-dmg "* ]]; then
  DMG="$OUT/Billable-$VERSION-arm64.dmg"
  STAGE=$(mktemp -d)
  cp -R "$APP" "$STAGE/"
  ln -s /Applications "$STAGE/Applications"
  rm -f "$DMG"
  hdiutil create -volname Billable -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
  rm -rf "$STAGE"
  echo "$DMG ($(du -sh "$DMG" | cut -f1))"
fi
