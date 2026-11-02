#!/usr/bin/env bash
# Publishes this Prism as a release that installed Prisms update from (Settings › General › Updates,
# `[updates] source`): the macOS app, and with --linux the Linux packages.
#
#   scripts/release.sh --to glab:group/project|gh:owner/repo [--linux] [--no-extra] [--skip-checks]
#                      [--rebuild] [--notes "text" | --notes-file file] [--no-tag] [--dry-run]
#
# - --to: where it is published, as [updates] source names it; PRISM_RELEASE_TO (environment or
#   .env) by default. A team's build (its blocks, PRISM_EXTRA_BLOCKS) goes to the team's private
#   repository, the one its pack names; Prism alone (--no-extra) to the public one.
# - The release is tagged prism-v<version>, the version of src-tauri/Cargo.toml: bump it first,
#   an existing release is refused.
# - Files: Prism_<version>_macos-universal.dmg (a first install), .app.tar.gz (what an update
#   replaces), and with --linux Prism_<version>_amd64.deb, .AppImage and .pkg.tar.zst (Arch
#   Linux), each with its .sha256
#   and its .minisig: signed with minisign (brew install minisign), with the key PRISM_SIGN_KEY
#   names (environment or .env; ~/.minisign/prism.key by default), whose public half is
#   src-tauri/update-key.pub. An installed Prism installs only a file signed with it.
#   On GitLab they go to the project's generic packages (prism/<version>), the release links to
#   them; on GitHub they are the release's assets.
# - Notes: the changelog since the previous release (scripts/changelog.sh, from the Conventional
#   commit subjects), which Settings › General › Updates shows before installing; --notes or
#   --notes-file replace it.
# - Once published, the commit is tagged prism-v<version> in this clone (not pushed): the next
#   changelog starts from it; its message is the changelog, without the line naming the build and
#   its blocks, so the tag can go to Prism's public repository. --no-tag leaves it out.
# - --dry-run builds and prints what would be published, without publishing.
# - Builds with scripts/dmg.sh and scripts/linux.sh (Docker): their checks and refusals apply. A
#   build of this commit (and of these blocks) already in dist/ is published as is, without
#   building again (`just dmg`, `just linux`, a dry run…); --rebuild builds anyway.
set -euo pipefail

cd "$(dirname "$0")/.."

to=""
linux=false
extra=true
checks=true
dry=false
rebuild=false
tag_it=true
notes=""
notes_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --to) to=${2:-}; shift ;;
    --linux) linux=true ;;
    --no-extra) extra=false ;;
    --skip-checks) checks=false ;;
    --notes) notes=${2:-}; shift ;;
    --notes-file) notes_file=${2:-}; shift ;;
    --no-tag) tag_it=false ;;
    --dry-run) dry=true ;;
    --rebuild) rebuild=true ;;
    -h | --help) sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

step() { printf '\n\033[1m▸ %s\033[0m\n' "$*"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

[[ "$(uname -s)" == Darwin ]] || fail "a release is built on macOS (the app; Linux in Docker)"
if [[ -z "$to" && -z "${PRISM_RELEASE_TO+x}" && -f .env ]]; then
  to=$(sed -n 's/^PRISM_RELEASE_TO=//p' .env | tail -1 | tr -d '"'"'")
fi
to=${to:-${PRISM_RELEASE_TO:-}}
case "$to" in
  glab:*/*) forge=glab; project=${to#glab:} ;;
  gh:*/*) forge=gh; project=${to#gh:} ;;
  *) fail "--to glab:group/project or gh:owner/repo (or PRISM_RELEASE_TO), not '${to}'" ;;
esac
command -v "$forge" >/dev/null || fail "$forge not found"
# The signing key, checked before anything is built.
if [[ -z "${PRISM_SIGN_KEY:-}" && -f .env ]]; then
  PRISM_SIGN_KEY=$(sed -n 's/^PRISM_SIGN_KEY=//p' .env | tail -1 | tr -d '"'"'")
