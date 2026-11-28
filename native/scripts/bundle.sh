#!/bin/bash
# Builds the release app and its disk image: release/Billable.app and
# release/Billable-<version>-universal.dmg. The app is universal, with one
# binary that runs natively on both Apple Silicon (arm64) and Intel (x86_64).
# Usage: scripts/bundle.sh [--no-dmg]
# BILLABLE_SLICES=<dir> uses slices already built at <dir>/<target>/billable.
# It never opens the app (the release build uses the real data folder).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
SLICES=()
for T in "${TARGETS[@]}"; do
  if [[ -n "${BILLABLE_SLICES:-}" ]]; then
    # CI builds each slice on its own Mac at the same time, then hands them here.
    SLICES+=("$BILLABLE_SLICES/$T/billable")
  else
    SLICES+=("$(scripts/build-slice.sh "$T" "${BILLABLE_PROFILE:-release}")")
  fi
done

OUT=release
APP="$OUT/Billable.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${SLICES[@]}" -output "$APP/Contents/MacOS/billable"
chmod +x "$APP/Contents/MacOS/billable"
lipo "$APP/Contents/MacOS/billable" -verify_arch arm64 x86_64
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
echo "$APP ($(du -sh "$APP" | cut -f1), $(lipo -archs "$APP/Contents/MacOS/billable"))"

if [[ " ${*:-} " != *" --no-dmg "* ]]; then
  DMG="$OUT/Billable-$VERSION-universal.dmg"
  STAGE=$(mktemp -d)
  cp -R "$APP" "$STAGE/"
  ln -s /Applications "$STAGE/Applications"
  rm -f "$DMG"
  hdiutil create -volname Billable -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
  rm -rf "$STAGE"
  echo "$DMG ($(du -sh "$DMG" | cut -f1))"
fi
