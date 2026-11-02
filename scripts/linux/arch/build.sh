#!/usr/bin/env bash
# Runs in an archlinux:base-devel container, started by scripts/linux.sh: /in the .deb and the
# licence, /pkg this folder (read only), /out the Arch package. makepkg refuses root: a user of
# its own builds it.
set -euo pipefail

useradd -m builder
work=/home/builder/pkg
mkdir -p "$work"
cp /pkg/PKGBUILD "$work/"
cp /in/prism.deb /in/LICENSE "$work/"
chown -R builder: "$work"
# --nodeps: its dependencies are what installing it pulls in, not what repackaging needs.
su builder -c "cd $work && PRISM_PKGVER='$PRISM_PKGVER' makepkg --nodeps --noconfirm" >&2
cp "$work"/prism-*.pkg.tar.zst /out/
