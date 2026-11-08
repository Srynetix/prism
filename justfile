# Prism's basic commands. `just` (or `just --list`) to see them.

# `.env` (git-ignored), if any: PRISM_EXTRA_BLOCKS, the folder of a team's blocks (docs/guide.md ›
# Blocks of another repository), for every recipe. `NO_EXTRA=1` leaves them out: Prism alone, as
# without the .env (`NO_EXTRA=1 just build`, `NO_EXTRA=1 just deploy`, `NO_EXTRA=1 just dmg`…).
set dotenv-load
set ignore-comments

NO_EXTRA := env("NO_EXTRA", "")
# A command left out the blocks starts with this: emptied, the build reads them as none, build.rs
# and vite.config.ts.
no_extra := if NO_EXTRA != "" { "PRISM_EXTRA_BLOCKS= PRISM_EXTRA_NAME= PRISM_EXTRA_LABEL= " } else { "" }
no_extra_flag := if NO_EXTRA != "" { "--no-extra " } else { "" }

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
    {{no_extra}}pnpm dev

# Runs the frontend alone in a browser (without Tauri)
web:
    pnpm --dir web dev

# Checks the frontend types (svelte-check, tsc) and compiles the Rust
check:
    # The window's types, generated from the Rust ones (web/src/bindings): a change on one side
    # the other does not follow fails svelte-check.
    {{no_extra}}cargo test --quiet --manifest-path src-tauri/Cargo.toml export_bindings
    {{no_extra}}pnpm --dir web check
    # A team's blocks (PRISM_EXTRA_BLOCKS): their window side against Prism's.
    {{ if NO_EXTRA == "" { 'if [ -n "$PRISM_EXTRA_BLOCKS" ]; then extra="$(cd "$PRISM_EXTRA_BLOCKS" && pwd)/web"; pnpm --dir web exec svelte-check --workspace "$extra" --tsconfig "$extra/tsconfig.json"; fi' } else { "true" } }}
    {{no_extra}}cargo check --workspace --manifest-path src-tauri/Cargo.toml

# Runs the tests, Rust (cargo test) and frontend (vitest)
test:
    pnpm --dir web test
    cargo test --workspace --manifest-path src-tauri/Cargo.toml

# Builds the release bundle of this system (Prism.app, deb + AppImage, NSIS installer); NO_EXTRA=1 without the .env's blocks
build:
    {{no_extra}}pnpm build

# Builds prism-server, Prism without a window, for this system (scripts/server.sh --help); NO_EXTRA=1 without the .env's blocks
server *args:
    scripts/server.sh {{no_extra_flag}}{{args}}

# Runs Prism as on a new machine (empty home, no config) to try the first-launch screen; your own setup is untouched
first-launch:
    #!/usr/bin/env bash
    set -euo pipefail
    # The release binary, frontend included, without the bundles: faster than `build`.
    {{no_extra}}web/node_modules/.bin/tauri build --no-bundle
    # A fresh home: no config, no profiles, its own data folder (what was created stays there).
    home=$(mktemp -d "${TMPDIR:-/tmp}/prism-first-launch.XXXXXX")
    echo "home: $home (rm -rf it when done)"
    HOME="$home" src-tauri/target/release/prism

# Builds the release app, replaces the one in /Applications and relaunches it (macOS); NO_EXTRA=1 without the .env's blocks
[macos]
deploy: build
    # Every profile open is quit, then reopened on the new app (scripts/deploy.sh).
    scripts/deploy.sh {{APP}}

# Builds a .dmg to install Prism on another Mac (universal; scripts/dmg.sh --help)
[macos]
dmg *args:
    scripts/dmg.sh {{no_extra_flag}}{{args}}

# Builds Linux packages (.deb, AppImage; .rpm, Arch) in Docker, from any system (scripts/linux.sh --help)
linux *args:
    scripts/linux.sh {{no_extra_flag}}{{args}}

# Runs the Linux .deb (--distro arch: the Arch package) on a desktop in a container, seen in the browser (scripts/linux-desktop.sh --help)
linux-desktop *args:
    scripts/linux-desktop.sh {{args}}

# Checks that Prism compiles for Windows, from macOS or Linux (cargo xwin; WINDOWS_ARCH=arm64 for ARM)
windows-check *args:
    {{no_extra}}scripts/windows.sh check {{args}}

# Builds the Windows installer (NSIS, unsigned) from macOS or Linux: x64, WINDOWS_ARCH=arm64 for ARM
windows-build *args:
    {{no_extra}}scripts/windows.sh build {{args}}

# Prints the changelog since the previous release, the notes `just release` publishes
changelog *args:
    scripts/changelog.sh {{args}}

# Publishes a release that installed Prisms update from (macOS; scripts/release.sh --help)
[macos]
release *args:
    scripts/release.sh {{no_extra_flag}}{{args}}

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
