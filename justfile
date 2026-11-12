# Prism's basic commands. `just` (or `just --list`) to see them.

# `.env` (git-ignored), if any: the release's settings (PRISM_RELEASE_TO, PRISM_SIGN_KEY…), for
# every recipe.
set dotenv-load
set ignore-comments


# macOS only: deploy and open; Linux and Windows install their bundle (deb, AppImage, NSIS).
APP := "src-tauri/target/release/bundle/macos/Prism.app"

[private]
default:
    @just --list

# Installs the frontend dependencies (and thus the Tauri CLI)
install:
    pnpm --dir web install

# Runs the app in development (Vite + Tauri window, hot reload)
dev:
    pnpm dev

# Runs the frontend alone in a browser (without Tauri)
web:
    pnpm --dir web dev

# Builds the example module, its back end (rustup target add wasm32-wasip2) then its web part, in examples/modules/hello/dist/hello: copy it into modules/ next to config.toml
module-example:
    cargo build --release --target wasm32-wasip2 --manifest-path examples/modules/hello/backend/Cargo.toml
    pnpm --dir examples/modules/hello install
    pnpm --dir examples/modules/hello exec vite build

# Checks that the module SDK alone (sdk/web, src-tauri/crates/module) builds the example module, and refuses what a module may not import
module-sdk-check:
    scripts/module-sdk-check.sh

# Checks the frontend types (svelte-check, tsc) and compiles the Rust
check:
    # The window's types, generated from the Rust ones (web/src/bindings): a change on one side
    # the other does not follow fails svelte-check.
    cargo test --quiet --manifest-path src-tauri/Cargo.toml export_bindings
    pnpm --dir web check
    cargo check --workspace --manifest-path src-tauri/Cargo.toml
    # The module SDK copies what Prism gives a module: in step with Prism's core and window.
    python3 scripts/module-sdk-types.py --check
    node web/scripts/module-sdk.mjs --check

# Runs the tests, Rust (cargo test) and frontend (vitest)
test:
    pnpm --dir web test
    cargo test --workspace --manifest-path src-tauri/Cargo.toml

# Builds the release bundle of this system (Prism.app, deb + AppImage, NSIS installer)
build:
    pnpm build

# Builds prism-server, Prism without a window, for this system (scripts/server.sh --help)
server *args:
    scripts/server.sh {{args}}

# Runs Prism as on a new machine (empty home, no config) to try the first-launch screen; your own setup is untouched
first-launch:
    #!/usr/bin/env bash
    set -euo pipefail
    # The release binary, frontend included, without the bundles: faster than `build`.
    web/node_modules/.bin/tauri build --no-bundle
    # A fresh home: no config, no profiles, its own data folder (what was created stays there).
    home=$(mktemp -d "${TMPDIR:-/tmp}/prism-first-launch.XXXXXX")
    echo "home: $home (rm -rf it when done)"
    HOME="$home" src-tauri/target/release/prism

# Builds the release app, replaces the one in /Applications and relaunches it (macOS)
[macos]
deploy: build
    # Every profile open is quit, then reopened on the new app (scripts/deploy.sh).
    scripts/deploy.sh {{APP}}

# Builds a .dmg to install Prism on another Mac (universal; scripts/dmg.sh --help)
[macos]
dmg *args:
    scripts/dmg.sh {{args}}

# Builds Linux packages (.deb, AppImage; .rpm, Arch) in Docker, from any system (scripts/linux.sh --help)
linux *args:
    scripts/linux.sh {{args}}

# Runs the Linux .deb (--distro arch: the Arch package) on a desktop in a container, seen in the browser (scripts/linux-desktop.sh --help)
linux-desktop *args:
    scripts/linux-desktop.sh {{args}}

# Checks that Prism compiles for Windows, from macOS or Linux (cargo xwin; WINDOWS_ARCH=arm64 for ARM)
windows-check *args:
    scripts/windows.sh check {{args}}

# Builds the Windows installer (NSIS, unsigned) from macOS or Linux: x64, WINDOWS_ARCH=arm64 for ARM
windows-build *args:
    scripts/windows.sh build {{args}}

# Builds prism-server.exe, Prism without a window, for Windows from macOS or Linux (cargo xwin; WINDOWS_ARCH=arm64 for ARM)
windows-server *args:
    scripts/windows.sh server {{args}}

# Prints the changelog since the previous release, the notes `just release` publishes
changelog *args:
    scripts/changelog.sh {{args}}

# Publishes a release that installed Prisms update from (macOS; scripts/release.sh --help)
[macos]
release *args:
    scripts/release.sh {{args}}

# Publishes a release for macOS, Linux and Windows, prism-server with it (scripts/release.sh --help)
[macos]
release-all *args:
    scripts/release.sh --server --linux --windows {{args}}

# Starts Tailscale for the remote beside a VPN (userspace tailscaled, no network change) and signs in; `just tailscale down` stops it
tailscale *args:
    scripts/tailscale.sh {{args}}

# Opens the built release app (macOS)
[macos]
open:
    open {{APP}}

# Removes the build artifacts (frontend and Rust)
clean:
    rm -rf web/dist
    cargo clean --manifest-path src-tauri/Cargo.toml
