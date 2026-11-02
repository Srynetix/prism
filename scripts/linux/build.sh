#!/usr/bin/env bash
# Runs in scripts/linux/Dockerfile's image, started by scripts/linux.sh: /src the sources (read
# only), /extra a team's blocks (optional), /cache what is kept between builds, /out the bundles.
set -euo pipefail

# A copy of the sources, updated in place, so that cargo and pnpm rebuild only what changed.
rsync -a --delete --exclude /web/node_modules --exclude /src-tauri/target --exclude /web/dist --exclude /dist --exclude /.env /src/ /cache/work/
cd /cache/work
# A team's blocks find Prism next to them (their tsconfig extends ../prism/web/…): from /extra, /prism.
[[ -e /prism ]] || ln -s /cache/work /prism

export CARGO_HOME=/cache/cargo CARGO_TARGET_DIR=/cache/target
# linuxdeploy (the AppImage) is an AppImage itself: without FUSE in a container, it unpacks itself.
export APPIMAGE_EXTRACT_AND_RUN=1
pnpm config set store-dir /cache/pnpm-store >/dev/null
pnpm --dir web install --frozen-lockfile

bundle=$CARGO_TARGET_DIR/release/bundle
# The volume keeps the bundles of earlier builds: only this one's go out.
rm -rf "$bundle"
web/node_modules/.bin/tauri build --bundles "$BUNDLES"

shopt -s nullglob
files=("$bundle"/deb/*.deb "$bundle"/appimage/*.AppImage "$bundle"/rpm/*.rpm)
[[ ${#files[@]} -gt 0 ]] || { echo "no bundle in $bundle" >&2; exit 1; }
for f in "${files[@]}"; do
  cp "$f" /out/
  (cd /out && sha256sum "$(basename "$f")" > "$(basename "$f").sha256")
  echo "/out/$(basename "$f")"
done
