#!/usr/bin/env bash
# Prism for Windows x64 (x86_64-pc-windows-msvc), from macOS or Linux, with cargo-xwin.
#
#   scripts/windows.sh check [cargo args]   cargo check of the workspace and its tests
#   scripts/windows.sh build [tauri args]   the NSIS installer, unsigned:
#                                           src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis
#
# Needs clang-cl, lld-link, ninja and cmake (macOS: `brew install llvm lld ninja cmake`), makensis
# to build (`brew install makensis`), cargo-xwin (`cargo install --locked cargo-xwin`) and the Rust
# target (`rustup target add x86_64-pc-windows-msvc`). cargo-xwin downloads Microsoft's CRT and
# Windows SDK on the first run: XWIN_ACCEPT_LICENSE=1 accepts their license.
#
# whisper-rs-sys's build script reads the system it runs on, not the target: from macOS it asks
# for ggml-blas (never built for Windows) and leaves out advapi32. The build gives it an empty
# ggml-blas.lib and advapi32.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
target=x86_64-pc-windows-msvc

usage() { sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; }
mode="${1:-}"
case "$mode" in
    check | build) shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
esac

for brew in /opt/homebrew/opt /usr/local/opt; do
    [ -d "$brew/llvm/bin" ] && PATH="$brew/llvm/bin:$PATH"
    [ -d "$brew/lld/bin" ] && PATH="$brew/lld/bin:$PATH"
done
export PATH
tools="cargo-xwin clang-cl lld-link ninja cmake"
[ "$mode" = build ] && tools="$tools makensis llvm-lib clang"
for tool in $tools; do
    command -v "$tool" >/dev/null || { echo "windows: $tool not found (scripts/windows.sh --help)" >&2; exit 1; }
done
if [ -z "${XWIN_ACCEPT_LICENSE:-}" ]; then
    echo "windows: set XWIN_ACCEPT_LICENSE=1 to accept the license of Microsoft's CRT and SDK that cargo-xwin downloads" >&2
    exit 1
fi

if [ "$mode" = check ]; then
    cd "$root/src-tauri"
    exec cargo xwin check --workspace --tests --target "$target" "$@"
fi

libs="$root/src-tauri/target/$target/xwin-libs"
mkdir -p "$libs"
if [ ! -f "$libs/ggml-blas.lib" ]; then
    printf 'void prism_no_blas(void) {}\n' > "$libs/empty.c"
    clang --target="$target" -c "$libs/empty.c" -o "$libs/empty.obj"
    llvm-lib /out:"$libs/ggml-blas.lib" "$libs/empty.obj"
fi
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-L native=$libs -l advapi32"
cd "$root"
web/node_modules/.bin/tauri build --runner cargo-xwin --target "$target" "$@"
ls "$root/src-tauri/target/$target/release/bundle/nsis/"*-setup.exe
