#!/usr/bin/env bash
# Builds Prism for Linux, from any machine with Docker (a Mac with OrbStack or Docker Desktop):
# a .deb (Debian, Ubuntu and their derivatives: installs tmux with it) and an AppImage (any other
# distribution: a single executable file).
#
#   scripts/linux.sh [--arch amd64|arm64] [--bundles deb,appimage,rpm,pacman] [--server] [--skip-checks] [--allow-dirty]
#
# - amd64 (x86_64 PCs) by default; on an Apple silicon Mac it runs emulated, slower: arm64 builds
#   natively for ARM machines.
# - --bundles: deb,appimage by default; rpm adds a package for Fedora and openSUSE, pacman one for
#   Arch Linux and its derivatives (amd64: the .deb repackaged in an archlinux container).
# - --server: prism-server too (Prism without a window, the phone's page in it), built after the
#   bundles in the same container: dist/prism-server_<version>_<arch>.
# - Checks first on this machine (frontend types, Rust tests); --skip-checks to go straight on.
# - Refuses uncommitted changes, so that the app says which commit it is; --allow-dirty to
#   build anyway (the app's commit then ends with `+`).
#
# Out: dist/Prism_<version>_<arch>.deb, .AppImage (.rpm, .pkg.tar.zst), each with its .sha256,
# (prism-server_<version>_<arch>) and Prism_<version>_<arch>.build: the commit they are of (release.sh reuses them).
# The first build takes a while (the image, then every crate); the next ones reuse a Docker
# volume, prism-linux-<arch>. `docker volume rm prism-linux-<arch>` starts over.
set -euo pipefail

cd "$(dirname "$0")/.."

arch=amd64
bundles=deb,appimage
server=false
checks=true
dirty=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) arch=${2:-}; shift ;;
    --bundles) bundles=${2:-}; shift ;;
    --server) server=true ;;
    --skip-checks) checks=false ;;
    --allow-dirty) dirty=true ;;
    -h | --help) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

step() { printf '\n\033[1m▸ %s\033[0m\n' "$*"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

[[ "$arch" == amd64 || "$arch" == arm64 ]] || fail "--arch: amd64 or arm64"
[[ "$bundles" =~ ^(deb|appimage|rpm|pacman)(,(deb|appimage|rpm|pacman))*$ ]] || fail "--bundles: deb, appimage, rpm and/or pacman, comma-separated"
# The Arch package is the .deb repackaged: Tauri builds the others, the .deb with them.
pacman=false
tauri_bundles=$(tr , '\n' <<<"$bundles" | { grep -vx pacman || true; } | paste -sd, -)
if [[ ",$bundles," == *,pacman,* ]]; then
  pacman=true
  # Arch Linux is x86_64 only (Arch Linux ARM is another distribution, its image another).
  [[ "$arch" == amd64 ]] || fail "--bundles pacman: amd64 only"
  [[ ",$tauri_bundles," == *,deb,* ]] || tauri_bundles=${tauri_bundles:+$tauri_bundles,}deb
fi
command -v docker >/dev/null || fail "docker not found (OrbStack, Docker Desktop…)"
docker info >/dev/null 2>&1 || fail "Docker is not running"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)
[[ -n "$version" ]] || fail "no version in src-tauri/Cargo.toml"
commit=$(git rev-parse --short HEAD)

# Tracked files only: untracked ones (local notes, tools' folders) are not part of the build.
if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
  $dirty || fail "uncommitted changes: commit them, or --allow-dirty"
  echo "uncommitted changes: building anyway (--allow-dirty)"
fi

step "Prism $version ($commit), linux $arch, $bundles$($server && echo ', prism-server')"

if $checks; then
  step "Checks: frontend types, Rust tests"
  pnpm --dir web check
  cargo test --quiet --workspace --manifest-path src-tauri/Cargo.toml
fi

step "Build image (cached after the first time)"
image=prism-linux-builder:$arch
node=$(node --version 2>/dev/null | tr -d v || true)
docker build --platform "linux/$arch" ${node:+--build-arg NODE_VERSION=$node} -t "$image" scripts/linux

step "Release build in the container"
mkdir -p dist
out=$(mktemp -d "${TMPDIR:-/tmp}/prism-linux.XXXXXX")
trap 'rm -rf "$out"' EXIT
docker run --rm --platform "linux/$arch" \
  -v "$PWD:/src:ro" -v "prism-linux-$arch:/cache" -v "$out:/out" \
  -e BUNDLES="$tauri_bundles" -e SERVER="$($server && echo 1 || true)" \
  "$image" /src/scripts/linux/build.sh

if $pacman; then
  step "Arch package, from the .deb"
  in=$(mktemp -d "${TMPDIR:-/tmp}/prism-arch.XXXXXX")
  trap 'rm -rf "$out" "$in"' EXIT
  debs=("$out"/*.deb)
  cp "${debs[0]}" "$in/prism.deb"
  cp LICENSE "$in/"
  # pacman's versions have no `-` (it separates the release): 0.1.0-a1 is 0.1.0_a1, older than 0.1.0 all the same.
  docker run --rm --platform linux/amd64 \
    -v "$PWD/scripts/linux/arch:/pkg:ro" -v "$in:/in:ro" -v "$out:/out" \
    -e PRISM_PKGVER="${version//-/_}" \
    archlinux:base-devel /pkg/build.sh
  # Asked for the Arch package alone, the .deb it came from is not one of the outputs.
  [[ ",$bundles," == *,deb,* ]] || rm -f "$out"/*.deb
fi

step "Done"
shopt -s nullglob
for f in "$out"/*; do
  case "$f" in
    *.sha256) continue ;;
    *.deb) name="Prism_${version}_${arch}.deb" ;;
    *.AppImage) name="Prism_${version}_${arch}.AppImage" ;;
    *.rpm) name="Prism_${version}_${arch}.rpm" ;;
    *.pkg.tar.zst) name="Prism_${version}_${arch}.pkg.tar.zst" ;;
    */prism-server) name="prism-server_${version}_${arch}" ;;
    *) continue ;;
  esac
  cp "$f" "dist/$name"
  (cd dist && shasum -a 256 "$name" > "$name.sha256")
  echo "dist/$name ($(du -h "dist/$name" | cut -f1))"
done
# Which commit they are of, for release.sh to reuse them (`-dirty`: uncommitted changes).
echo "$commit$([[ -n "$(git status --porcelain --untracked-files=no)" ]] && echo -dirty)" > "dist/Prism_${version}_${arch}.build"
cat <<'TXT'

On the Linux machine:
  .deb       sudo apt install ./Prism_<version>_<arch>.deb
  AppImage   chmod +x Prism_<version>_<arch>.AppImage && ./Prism_<version>_<arch>.AppImage
             (needs libfuse2: sudo apt install libfuse2, or libfuse2t64 on Ubuntu 24.04)
  Arch       sudo pacman -U Prism_<version>_amd64.pkg.tar.zst
TXT
