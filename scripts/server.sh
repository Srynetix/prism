#!/usr/bin/env bash
# Builds prism-server, Prism without its window (docs/guide.md › Without a window), for this
# system: the phone's page first (web/dist, carried in the binary), then the binary.
#
#   scripts/server.sh [--universal] [--out file]
#
# - --universal (macOS): Apple silicon and Intel in one binary, as the .dmg.
# - Out: src-tauri/target/release/prism-server (--universal:
#   src-tauri/target/universal-apple-darwin/release/prism-server); --out copies it there too.
set -euo pipefail

cd "$(dirname "$0")/.."

universal=false
out=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --universal) universal=true ;;
    --out) out=${2:-}; shift ;;
    -h | --help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

step() { printf '\n\033[1m▸ %s\033[0m\n' "$*"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

for tool in pnpm cargo; do
  command -v "$tool" >/dev/null || fail "$tool not found"
done
[[ -d web/node_modules ]] || fail "no web/node_modules: just install"
if $universal; then
  [[ "$(uname -s)" == Darwin ]] || fail "--universal: macOS only"
  command -v lipo >/dev/null || fail "lipo not found"
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)
step "prism-server $version ($(git rev-parse --short HEAD))$($universal && echo ', universal')"

step "The phone's page (web/dist)"
pnpm --dir web build

step "The binary"
if $universal; then
  bins=()
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed | grep -qx "$t" || { step "Rust target $t"; rustup target add "$t"; }
    cargo build --release -p prism-server --manifest-path src-tauri/Cargo.toml --target "$t"
    bins+=("src-tauri/target/$t/release/prism-server")
  done
  bin=src-tauri/target/universal-apple-darwin/release/prism-server
  mkdir -p "$(dirname "$bin")"
  lipo -create -output "$bin" "${bins[@]}"
  lipo -archs "$bin" | grep -q "x86_64 arm64\|arm64 x86_64" || fail "the binary is not universal"
else
  cargo build --release -p prism-server --manifest-path src-tauri/Cargo.toml
  bin=src-tauri/target/release/prism-server
fi
[[ -x "$bin" ]] || fail "no $bin after the build"
if [[ -n "$out" ]]; then
  mkdir -p "$(dirname "$out")"
  cp "$bin" "$out"
  bin=$out
fi

step "Done"
echo "$bin ($(du -h "$bin" | cut -f1))"
echo "install it: install $bin ~/.local/bin/ (docs/guide.md › Without a window)"
