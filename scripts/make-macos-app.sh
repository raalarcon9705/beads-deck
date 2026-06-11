#!/usr/bin/env bash
# Build a macOS .app bundle for Beads Deck so it appears in Launchpad /
# Applications and is searchable via Spotlight (Cmd+Space).
#
#   ./scripts/make-macos-app.sh            # build + install to /Applications (or ~/Applications)
#   DEST=~/Applications ./scripts/make-macos-app.sh
set -euo pipefail

cd "$(dirname "$0")/.."
APP_NAME="Beads Deck"
BIN="beads-deck"
BUNDLE_ID="com.raalarcon9705.beads-deck"
VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"

command -v cargo >/dev/null || { echo "cargo required"; exit 1; }
command -v iconutil >/dev/null || { echo "iconutil (macOS) required"; exit 1; }

echo "==> Building release binary"
cargo build --release

# --- Icon: SVG -> iconset -> .icns ---
echo "==> Rendering icon"
WORK="$(mktemp -d)"
ICONSET="$WORK/icon.iconset"
mkdir -p "$ICONSET"
# Master 1024 PNG from the SVG via Quick Look.
qlmanage -t -s 1024 assets/logo.svg -o "$WORK" >/dev/null 2>&1
MASTER="$WORK/logo.svg.png"
for s in 16 32 64 128 256 512 1024; do
  sips -z "$s" "$s" "$MASTER" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
done
# @2x variants
cp "$ICONSET/icon_32x32.png"   "$ICONSET/icon_16x16@2x.png"
cp "$ICONSET/icon_64x64.png"   "$ICONSET/icon_32x32@2x.png"
cp "$ICONSET/icon_256x256.png" "$ICONSET/icon_128x128@2x.png"
cp "$ICONSET/icon_512x512.png" "$ICONSET/icon_256x256@2x.png"
cp "$ICONSET/icon_1024x1024.png" "$ICONSET/icon_512x512@2x.png"
rm -f "$ICONSET/icon_64x64.png" "$ICONSET/icon_1024x1024.png"
iconutil -c icns "$ICONSET" -o "$WORK/icon.icns"

# --- Assemble the .app ---
APP="$WORK/$APP_NAME.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/release/$BIN" "$APP/Contents/MacOS/$BIN"
cp "$WORK/icon.icns" "$APP/Contents/Resources/icon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

# --- Install ---
DEST="${DEST:-/Applications}"
if [ ! -w "$DEST" ]; then DEST="$HOME/Applications"; fi
mkdir -p "$DEST"
rm -rf "$DEST/$APP_NAME.app"
cp -R "$APP" "$DEST/"
# Ad-hoc sign so Gatekeeper lets it run locally.
codesign --force --deep --sign - "$DEST/$APP_NAME.app" >/dev/null 2>&1 || true
# Refresh Launch Services / Spotlight so it shows up immediately.
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
  -f "$DEST/$APP_NAME.app" >/dev/null 2>&1 || true
mdimport "$DEST/$APP_NAME.app" >/dev/null 2>&1 || true
rm -rf "$WORK"

echo "==> Installed: $DEST/$APP_NAME.app"
echo "    Search it with Cmd+Space → 'Beads Deck'."
