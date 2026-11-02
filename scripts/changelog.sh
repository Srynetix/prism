#!/usr/bin/env bash
# Prints, in Markdown, what changed since the previous release: the release notes that
# scripts/release.sh publishes, and Settings › General › Updates shows before installing.
#
#   scripts/changelog.sh [--since <commit>] [<until>]
#
# - From the commits' Conventional subjects (`feat: …`, `fix(scope): …`): Features, Fixes and
#   Performance, in the order they were made; the rest (refactor, test, docs, chore…) is only
#   counted. A `!` (`feat!: …`) or a `BREAKING CHANGE` in the body marks a change to read first.
# - Since the previous release: the newest prism-v* tag before <until> (HEAD by default), which
#   scripts/release.sh sets on the commit it publishes; without one, the commit that set the
#   previous version of src-tauri/Cargo.toml. --since names it instead.
set -euo pipefail

cd "$(dirname "$0")/.."

since=""
until=HEAD
while [[ $# -gt 0 ]]; do
  case "$1" in
    --since) since=${2:-}; shift ;;
    -h | --help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
    *) until=$1 ;;
  esac
  shift
done

if [[ -z "$since" ]]; then
  since=$(git describe --tags --abbrev=0 --match 'prism-v*' "$until^" 2>/dev/null || true)
fi
if [[ -z "$since" ]]; then
  # The version bumps, newest first: this release's, then the previous one's.
  since=$(git log --format=%H -G '^version = ' "$until" -- src-tauri/Cargo.toml | sed -n 2p)
fi
range=${since:+$since..}$until

features=() fixes=() perf=() breaking=()
other=0
# One commit per record: subject, unit separator, body.
while IFS= read -r -d $'\x1e' record; do
  record=${record#$'\n'}
  subject=${record%%$'\x1f'*}
  body=${record#*$'\x1f'}
  [[ -n "$subject" ]] || continue
  re='^([a-z]+)(\([^)]*\))?(!?): (.+)$'
  if [[ ! $subject =~ $re ]]; then
    other=$((other + 1))
    continue
  fi
  type=${BASH_REMATCH[1]}
  scope=${BASH_REMATCH[2]}
  bang=${BASH_REMATCH[3]}
  text=${BASH_REMATCH[4]}
  scope=${scope#(}
  scope=${scope%)}
  line="${scope:+**$scope**: }$text"
  if [[ -n "$bang" || $body == *"BREAKING CHANGE"* ]]; then breaking+=("$line"); fi
  case "$type" in
    feat) features+=("$line") ;;
    fix) fixes+=("$line") ;;
    perf) perf+=("$line") ;;
    *) other=$((other + 1)) ;;
  esac
done < <(git log --reverse --no-merges --format='%s%x1f%b%x1e' "$range")

section() {
  local title=$1
  shift
  [[ $# -gt 0 ]] || return 0
  printf '### %s\n\n' "$title"
  printf -- '- %s\n' "$@"
  printf '\n'
}

section "Read first" ${breaking[@]+"${breaking[@]}"}
section "Features" ${features[@]+"${features[@]}"}
section "Fixes" ${fixes[@]+"${fixes[@]}"}
section "Performance" ${perf[@]+"${perf[@]}"}
if [[ $other -gt 0 ]]; then
  printf '%s other change%s (refactoring, tests, docs).\n' "$other" "$([[ $other -gt 1 ]] && echo s)"
elif [[ ${#features[@]} -eq 0 && ${#fixes[@]} -eq 0 && ${#perf[@]} -eq 0 ]]; then
  printf 'No change since %s.\n' "${since:-the first commit}"
fi
