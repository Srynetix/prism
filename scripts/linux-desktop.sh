#!/usr/bin/env bash
# Runs the Linux build of Prism on a desktop in a container, to see and use it from this machine:
# an Xfce desktop, in the browser (noVNC) or a VNC client, Prism started in it on the demo profile.
#
#   scripts/linux-desktop.sh [--distro ubuntu|arch] [--arch arm64|amd64] [--package <file>] [--geometry 1600x1000]
#
# - ubuntu (22.04, the bundles' baseline) by default; arch: Arch Linux, the Arch package.
# - arm64 by default (native on an Apple silicon Mac); amd64 runs emulated. Arch Linux is amd64
#   only: emulated there, slow.
# - Takes dist/Prism_<version>_<arch>.deb, built first by `just linux --arch <arch> --bundles deb`
#   (Arch: dist/Prism_<version>_amd64.pkg.tar.zst, `just linux --bundles pacman`).
# - Demo profile (src-tauri/crates/core/demo/config.toml): fake data, nothing read from anywhere.
# - Browser: http://localhost:6080/vnc.html?autoconnect=1&resize=remote; VNC: vnc://localhost:5901
#   (both on 127.0.0.1 only, a password drawn each run, printed with the address). Ctrl-C stops
#   and removes the container.
set -euo pipefail

cd "$(dirname "$0")/.."

distro=ubuntu
arch=""
package=""
geometry=1600x1000
while [[ $# -gt 0 ]]; do
  case "$1" in
    --distro) distro=${2:-}; shift ;;
    --arch) arch=${2:-}; shift ;;
    --package | --deb) package=${2:-}; shift ;;
    --geometry) geometry=${2:-}; shift ;;
    -h | --help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

fail() { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

version=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)
case "$distro" in
  ubuntu)
    arch=${arch:-arm64}
    [[ "$arch" == amd64 || "$arch" == arm64 ]] || fail "--arch: amd64 or arm64"
    package=${package:-dist/Prism_${version}_${arch}.deb}
    [[ -f "$package" ]] || fail "no $package: just linux --arch $arch --bundles deb"
    dockerfile=Dockerfile copy=prism.deb tag=$arch ;;
  arch)
    arch=${arch:-amd64}
    [[ "$arch" == amd64 ]] || fail "--distro arch: amd64 only"
    package=${package:-dist/Prism_${version}_amd64.pkg.tar.zst}
    [[ -f "$package" ]] || fail "no $package: just linux --bundles pacman"
    dockerfile=Dockerfile.arch copy=prism.pkg.tar.zst tag=arch ;;
  *) fail "--distro: ubuntu or arch" ;;
esac
docker info >/dev/null 2>&1 || fail "Docker is not running"

ctx=$(mktemp -d "${TMPDIR:-/tmp}/prism-desktop.XXXXXX")
trap 'rm -rf "$ctx"' EXIT
cp "scripts/linux/desktop/$dockerfile" "$ctx/Dockerfile"
cp scripts/linux/desktop/start.sh "$ctx/"
cp "$package" "$ctx/$copy"
cp src-tauri/crates/core/demo/config.toml "$ctx/demo-config.toml"
image=prism-linux-desktop:$tag
docker build --platform "linux/$arch" -t "$image" "$ctx"

# VNC's passwords are 8 characters at most.
password=$(openssl rand -hex 4)
echo "Browser: http://localhost:6080/vnc.html?autoconnect=1&resize=remote&password=$password  ·  VNC: vnc://localhost:5901 (password $password)"
tty=()
[[ -t 0 ]] && tty=(-it)
docker run --rm ${tty[@]+"${tty[@]}"} --name "prism-desktop-$tag" --platform "linux/$arch" --shm-size=1g \
  -p 127.0.0.1:6080:6080 -p 127.0.0.1:5901:5901 -e GEOMETRY="$geometry" -e VNC_PASSWORD="$password" "$image"