fi
sign_key=${PRISM_SIGN_KEY:-$HOME/.minisign/prism.key}
sign_key=${sign_key/#\~/$HOME}
command -v minisign >/dev/null || fail "minisign not found (brew install minisign)"
[[ -f "$sign_key" ]] || fail "no signing key $sign_key (PRISM_SIGN_KEY): minisign -G -p src-tauri/update-key.pub -s $sign_key"
[[ -f src-tauri/update-key.pub ]] || fail "no src-tauri/update-key.pub: the public key installed Prisms check"
[[ -z "$notes_file" || -f "$notes_file" ]] || fail "--notes-file: no $notes_file"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)
[[ -n "$version" ]] || fail "no version in src-tauri/Cargo.toml"
tag="prism-v$version"
host=${GITLAB_HOST:-gitlab.com}
host=${host#https://}
enc=$(printf '%s' "$project" | sed 's|/|%2F|g')

step "Prism $version → $to ($tag)"
if [[ $forge == glab ]]; then
  if glab api --hostname "$host" "projects/$enc/releases/$tag" >/dev/null 2>&1; then fail "$tag is already released in $project: bump the version in src-tauri/Cargo.toml"; fi
  glab api --hostname "$host" "projects/$enc" >/dev/null || fail "no access to $project with glab"
else
  if gh release view "$tag" -R "$project" >/dev/null 2>&1; then fail "$tag is already released in $project: bump the version in src-tauri/Cargo.toml"; fi
  gh repo view "$project" >/dev/null || fail "no access to $project with gh"
fi

# A release is a commit: the builds name it (dmg.sh, linux.sh), and are reused for it.
[[ -z "$(git status --porcelain --untracked-files=no)" ]] || fail "uncommitted changes: commit them first"
commit=$(git rev-parse --short HEAD)
label=""
if $extra; then
  source scripts/extra.sh
  extra_from_env
  [[ -z "${PRISM_EXTRA_BLOCKS:-}" ]] || label=$(extra_label "$PRISM_EXTRA_BLOCKS")
else
  export PRISM_EXTRA_BLOCKS=""
fi

opts=()
$extra || opts+=(--no-extra)
$checks || opts+=(--skip-checks)

step "macOS"
# The name dmg.sh gives this commit and these blocks.
dmg="dist/Prism_${version}_${commit}${label:+_$(echo "$label" | sed 's/+$/-dirty/; s/ /-/g')}_universal.dmg"
if [[ -f "$dmg" ]] && ! $rebuild; then
  echo "$dmg already built: reused (--rebuild to build again)"
else
  scripts/dmg.sh ${opts[@]+"${opts[@]}"}
  [[ -f "$dmg" ]] || fail "no $dmg after scripts/dmg.sh"
fi

out=dist/release-$version
rm -rf "$out"
mkdir -p "$out"
cp "$dmg" "$out/Prism_${version}_macos-universal.dmg"
# The app of the .dmg itself, without macOS' ._ files: what an update installs is what a first
# install gets, its signature included.
mnt=$(mktemp -d "${TMPDIR:-/tmp}/prism-release.XXXXXX")
trap 'hdiutil detach -quiet "$mnt" 2>/dev/null || true; rmdir "$mnt" 2>/dev/null || true' EXIT
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mnt" "$dmg"
[[ -d "$mnt/Prism.app" ]] || fail "no Prism.app in $dmg"
COPYFILE_DISABLE=1 tar -czf "$out/Prism_${version}_macos-universal.app.tar.gz" -C "$mnt" Prism.app
hdiutil detach -quiet "$mnt"

if $linux; then
  step "Linux"
  # linux.sh notes which commit and blocks its packages are of.
  stamp=dist/Prism_${version}_amd64.build
  if [[ -f "$stamp" && "$(cat "$stamp")" == "$commit|$label" && -f dist/Prism_${version}_amd64.deb && -f dist/Prism_${version}_amd64.AppImage && -f dist/Prism_${version}_amd64.pkg.tar.zst ]] && ! $rebuild; then
    echo "dist/Prism_${version}_amd64.{deb,AppImage,pkg.tar.zst} already built: reused (--rebuild to build again)"
  else
    scripts/linux.sh --skip-checks --bundles deb,appimage,pacman ${opts[@]+"${opts[@]}"}
  fi
  for ext in deb AppImage pkg.tar.zst; do
    f=dist/Prism_${version}_amd64.$ext
    [[ -f "$f" ]] || fail "no $f"
    cp "$f" "$out/"
  done
fi

(cd "$out" && for f in Prism_*; do [[ $f == *.sha256 ]] || shasum -a 256 "$f" > "$f.sha256"; done)
step "Signing (the key's password, once per file)"
# The file's name goes along, signed (the trusted comment): a signed file cannot pass for another.
for f in "$out"/Prism_*; do
  [[ $f == *.sha256 || $f == *.minisig ]] && continue
  minisign -S -s "$sign_key" -m "$f" -t "file:$(basename "$f")" >/dev/null || fail "signing $(basename "$f")"
  # With the key the app checks: a signature it would refuse fails here, not on every machine.
  minisign -V -q -p src-tauri/update-key.pub -m "$f" >/dev/null || fail "$(basename "$f") does not verify with src-tauri/update-key.pub: PRISM_SIGN_KEY is another key"
done
files=("$out"/Prism_*)

if [[ -n "$notes_file" ]]; then
  notes=$(cat "$notes_file")
elif [[ -z "$notes" ]]; then
  changes=$(scripts/changelog.sh)
  notes="$changes

Prism $version, built from $commit${label:+ with the blocks of $label}."
fi

step "Files"
for f in "${files[@]}"; do echo "  $(basename "$f") ($(du -h "$f" | cut -f1))"; done
if $dry; then
  step "Dry run: nothing published"
  printf 'Would publish %s to %s, notes:\n%s\n' "$tag" "$to" "$notes"
  exit 0
fi

step "Publishing"
if [[ $forge == glab ]]; then
  links="["
  for f in "${files[@]}"; do
    name=$(basename "$f")
    path="projects/$enc/packages/generic/prism/$version/$name"
    echo "  ↑ $name"
    glab api --hostname "$host" -X PUT "$path" --input "$f" >/dev/null
    type=package
    [[ $name == *.sha256 || $name == *.minisig ]] && type=other
    links+="{\"name\":\"$name\",\"url\":\"https://$host/api/v4/$path\",\"link_type\":\"$type\"},"
  done
  links="${links%,}]"
  GITLAB_HOST=$host glab release create "$tag" -R "$project" --name "Prism $version" --notes "$notes" --assets-links "$links"
else
  gh release create "$tag" -R "$project" --title "Prism $version" --notes "$notes" "${files[@]}"
fi

if $tag_it; then
  step "Tag"
  if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "$tag already tagged here: left as is"
  else
    # Verbatim: the changelog's headings start with #, which git would take for comments. The
    # changelog without its build line: a tag is pushed with the code, a team's blocks stay with the team.
    git tag -a --cleanup=verbatim "$tag" -m "Prism $version" -m "${changes:-$notes}"
    echo "$tag on $commit, in this clone only (git push origin $tag to share it)"
  fi
fi

step "Done"
echo "Prism $version published to $to: Prisms whose [updates] source is $to see it within 6 hours (Settings › General › Updates › Check now: at once)."
