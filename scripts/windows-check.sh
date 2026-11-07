#!/usr/bin/env bash
# Checks that Prism compiles for Windows, from macOS or Linux: `cargo xwin check` of the whole
# workspace, its tests included, for x86_64-pc-windows-msvc. Nothing is linked: whisper-rs-sys's
# build script reads the system it runs on, not the target, and asks for macOS's libraries.
#
# Needs clang-cl, lld-link and ninja (macOS: `brew install llvm lld ninja`), cargo-xwin
# (`cargo install --locked cargo-xwin`) and the Rust target (`rustup target add
# x86_64-pc-windows-msvc`). cargo-xwin downloads Microsoft's CRT and Windows SDK on the first run:
# XWIN_ACCEPT_LICENSE=1 accepts their license.
set -euo pipefail
cd "$(dirname "$0")/../src-tauri"

if [ "${1:-}" = "--help" ]; then
    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
fi

for brew in /opt/homebrew/opt /usr/local/opt; do
    [ -d "$brew/llvm/bin" ] && PATH="$brew/llvm/bin:$PATH"
    [ -d "$brew/lld/bin" ] && PATH="$brew/lld/bin:$PATH"
done
export PATH
for tool in cargo-xwin clang-cl lld-link ninja cmake; do
    command -v "$tool" >/dev/null || { echo "windows-check: $tool not found (scripts/windows-check.sh --help)" >&2; exit 1; }
done
if [ -z "${XWIN_ACCEPT_LICENSE:-}" ]; then
    echo "windows-check: set XWIN_ACCEPT_LICENSE=1 to accept the license of Microsoft's CRT and SDK that cargo-xwin downloads" >&2
    exit 1
fi
exec cargo xwin check --workspace --tests --target x86_64-pc-windows-msvc "$@"
