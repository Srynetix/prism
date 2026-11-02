#!/usr/bin/env bash
# Installs a built Prism.app in /Applications and reopens the profiles that were open.
#
#   scripts/deploy.sh <path/to/Prism.app>
#
# Each profile open runs its own Prism (its pid in instance.pid, in the profile's data folder):
# they are all quit, the app replaced, then each one opened again on its profile, the default
# first (it serves the phone; the others reach it through it). None open: the default profile's
# (or the one chosen last) opens, as before. The agents keep running: their tmux servers are not
# Prism's. Run from a phone (a shell in Prism), the shell survives the quit for the same reason.
set -euo pipefail

app="${1:?usage: scripts/deploy.sh <path/to/Prism.app>}"
dest=/Applications/Prism.app
base="$HOME/Library/Application Support/app.prism.desktop"

# A running Prism (a pid left by a crash may have been reused).
alive() { kill -0 "$1" 2>/dev/null && ps -p "$1" -o comm= | grep -qi prism; }

profiles=()
for f in "$base/instance.pid" "$base"/profiles/*/instance.pid; do
  [ -f "$f" ] || continue
  pid=$(tr -d '[:space:]' <"$f")
  [ -n "$pid" ] && alive "$pid" || continue
  if [ "$f" = "$base/instance.pid" ]; then profiles+=(default); else profiles+=("$(basename "$(dirname "$f")")"); fi
done

# Closing a window does not quit Prism (menu bar): quit for good. The Apple event reaches one
# Prism at a time, so it is sent until none is left; one that does not answer gets SIGTERM.
# Only the apps themselves: the executable alone or with --profile. Not every process whose
# command line names it: a tmux server and its agents carry `…/prism hook` (Codex's hooks on its
# command line), and they must outlive Prism.
running() { ps -axo pid=,args= | awk '$2 ~ /Prism\.app\/Contents\/MacOS\/prism$/ && ($3 == "" || $3 == "--profile") { print $1 }'; }
for _ in $(seq 1 15); do
  [ -z "$(running)" ] && break
  osascript -e 'tell application "Prism" to quit' 2>/dev/null || true
  sleep 1
done
if [ -n "$(running)" ]; then
  echo "Prism did not quit: SIGTERM to $(running | tr '\n' ' ')"
  running | xargs kill -TERM 2>/dev/null || true
  sleep 2
fi

rm -rf "$dest"
cp -R "$app" /Applications/

if [ ${#profiles[@]} -eq 0 ]; then
  open "$dest"
  echo "Prism.app installed in /Applications and opened"
  exit 0
fi
first=1
for id in "${profiles[@]}"; do
  # -n: a Prism of its own for each profile after the first.
  if [ $first -eq 1 ]; then open "$dest" --args --profile "$id"; first=0; else open -n "$dest" --args --profile "$id"; fi
  sleep 2
done
echo "Prism.app installed in /Applications and reopened: ${profiles[*]}"
