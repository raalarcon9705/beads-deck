#!/usr/bin/env bash
# Beads Deck installer — builds from source and installs the binary.
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/raalarcon9705/beads-deck/main/install.sh | bash
# or, from a checkout:
#   ./install.sh
set -euo pipefail

REPO="raalarcon9705/beads-deck"
BIN="beads-deck"
PREFIX="${PREFIX:-$HOME/.local/bin}"

info() { printf '\033[0;36m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[0;33mwarning:\033[0m %s\n' "$1"; }
die()  { printf '\033[0;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || die "Rust/cargo is required. Install it from https://rustup.rs"
command -v bd >/dev/null 2>&1 || warn "'bd' (beads CLI) was not found on PATH. Beads Deck needs it at runtime: https://github.com/steveyegge/beads"

# Use the current checkout if we're inside it; otherwise clone.
if [ -f "Cargo.toml" ] && grep -q 'name = "beads-deck"' Cargo.toml 2>/dev/null; then
  SRC="$(pwd)"
  CLEANUP=""
else
  command -v git >/dev/null 2>&1 || die "git is required to fetch the source."
  TMP="$(mktemp -d)"
  CLEANUP="$TMP"
  info "Cloning $REPO …"
  git clone --depth 1 "https://github.com/$REPO" "$TMP/beads-deck" >/dev/null 2>&1
  SRC="$TMP/beads-deck"
fi

info "Building release (this may take a few minutes the first time) …"
( cd "$SRC" && cargo build --release )

mkdir -p "$PREFIX"
install -m 0755 "$SRC/target/release/$BIN" "$PREFIX/$BIN"
info "Installed $BIN → $PREFIX/$BIN"

[ -n "$CLEANUP" ] && rm -rf "$CLEANUP"

case ":$PATH:" in
  *":$PREFIX:"*) ;;
  *) warn "$PREFIX is not on your PATH. Add it, e.g.:  echo 'export PATH=\"$PREFIX:\$PATH\"' >> ~/.zshrc" ;;
esac

info "Done. Run '$BIN' to launch, or '$BIN /path/to/project' to open a workspace."
