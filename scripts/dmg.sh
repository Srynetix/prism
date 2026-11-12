#!/usr/bin/env bash
# Builds Prism for distribution on macOS: a .dmg to install it on another Mac.
#
#   scripts/dmg.sh [--native] [--skip-checks] [--allow-dirty]
#
# - Universal by default (Apple silicon and Intel); --native builds for this Mac only, faster.
# - Checks first (frontend types, Rust tests); --skip-checks to go straight to the build.
# - Refuses uncommitted changes, so that the app says which commit it is; --allow-dirty to
#   build anyway (the app's commit then ends with `+`).
#
# Out: dist/Prism_<version>_<commit>_<arch>.dmg and its .sha256, say
# Prism_0.1.0_2cac323_universal.dmg (`-dirty`: uncommitted changes). The .dmg,
# its volume named the same way, holds Prism.app and a link to /Applications: drag one onto the other.
#
# Signing: ad hoc by default (no Apple account). On another Mac, macOS then refuses the first
# launch; System Settings › Privacy & Security › Open Anyway lets it through, once. With a
# Developer ID, the app is signed with it and notarized, and opens like any other:
#   PRISM_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"   the certificate, in the keychain
#   PRISM_NOTARY_PROFILE="prism"   a profile saved by `xcrun notarytool store-credentials`
set -euo pipefail

cd "$(dirname "$0")/.."

native=false
checks=true
dirty=false
for arg in "$@"; do
  case "$arg" in
    --native) native=true ;;
    --skip-checks) checks=false ;;
    --allow-dirty) dirty=true ;;
    -h | --help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done

step() { printf '\n\033[1m▸ %s\033[0m\n' "$*"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

[[ "$(uname -s)" == Darwin ]] || fail "a .dmg is built on macOS"
for tool in pnpm cargo rustup hdiutil codesign; do
  command -v "$tool" >/dev/null || fail "$tool not found"
done

version=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)
[[ -n "$version" ]] || fail "no version in src-tauri/Cargo.toml"
commit=$(git rev-parse --short HEAD)

# Tracked files only: untracked ones (local notes, tools' folders) are not part of the build.
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  $dirty || fail "uncommitted changes: commit them, or --allow-dirty"
  echo "uncommitted changes: building anyway (--allow-dirty)"
  commit+="-dirty"
fi

tag="$commit"

if $native; then
  target=""
  arch=$(uname -m)
  bundle=src-tauri/target/release/bundle/macos
else
  target=universal-apple-darwin
  arch=universal
  bundle=src-tauri/target/$target/release/bundle/macos
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed | grep -qx "$t" || { step "Rust target $t"; rustup target add "$t"; }
  done
fi

step "Prism $version ($commit), $arch"

if $checks; then
  step "Checks: frontend types, Rust tests"
  pnpm --dir web check
  cargo test --quiet --workspace --manifest-path src-tauri/Cargo.toml
fi

step "Release build"
args=(build --bundles app)
[[ -n "$target" ]] && args+=(--target "$target")
web/node_modules/.bin/tauri "${args[@]}"
app="$bundle/Prism.app"
[[ -d "$app" ]] || fail "no $app after the build"

if [[ -n "${PRISM_SIGN_IDENTITY:-}" ]]; then
  step "Signing with $PRISM_SIGN_IDENTITY"
  # The hardened runtime and the app's entitlements (Calendar access), as notarization requires.
  codesign --force --deep --timestamp --options runtime --entitlements src-tauri/Entitlements.plist --sign "$PRISM_SIGN_IDENTITY" "$app"
fi
codesign --verify --deep --strict "$app" || fail "the app's signature does not verify"
[[ "$arch" != universal ]] || lipo -archs "$app/Contents/MacOS/prism" | grep -q "x86_64 arm64\|arm64 x86_64" || fail "the binary is not universal"

step "Disk image"
mkdir -p dist
dmg="dist/Prism_${version}_${tag}_${arch}.dmg"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
rm -f "$dmg"
hdiutil create -volname "Prism $version ($commit)" -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$dmg" >/dev/null
if [[ -n "${PRISM_SIGN_IDENTITY:-}" ]]; then
  codesign --force --timestamp --sign "$PRISM_SIGN_IDENTITY" "$dmg"
fi
hdiutil verify "$dmg" >/dev/null || fail "the disk image does not verify"

if [[ -n "${PRISM_SIGN_IDENTITY:-}" && -n "${PRISM_NOTARY_PROFILE:-}" ]]; then
  step "Notarization (a few minutes)"
  xcrun notarytool submit "$dmg" --keychain-profile "$PRISM_NOTARY_PROFILE" --wait
  xcrun stapler staple "$dmg"
fi

(cd dist && shasum -a 256 "$(basename "$dmg")" > "$(basename "$dmg").sha256")

step "Done"
echo "$dmg ($(du -h "$dmg" | cut -f1)), sha256 in $dmg.sha256"
if [[ -z "${PRISM_SIGN_IDENTITY:-}" ]]; then
  cat <<'EOF'

Signed ad hoc: on the other Mac, open the .dmg, drag Prism onto Applications, open it once;
macOS refuses it. Then System Settings › Privacy & Security › "Prism was blocked" › Open Anyway.
EOF
elif [[ -z "${PRISM_NOTARY_PROFILE:-}" ]]; then
  echo "Signed but not notarized (no PRISM_NOTARY_PROFILE): Gatekeeper still asks on the other Mac."
fi
