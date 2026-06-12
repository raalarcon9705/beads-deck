#!/usr/bin/env bash
# Beads Deck installer.
# Downloads the prebuilt binary for your platform from the latest GitHub
# release; falls back to building from source (requires Rust) if none matches.
#
#   curl -fsSL https://raw.githubusercontent.com/raalarcon9705/beads-deck/main/install.sh | bash
#   PREFIX=/usr/local/bin ./install.sh        # choose install dir
#   BEADS_DECK_FROM_SOURCE=1 ./install.sh      # force building from source
set -euo pipefail

REPO="raalarcon9705/beads-deck"
BIN="beads-deck"
PREFIX="${PREFIX:-$HOME/.local/bin}"
APP_NAME="Beads Deck"
BUNDLE_ID="com.raalarcon9705.beads-deck"

info() { printf '\033[0;36m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[0;33mwarning:\033[0m %s\n' "$1"; }
die()  { printf '\033[0;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

command -v bd >/dev/null 2>&1 || warn "'bd' (beads CLI) not found on PATH. Beads Deck needs it at runtime: https://github.com/steveyegge/beads"

# Map this host to a release target triple.
detect_target() {
  local os arch
  os="$(uname -s)"; arch="$(uname -m)"
  case "$os" in
    Darwin) case "$arch" in
        arm64|aarch64) echo "aarch64-apple-darwin" ;;
        x86_64)        echo "x86_64-apple-darwin" ;;
        *) echo "" ;; esac ;;
    Linux)  case "$arch" in
        x86_64) echo "x86_64-unknown-linux-gnu" ;;
        *) echo "" ;; esac ;;
    *) echo "" ;;
  esac
}

build_from_source() {
  command -v cargo >/dev/null 2>&1 || die "Rust/cargo is required to build from source. Install it from https://rustup.rs"
  local SRC CLEANUP=""
  if [ -f "Cargo.toml" ] && grep -q 'name = "beads-deck"' Cargo.toml 2>/dev/null; then
    SRC="$(pwd)"
  else
    command -v git >/dev/null 2>&1 || die "git is required to fetch the source."
    local TMP; TMP="$(mktemp -d)"; CLEANUP="$TMP"
    info "Cloning $REPO …"
    git clone --depth 1 "https://github.com/$REPO" "$TMP/beads-deck" >/dev/null 2>&1
    SRC="$TMP/beads-deck"
  fi
  info "Building release (this may take a few minutes) …"
  ( cd "$SRC" && cargo build --release )
  mkdir -p "$PREFIX"
  install -m 0755 "$SRC/target/release/$BIN" "$PREFIX/$BIN"
  [ -n "$CLEANUP" ] && rm -rf "$CLEANUP"
}

install_prebuilt() {
  local target="$1"
  local url="https://github.com/$REPO/releases/latest/download/${BIN}-${target}.tar.gz"
  local TMP; TMP="$(mktemp -d)"
  info "Downloading prebuilt binary ($target) …"
  if ! curl -fsSL "$url" -o "$TMP/b.tar.gz"; then
    rm -rf "$TMP"; return 1
  fi
  tar -xzf "$TMP/b.tar.gz" -C "$TMP"
  mkdir -p "$PREFIX"
  install -m 0755 "$TMP/$BIN" "$PREFIX/$BIN"
  # Clear the quarantine flag so macOS lets the downloaded binary run.
  [ "$(uname -s)" = "Darwin" ] && xattr -d com.apple.quarantine "$PREFIX/$BIN" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}

# Wrap the just-installed binary in a macOS .app bundle so Beads Deck shows up
# in Launchpad / Applications and Spotlight. Best-effort: skips gracefully if
# build tools or the icon are unavailable. Disable with BEADS_DECK_NO_APP=1.
make_macos_app() {
  [ "$(uname -s)" = "Darwin" ] || return 0
  [ -n "${BEADS_DECK_NO_APP:-}" ] && return 0
  command -v iconutil >/dev/null 2>&1 || { warn "iconutil not found; skipping .app bundle"; return 0; }

  local version svg work iconset master app dest icon_key="" s
  if [ -f Cargo.toml ] && grep -q 'name = "beads-deck"' Cargo.toml 2>/dev/null; then
    version="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
  else
    version="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
      | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -1)"
  fi
  [ -n "$version" ] || version="0.0.0"

  work="$(mktemp -d)"
  if [ -f assets/logo.svg ]; then
    svg="assets/logo.svg"
  else
    svg="$work/logo.svg"
    curl -fsSL "https://raw.githubusercontent.com/$REPO/main/assets/logo.svg" -o "$svg" 2>/dev/null || svg=""
  fi

  app="$work/$APP_NAME.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  install -m 0755 "$PREFIX/$BIN" "$app/Contents/MacOS/$BIN"

  # Render an .icns from the SVG when the icon tools are present.
  if [ -n "$svg" ] && command -v sips >/dev/null 2>&1 && command -v qlmanage >/dev/null 2>&1; then
    iconset="$work/icon.iconset"; mkdir -p "$iconset"
    qlmanage -t -s 1024 "$svg" -o "$work" >/dev/null 2>&1 || true
    master="$work/$(basename "$svg").png"
    if [ -f "$master" ]; then
      for s in 16 32 128 256 512; do
        sips -z "$s" "$s" "$master" --out "$iconset/icon_${s}x${s}.png" >/dev/null 2>&1
        sips -z "$((s * 2))" "$((s * 2))" "$master" --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null 2>&1
      done
      iconutil -c icns "$iconset" -o "$app/Contents/Resources/icon.icns" >/dev/null 2>&1 \
        && icon_key='  <key>CFBundleIconFile</key><string>icon</string>'
    fi
  fi

  cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$BIN</string>
$icon_key
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleSignature</key><string>????</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

  dest="${DEST:-/Applications}"
  [ -w "$dest" ] || dest="$HOME/Applications"
  mkdir -p "$dest"
  rm -rf "$dest/$APP_NAME.app"
  cp -R "$app" "$dest/"
  # Ad-hoc sign + refresh Launch Services so Gatekeeper allows it and it shows up.
  codesign --force --deep --sign - "$dest/$APP_NAME.app" >/dev/null 2>&1 || true
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
    -f "$dest/$APP_NAME.app" >/dev/null 2>&1 || true
  # Feed Spotlight's metadata index so it's searchable via Cmd+Space immediately.
  mdimport "$dest/$APP_NAME.app" >/dev/null 2>&1 || true
  rm -rf "$work"
  info "Installed app bundle → $dest/$APP_NAME.app (Spotlight: 'Beads Deck')"
}

TARGET="$(detect_target)"
if [ -n "${BEADS_DECK_FROM_SOURCE:-}" ] || [ -z "$TARGET" ]; then
  [ -z "$TARGET" ] && warn "No prebuilt binary for this platform; building from source."
  build_from_source
elif ! install_prebuilt "$TARGET"; then
  warn "Prebuilt download failed; building from source."
  build_from_source
fi

info "Installed $BIN → $PREFIX/$BIN"
make_macos_app
case ":$PATH:" in
  *":$PREFIX:"*) ;;
  *) warn "$PREFIX is not on your PATH. Add it, e.g.:  echo 'export PATH=\"$PREFIX:\$PATH\"' >> ~/.zshrc" ;;
esac
info "Done. Run '$BIN' to launch, or '$BIN /path/to/project' to open a workspace."
