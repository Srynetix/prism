#!/usr/bin/env bash
# Prism for Windows, x64 (x86_64-pc-windows-msvc) or arm64 (aarch64-pc-windows-msvc,
# WINDOWS_ARCH=arm64), from macOS or Linux, with cargo-xwin.
#
#   scripts/windows.sh check [cargo args]   cargo check of the workspace and its tests
#   scripts/windows.sh build [tauri args]   the NSIS installer, unsigned:
#                                           src-tauri/target/<target>/release/bundle/nsis
#
# Needs clang-cl, lld-link, ninja and cmake (macOS: `brew install llvm lld ninja cmake`), makensis
# to build (`brew install makensis`), cargo-xwin (`cargo install --locked cargo-xwin`) and the Rust
# target (`rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc`). cargo-xwin
# downloads Microsoft's CRT and Windows SDK on the first run: XWIN_ACCEPT_LICENSE=1 accepts their
# license.
#
# whisper-rs-sys's build script reads the system it runs on, not the target: from macOS it asks
# for ggml-blas (never built for Windows) and leaves out advapi32. The build gives it an empty
# ggml-blas.lib and advapi32.
# On arm64, two wrappers of the build's go first on the PATH: ring compiles with `clang` called by
# name, given cargo-xwin's clang-cl flags (`clang` reads their `/imsvc` as `-isystem`); psm's GNU
# assembly reaches `clang-cl`, which cannot preprocess it (`clang-cl` hands a `.s` to clang,
# preprocessed as psm does elsewhere, with its `CFG_TARGET_*`).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
case "${WINDOWS_ARCH:-x64}" in
    x64) target=x86_64-pc-windows-msvc ;;
    arm64) target=aarch64-pc-windows-msvc ;;
    *) echo "windows: WINDOWS_ARCH x64 or arm64, not ${WINDOWS_ARCH}" >&2; exit 2 ;;
esac

usage() { sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; }
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
tools="cargo-xwin clang-cl lld-link ninja cmake clang"
[ "$mode" = build ] && tools="$tools makensis llvm-lib"
for tool in $tools; do
    command -v "$tool" >/dev/null || { echo "windows: $tool not found (scripts/windows.sh --help)" >&2; exit 1; }
done
if [ -z "${XWIN_ACCEPT_LICENSE:-}" ]; then
    echo "windows: set XWIN_ACCEPT_LICENSE=1 to accept the license of Microsoft's CRT and SDK that cargo-xwin downloads" >&2
    exit 1
fi

work="$root/src-tauri/target/$target/xwin-libs"
mkdir -p "$work"
if [ "$target" = aarch64-pc-windows-msvc ]; then
    mkdir -p "$work/bin"
    cat > "$work/bin/clang" <<EOF
#!/usr/bin/env bash
args=()
for a in "\$@"; do
    case "\$a" in
        /imsvc) args+=("-isystem") ;;
        /imsvc*) args+=("-isystem" "\${a#/imsvc}") ;;
        *) args+=("\$a") ;;
    esac
done
exec "$(command -v clang)" "\${args[@]}"
EOF
    cat > "$work/bin/clang-cl" <<EOF
#!/usr/bin/env bash
src="" out="" incs=() next=""
for a in "\$@"; do
    if [ "\$next" = o ]; then out="\$a"; next=""; continue; fi
    if [ "\$next" = I ]; then incs+=("-I\$a"); next=""; continue; fi
    case "\$a" in
        *.s) src="\$a" ;;
        -o) next=o ;;
        /I | -I) next=I ;;
        /Fo* | -Fo*) out="\${a#?Fo}" ;;
        /I* | -I*) incs+=("-I\${a#?I}") ;;
    esac
done
[ -n "\$src" ] || exec "$(command -v clang-cl)" "\$@"
exec "$(command -v clang)" --target=aarch64-pc-windows-msvc -x assembler-with-cpp -DCFG_TARGET_OS_windows=1 -DCFG_TARGET_ARCH_aarch64=1 -DCFG_TARGET_ENV_msvc=1 "\${incs[@]}" -c "\$src" -o "\$out"
EOF
    chmod +x "$work/bin/clang" "$work/bin/clang-cl"
    PATH="$work/bin:$PATH"
fi

if [ "$mode" = check ]; then
    cd "$root/src-tauri"
    exec cargo xwin check --workspace --tests --target "$target" "$@"
fi

if [ ! -f "$work/ggml-blas.lib" ]; then
    printf 'void prism_no_blas(void) {}\n' > "$work/empty.c"
    "$(command -v clang)" --target="$target" -c "$work/empty.c" -o "$work/empty.obj"
    llvm-lib /out:"$work/ggml-blas.lib" "$work/empty.obj"
fi
flags="-L native=$work -l advapi32"
case "$target" in
    x86_64-pc-windows-msvc) export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="$flags" ;;
    aarch64-pc-windows-msvc) export CARGO_TARGET_AARCH64_PC_WINDOWS_MSVC_RUSTFLAGS="$flags" ;;
esac
cd "$root"
web/node_modules/.bin/tauri build --runner cargo-xwin --target "$target" "$@"
ls "$root/src-tauri/target/$target/release/bundle/nsis/"*-setup.exe
