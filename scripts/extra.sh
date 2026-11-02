# A team's blocks for the packaging scripts (dmg.sh, linux.sh), sourced: their folder as `just`
# reads it, and the label the app shows after its version (src-tauri/build.rs, web/vite.config.ts).
#
#   extra_from_env          PRISM_EXTRA_BLOCKS and PRISM_EXTRA_NAME from .env, unless the environment sets them
#   extra_label <folder>    `acme 5786d86`: PRISM_EXTRA_NAME, else their repository's folder, and
#                           their commit, `+` with uncommitted changes in the folder

extra_from_env() {
  local key value
  for key in PRISM_EXTRA_BLOCKS PRISM_EXTRA_NAME; do
    [[ -z "${!key+x}" && -f .env ]] || continue
    value=$(sed -n "s/^$key=//p" .env | tail -1 | tr -d '"'"'")
    [[ -n "$value" ]] && printf -v "$key" '%s' "$value"
  done
  return 0
}

extra_label() {
  local dir=$1 sha name
  sha=$(git -C "$dir" rev-parse --short HEAD 2>/dev/null || true)
  if [[ -n "$sha" && -n "$(git -C "$dir" status --porcelain --untracked-files=no . 2>/dev/null)" ]]; then sha+="+"; fi
  name=${PRISM_EXTRA_NAME:-$(basename "$(git -C "$dir" rev-parse --show-toplevel 2>/dev/null || echo "$dir")")}
  echo "$name${sha:+ $sha}"
}
