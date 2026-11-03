#!/usr/bin/env bash
# Tailscale for Prism's remote, beside a VPN: the open-source tailscaled in userspace mode, which
# creates no network interface, route nor DNS setting (the Tailscale app does, and fights a VPN
# for them). It only connects out to the tailnet; `tailscale serve`, run by Prism, brings phones
# to Prism's loopback port through it. Nothing else of the computer, nor of the VPN's network, is
# reachable: no subnet router, no exit node.
#
#   scripts/tailscale.sh [up | down | status]
#
# - up (default): starts tailscaled if it is not running, then signs in (`tailscale up`: the first
#   time, it prints an address to open). Then set `[remote] tailscale_socket` to the socket it prints.
# - down: signs this node out of the tailnet's traffic and stops tailscaled; its login is kept.
# - status: the node as the tailnet sees it.
#
# Needs Homebrew's tailscale (`brew install tailscale`: tailscaled and its CLI), not the app's CLI.
# Its state, socket and log: ~/.local/share/prism/tailscale (PRISM_TAILSCALE_DIR).
set -euo pipefail

dir="${PRISM_TAILSCALE_DIR:-$HOME/.local/share/prism/tailscale}"
sock="$dir/tailscaled.sock"
pidfile="$dir/tailscaled.pid"
prefix="$(brew --prefix tailscale 2>/dev/null || true)"
daemon="$prefix/bin/tailscaled"
cli="$prefix/bin/tailscale"
if [[ -z "$prefix" || ! -x "$daemon" ]]; then
  echo "tailscaled not found: brew install tailscale" >&2
  exit 1
fi
ts() { "$cli" --socket="$sock" "$@"; }
running() { [[ -f "$pidfile" ]] && kill -0 "$(cat "$pidfile")" 2>/dev/null; }

case "${1:-up}" in
  up)
    mkdir -p "$dir"
    chmod 700 "$dir"
    if ! running; then
      # As this user, not root: userspace mode needs no privilege. --port=0: any UDP port.
      nohup "$daemon" --tun=userspace-networking --socket="$sock" --statedir="$dir" --port=0 >>"$dir/tailscaled.log" 2>&1 &
      echo $! >"$pidfile"
      for _ in $(seq 50); do
        [[ -S "$sock" ]] && break
        sleep 0.2
      done
      [[ -S "$sock" ]] || { echo "tailscaled did not start: see $dir/tailscaled.log" >&2; exit 1; }
      echo "tailscaled started (userspace), log: $dir/tailscaled.log"
    fi
    # Its own name in the tailnet, apart from the Tailscale app's node of this computer.
    ts up --hostname="prism-$(hostname -s | tr '[:upper:]' '[:lower:]')"
    ts status --self --peers=false || true
    echo
    echo "In config.toml (Settings › Remote › Access):"
    echo "  [remote]"
    echo "  tailscale_socket = \"$sock\""
    ;;
  down)
    if running; then
      ts down || true
      kill "$(cat "$pidfile")"
      rm -f "$pidfile"
      echo "tailscaled stopped"
    else
      echo "tailscaled is not running"
    fi
    ;;
  status)
    running || { echo "tailscaled is not running: scripts/tailscale.sh up"; exit 1; }
    ts status --self --peers=false
    ;;
  *)
    echo "usage: scripts/tailscale.sh [up | down | status]" >&2
    exit 2
    ;;
esac
