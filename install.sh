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

TARGET="$(detect_target)"
if [ -n "${BEADS_DECK_FROM_SOURCE:-}" ] || [ -z "$TARGET" ]; then
  [ -z "$TARGET" ] && warn "No prebuilt binary for this platform; building from source."
  build_from_source
elif ! install_prebuilt "$TARGET"; then
  warn "Prebuilt download failed; building from source."
  build_from_source
fi

info "Installed $BIN → $PREFIX/$BIN"
case ":$PATH:" in
  *":$PREFIX:"*) ;;
  *) warn "$PREFIX is not on your PATH. Add it, e.g.:  echo 'export PATH=\"$PREFIX:\$PATH\"' >> ~/.zshrc" ;;
esac
info "Done. Run '$BIN' to launch, or '$BIN /path/to/project' to open a workspace."
