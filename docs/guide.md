# Building Prism

How to develop, check, build, package and release Prism. Using it (sources, settings, blocks,
packs, keys) is documented inside the app: **Help** (⌘? or F1), the same guides as
`web/src/help/`. The [README](../README.md) lists what each source needs;
[config.example.toml](../config.example.toml) annotates every setting.

## Prerequisites

[Rust](https://rustup.rs), Node.js with [pnpm](https://pnpm.io), [just](https://just.systems)
(`brew install just`) and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
of your system. On Linux, also `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`,
`librsvg2-dev` and `libdbus-1-dev` (keyring). Docker (OrbStack, Docker Desktop) for the Linux
packages built from another system.

## Everyday commands

`just` shows them all (`justfile`):

| Command | Effect |
|---|---|
| `just install` | frontend dependencies, and the Tauri CLI |
| `just dev` | the app in development, hot reload (Vite + Tauri window) |
| `just web` | the frontend alone, in a browser (see below) |
| `just check` | regenerates the window's types from Rust, then svelte-check, tsc and `cargo check` of every crate |
| `just test` | vitest and `cargo test` of every crate |
| `just build` | the release bundle of this system |
| `just first-launch` | Prism as on a new machine (empty home, no config), your setup untouched |
| `just deploy` | macOS: release build, replaces `/Applications/Prism.app` and relaunches it |
| `just dmg` / `just linux` | packages to install elsewhere (below) |
| `just changelog` / `just release` | the notes since the last release, and publishing one (below) |
| `just open` / `just clean` | opens the built app / removes the build artifacts |

The window's types in `web/src/bindings/` are generated from the Rust ones by `cargo test`
(`export_bindings`, in the app's crate: the crates derive them with their `ts` feature): never edit them by hand. Type-check the frontend with
`pnpm --dir web check`; a bare `svelte-check` reads no tsconfig and checks nothing.

### Crates

`src-tauri/` is a Cargo workspace. Its root package, `prism`, is the app: the Tauri shell (window,
tray, menu, the window's commands, launch) over the library crates of `src-tauri/crates/`. Each is
a layer that depends only on the ones below it, and none on Tauri:

| Crate | Holds |
|---|---|
| `prism-base` | programs run with the login shell's environment, git, HTTP, the keyring, fonts, the runtime the collectors run on |
| `prism-core` | the config (packs, profiles, settings), the snapshot and its hub, conventions, the forge and tracker types, the blocks' contract |
| `prism-collectors` | the sources (forge, tracker, CI, clusters, calendar, mail, usage, vulnerabilities…), and `Forge` / `Tracker` over the providers |
| `prism-agents` | the coding agents: tmux sessions, worktrees, hooks, files, tasks, chains |
| `prism-blocks` | Prism's own blocks |
| `prism-demo` | the demo profile's data |
| `prism-ops` | checks, clones and new projects, feedback, updates |
| `prism-remote` | Prism from a phone: its server, pairing, passkeys, the tunnel |
| `prism-commands` | Prism running without its window: the state the commands share, the commands and their table, the subcommands (`prism pack`, `prism pair`…); Tauri an optional feature |
| `prism-server` | the binary without a window (below, *Without a window*) |

A crate's modules keep the paths they had in a single crate: each crate names the ones below it
as modules of its own (`use prism_core::{config, state}`, `use prism_collectors as collectors`),
so `crate::config::Config` still reads the same everywhere. What Tauri provides (its runtime, its
opener) is given to `prism-base` at launch (`rt::init`, `platform::set_opener`), and the blocks to
the core (`blocks::register`). `cargo test --workspace` runs every crate's tests; `-p <crate>`
one's.

### Browser preview

`just web` serves the frontend alone at http://127.0.0.1:5173, reading
`web/public/dev-snapshot.json` every 5 s (git-ignored). Two ways to fill it:

- the demo's fictitious data: `PRISM_DEMO_SNAPSHOT=$PWD/web/public/dev-snapshot.json cargo test --manifest-path src-tauri/Cargo.toml dump_for_preview -- --ignored`;
- your own data, from a running app: `PRISM_DUMP_SNAPSHOT=$PWD/web/public/dev-snapshot.json just dev`. It then holds real data: keep it to yourself.

What goes through the app (actions, agents, time tracking) is absent from the preview.

## Bundles

`just build` produces, in `src-tauri/target/release/bundle/`:

| System | Bundle | Notes |
|---|---|---|
| macOS | `macos/Prism.app` | unsigned: right-click › Open at first launch |
| Linux | `deb/*.deb`, `appimage/*.AppImage` | the `.deb` pulls in `tmux` |
| Windows | `nsis/*-setup.exe` | per-user installer, unsigned (SmartScreen: More info › Run anyway); installs WebView2 if missing |

Each system builds its own bundle, no cross-compilation, except Linux through Docker and Windows
through cargo-xwin:

- **`just linux`** (`scripts/linux.sh`) builds `dist/Prism_<version>_amd64.deb` and `.AppImage`,
  with their `.sha256`, in an Ubuntu 22.04 container: they run on any distribution as recent (the
  AppImage needs `libfuse2`). `--arch arm64` for ARM (native on Apple silicon), `--bundles
  deb,appimage,rpm` adds an `.rpm`, `pacman` an Arch Linux package (`.pkg.tar.zst`, `amd64`: the
  `.deb` repackaged by `makepkg` in an `archlinux` container; `sudo pacman -U` pulls in `tmux` and
  WebKitGTK). Later builds reuse the Docker volume `prism-linux-<arch>`.
- **`just dmg`** (`scripts/dmg.sh`) builds `dist/Prism_<version>_<commit>_universal.dmg` and its
  `.sha256`: checks (frontend types, Rust tests), a universal release build (Apple silicon and
  Intel), then a disk image with `Prism.app` and a link to `/Applications`. `--native` builds for
  this machine's architecture only.
- **`just windows-build`** (`scripts/windows.sh build`) builds the NSIS installer with cargo-xwin
  (Microsoft's CRT and SDK, `XWIN_ACCEPT_LICENSE=1`), x64 by default, `WINDOWS_ARCH=arm64` for
  ARM; `just windows-check` checks that the workspace compiles for Windows. Its pictures are
  `src-tauri/icons/nsis-sidebar.bmp` (164×314, the welcome and finish pages) and
  `nsis-header.bmp` (150×57, the other pages), 24-bit BMPs made from `icon.png` with ImageMagick:
  `magick -size 164x314 gradient:'#2c2452'-'#16132a' \( icon.png -resize 124x124 \) -gravity north
  -geometry +0+70 -composite -font HelveticaNeue.ttc -pointsize 26 -fill '#e9e4ff' -annotate +0+200
  Prism -alpha off -type truecolor BMP3:nsis-sidebar.bmp`, and `magick -size 150x57 xc:white
  \( icon.png -resize 52x52 \) -gravity east -geometry +4+0 -composite -alpha off -type truecolor
  BMP3:nsis-header.bmp`.

`just dmg` and `just linux` refuse uncommitted changes to tracked files, so the app names its commit (`--allow-dirty`
anyway), and run the checks first (`--skip-checks` not). Nothing personal goes into a package:
config, packs and tokens stay in `~/.config/prism` and the keychain of each machine.

### Without a window (a server)

`prism-server` is Prism without its window, for a machine reached from a phone only (a Linux
server, typically): the same collectors, agents, chains and remote, no WebKit, the phone's page
carried in the binary. Prism's releases carry it (`prism-server_<version>_amd64.tar.gz`, `_macos-universal.tar.gz`,
`_windows-x64.zip`, `_windows-arm64.zip`, each with its `.sha256` and `.minisig`), or build it on the server itself, the page first, then the binary:

```bash
just install && just server   # scripts/server.sh: pnpm --dir web build, then cargo build -p prism-server
install src-tauri/target/release/prism-server ~/.local/bin/
```

A team's blocks are modules, loaded at run time as in the app. A release carries the server with
`--server`.
The Linux one is built on Ubuntu 22.04: glibc 2.35 or newer, and `libdbus-1-3`. The config is the same file
(`~/.config/prism/config.toml`, profiles, packs), written by hand: no wizard. The remote must be
on, behind Tailscale (`tailscale serve` needs `sudo tailscale set --operator=$USER` once):

```toml
[remote]
enabled = true
tunnel = "tailscale"
```

Run it as a user service, which lingers without a session (`loginctl enable-linger`), with the
tokens in its environment if no keyring is unlocked there (`CLICKUP_TOKEN`…):

```ini
# ~/.config/systemd/user/prism.service
[Unit]
Description=Prism
After=network-online.target

[Service]
ExecStart=%h/.local/bin/prism-server serve
EnvironmentFile=-%h/.config/prism/env
Restart=on-failure

[Install]
WantedBy=default.target
```

`systemctl --user enable --now prism`, then `prism-server pair`: the QR code in the terminal, the
code the phone shows confirmed there. `prism-server restart` (or Restart from the phone) restarts
it in place; its log is the journal's and `~/.local/state/app.prism.desktop/`. A chain's *Notify
me* step reaches the phone as a push (`[remote] push`), as an agent waiting does.

What only a window or a desktop has is not there: profiles' switcher, clipboard, Audio, Now Playing,
captures, the computer's power and `keep_running`. Nor what the window runs on its timers: note and
meeting reminders, the digest, alerts' notifications, the checks for updates of Prism and its
packs. Prism's updater does not replace `prism-server` either: build it again.

### Another computer's Prism in a window (`[[machines]]`)

The Prism of a desk opens a server's (or another computer's) in a window of its own, the same
page as its own: agents and their terminals, worktrees, settings, everything the window does,
answered by that Prism. Its remote is on (above). On the desk, Settings › Remote › Machines (or
by hand):

```toml
[[machines]]
name = "build-srv"
url = "https://build-srv.tail1234.ts.net"   # or http://127.0.0.1:9000 with `ssh -L 9000:127.0.0.1:<its port> build-srv`
ssh = "build-srv"                           # optional: its folders open here in VS Code (Remote-SSH)
```

The profile menu lists it under *Other computers*. The first time, its window asks for a code:

```bash
prism-server peer pair    # shows a code for 5 minutes, then asks to accept the desk by its name
```

(or Settings › Remote › Devices › *Pair a computer* on a machine with a window). Five wrong codes
end the pairing; the token goes to the desk once, kept in its keychain, never in the page.
`peer add <name>` prints a token to paste instead (or to set as `PRISM_MACHINE_TOKEN_<NAME>` where
no keychain is unlocked); `peer list`, `peer remove <name>` (its windows close within ten seconds).
The window then shows that Prism with a blue band and rail edge. Open or not, the status bar
has a chip per machine (its agents, those waiting for you; faded out of reach, a click opens its
window), and an agent waiting there notifies here as one of this computer's would (a permission,
a question; quiet hours and the digest apply), counted in the menu bar (the tray on Linux) too.

What that machine classes critical (a merge, a manual job, an agent stopped, a worktree removed,
a file of an agent saved or a secret of its folder downloaded, a command of one of its own blocks)
first asks the person at the desk: Touch ID or the account's password on a Mac,
polkit's agent of the desktop on Linux (`pkcheck`, the password or a fingerprint where PAM takes
one; refused without polkit), Windows Hello on Windows (a face, a finger or the PIN; refused when
it is not set up). Asked by the app, never by the page; `skip_confirm = true` in the
machine's `[[machines]]` goes without.

A machine with several profiles (each its own `prism-server --profile <id> serve`, the others
with `[remote] tunnel = "relay"` behind the one with the tunnel, as for a phone) lists them under
its name in the profile menu: each opens in a window of its own, reached through the hub, which
checks the token; each keeps its own `[remote.peers]` rights. Its chip and notifications count
every profile's agents.

That confirmation guards against someone at the desk, not against a token in other hands: a
token holder can confirm by itself, so what holds is the machine's own `[remote.peers]`. `actions`
includes launching an agent, which runs commands of its own (so `typing = false` with `actions`
on does not stop code from running there). Pairing needs the tailnet's user when the tunnel is
Tailscale (a peer with its token may come through `ssh -L` without it).

A token is worth that machine's account: keep it as an SSH key. What a desk may do there is
`[remote.peers]` on that machine (Settings › Remote › Machines): `actions`, `critical`, `typing`,
`settings`, all on by default; it always reads, and each call that changes something is logged
with the desk's name. What opens on a screen (Finder or the file
manager, a terminal, an app) stays on the desk or is not offered; the editor opens the machine's folders over
SSH when `ssh` is set. An agent's files downloaded (Files tab) are prepared there (a zip for
several) and saved here, where the desk's dialog says, fetched once over the machine's remote.

### Signing (macOS)

Signed ad hoc, the app is refused at its first launch on another Mac: System Settings › Privacy &
Security › Open Anyway lets it through, once. With a Developer ID certificate in the keychain,
`PRISM_SIGN_IDENTITY="Developer ID Application: …"` signs it with the hardened runtime, and
`PRISM_NOTARY_PROFILE` (a `xcrun notarytool store-credentials` profile) has it notarized and
stapled: it then opens like any other app.

## Version

The version is the one of `src-tauri/Cargo.toml` (Tauri reads it). The boot screen, Settings and
About show it with the built commit: `0.1.0-a6 · a08aded`, a `+` after the hash when the tree had
uncommitted changes.

## Releases and updates

`just release` (`scripts/release.sh --to <source>`) publishes this Prism where installed Prisms
update from: a release tagged `prism-v<version>` (bump `Cargo.toml` first: an existing release is
refused) holding `Prism_<version>_macos-universal.dmg` (a first install), `.app.tar.gz` (what an
update replaces), with `--linux` the `amd64` `.deb`, AppImage and Arch package, and with
`--windows` the installers `Prism_<version>_windows-x64-setup.exe` and `windows-arm64-setup.exe`
(built from the Mac with `scripts/windows.sh`), each with its `.sha256` and its `.minisig`.

- Signed with minisign (`brew install minisign`): the key `PRISM_SIGN_KEY` names (environment or
  `.env`; `~/.minisign/prism.key` by default), its password asked once per file. Its public half
  is `src-tauri/update-key.pub`, built into the app: an installed Prism installs only a file
  signed with it, under its own name, whatever `[updates] source` says (a pack can set it). Made
  once with `minisign -G -p src-tauri/update-key.pub -s ~/.minisign/prism.key`; keep the secret
  key and its password safe: without them no update installs anymore. A fork that publishes
  its own releases commits its own `update-key.pub`.

- `--to glab:group/project`: the project's generic packages, linked from a GitLab release;
  `--to gh:owner/repo`: a GitHub release. `PRISM_RELEASE_TO` (environment or `.env`) is the default;
  none given, the script stops. `just release` and `just release-all` give `gh:Srynetix/prism`
  (a `--to` after them wins).
- `--dry-run` builds without publishing. A build of the same commit already in `dist/`
  is published as is; `--rebuild` builds again.
- The notes are the changelog (`scripts/changelog.sh`): the Conventional commits since the newest
  `prism-v*` tag, `feat` under Features, `fix` under Fixes, `perf` under Performance, `!` or
  `BREAKING CHANGE` under "Read first". `--notes` or `--notes-file` replace them. The script tags
  the commit it publishes, in this clone (`--no-tag`: not).

Installed Prisms follow `[updates] source` (the same `glab:`/`gh:` form, read with glab or gh):
they check at launch and every 6 hours, and Settings › General › Updates installs the new
version. On Windows, the installer of the computer's architecture is downloaded and checked, then
runs as Prism restarts, without a question, and starts the new Prism; the agents go on in
`prism sessions`, which runs from a copy of `prism.exe` the installer leaves alone. One build for everyone: a team's
blocks come as modules, with its pack, whichever Prism release it runs.

## Modules

A **module** brings a team's blocks to Prism **at run time**, without rebuilding it: a folder of
`modules/` next to `config.toml` (each profile its own), named after the module's id. Prism loads
its web part when the window starts, and on the phone; a module added, replaced or removed shows
once the window reloads (Settings › Packs › Modules › Reload the window).

```text
modules/hello/
  module.toml        [module] id, version, contract = 1, requires; [web] dir = "web"
  web/defs.js        its blocks' declarations and words, read first
  web/components.js  their components (and the chunks they import)
```

- `defs.js` exports `prism` (the module contract it was built for), `svelte` (the version it was
  compiled with: Prism refuses another minor one, Svelte's runtime not being a public API),
  `words` (its French, under Prism's own) and `blocks()` (its `BlockDef`s). It may import
  `@prism/sdk/words` only: it is read before anything of Prism reads the blocks.
- `components.js` exports `components`, by block id (`<id>:status` for a status-bar item). They
  reach Prism through `@prism/sdk` (`web/src/modules/sdk.ts`: words, the snapshot, layout,
  Prism's actions, formats, components) and Svelte, both Prism's own instances at run time.
  Of Svelte, Prism gives `svelte`, `svelte/animate`, `attachments`, `easing`, `events`,
  `motion`, `reactivity` (and `reactivity/window`), `store` and `transition`: the build refuses
  another one (`web/src/modules/host.ts`).
- A module's block runs inside a boundary: what it throws stays in its place, with Retry, and in
  the Problems list. A module that does not load (its manifest, its contract, its Svelte, a newer
  Prism needed, its files) is said in Problems and Settings › Packs, as is a block whose id is
  already taken (the core's, Prism's, or a module's before it).
- The window reads the files through `prism-module://localhost/<id>/…` (`http://prism-module.localhost/…`
  on Windows), the phone through `/modules/<id>/…` behind its session: a module's web folder
  only. A profile's window loads that profile's modules; another computer's window, none.

### Packs and trust

A pack brings its modules: `[[modules]]` with an `id` and a `source` (a folder of a repository,
`glab:group/project:path@ref` or `gh:owner/repo:path@ref`; a `.zip`; or a local folder or archive,
relative to a local pack). Importing or updating the pack installs each in `modules/<id>/`, whole,
marked with its pack (`.pack`); removing the pack, or a module from it, removes the folder.
`pack check` (the packs' update alert) reads their sources again too: the last commit of their
folder, their version.

Nothing of a module runs before I trust it, by the fingerprint of its folder (every file but
`.pack`), kept in the profile's data folder (`modules-trust.json`), never next to the modules.
Until then its files are not served (window, phone), its back end does not start (its blocks say
they wait), and Settings › Packs › Modules shows what it may reach; **Trust** (two clicks,
`modules_trust`, never from a phone) starts its back end at once. A new version waits again, the
lines it changes shown (a capability, a block, a config table).

### A module's back end

A module may bring its blocks' collector too: a **WebAssembly component** (`[wasm]`), which Prism
runs in a sandbox (wasmtime) as one of its own blocks: configured with the config, started with
its repositories, then read part by part when due, called from the window, with its demo data.

```toml
[wasm]
component = "backend.wasm"
tables = ["hello"]                     # the only config tables it is given
capabilities = { git = true }          # git, gitlab, http = ["host", …], exec = ["program", …]

[[wasm.blocks]]
id = "hello"
parts = ["hello_reads"]                # its sources read apart: their errors, their pace
repos = "one"                          # none | one | many, as a block's [<id>] repo / repos
reloads = true                         # a change of its tables configures it anew
provides = ["stages", "images"]        # for [clusters] stages / [vulnerabilities] images
remote = { ping = "read" }             # its commands from a phone: read | act | critical
```

- It implements the `prism:module` world (`src-tauri/crates/module/wit/prism-module.wit`):
  `configure` (its tables, the language, the demo: its parts, its stages, its demo states),
  `start` (what Prism resolved for a block: its repositories and their clones), `tick` (a part
  read: the states it changed, with their alerts and changes, its error, when to read it again),
  `call`, `hints`, `images`. Any language that builds a component will do; Rust:
  `cargo build --release --target wasm32-wasip2` (`examples/modules/hello/backend`).
- It reaches nothing but what Prism gives it, each a **capability** it declares: GitLab's API
  through glab (`gitlab`), git in the user's clones (its blocks' repositories and the other `[[repos]]`), read subcommands only (`git`), a GET to the
  hosts it names (`http`), the programs it names (`exec`); WASI gives it clocks and randomness, no
  file, no network, no environment. A capability it did not declare fails, said in its error.
- Each call is started apart (`begin-*`, then `finished`, `take`; `wait` when it has nothing
  else to do): its futures run side by side, eight calls at most at a time, and the part that
  started one is ticked again when it finishes. The blocking calls (`gitlab-api`, `git`…) stay
  for components built before.
- Its standard error goes to Prism's log (`host::log` too). A panic, or a call computing longer
  than two minutes of its own (ten in all), stops it: its blocks say so, and Prism starts it again 30 s later, three times at most
  in five minutes.
- Compiled once, it is kept in Prism's cache folder: the next launch loads it at once. On macOS
  it is interpreted (Pulley): the app's hardened runtime runs no code it did not sign.
- A back end that stopped Prism (a crash in it) leaves `modules/.stopped/<id>`: the same
  component is refused at the next launch, until it is replaced or the mark deleted.

A block that reads a repository declares it (`repos` in its `[[wasm.blocks]]`: `"one"`, or
`"many"`): its table's `repo` (or `repos`) names it among the `[[repos]]`, otherwise the one bare
MR references name (`[forge] bare_mr_repo`); `ctx.env()` gives them (`repos`, the first one's
`project`, `clone` and `gitlab`), and Settings › Code › Repositories lists the block under *Blocks
that read a repository*.

### The module SDK

A module is written with the **module SDK**, from Prism's repository, nothing else of it:

- `sdk/web` (`@prism/sdk`): the web part's build (`@prism/sdk/vite`, `moduleConfig(id, root)`),
  the types of what `@prism/sdk` and `@prism/sdk/words` give, and their names (`exports.json`).
  What Prism does not give fails at the build: another Svelte module, Prism's sources, a file out
  of the module's folder.
- `src-tauri/crates/module` (`prism-module`): the back end's crate (`sdk`), for `wasm32-wasip2`;
  its blocks' loops' `ctx.pause` hand control back to Prism, which resumes them when due.
- The SDK copies what Prism gives: `scripts/module-sdk-types.py` (the core's data types, into
  the crate) and `web/scripts/module-sdk.mjs` (the names, the snapshot's types, into the package),
  each with `--check` in `just check`. `just module-sdk-check` builds the example with the SDK
  alone, in a folder of its own.

Help › **Write a module** is the author's guide.

`examples/modules/hello` is one, with a back end (`just module-example` builds it into its
`dist/hello/`, after `rustup target add wasm32-wasip2`; copy that folder into `modules/`). The browser preview reads `PRISM_MODULES_DIR`:
`PRISM_MODULES_DIR=examples/modules/hello/dist just web`.

## Command line

`prism` and `prism-server` share their subcommands (one table, `prism_commands::cli::args`):
`--help` on each, `help <command>`, `--version` (the version and the build, as About says it),
`--profile <id>` before or after the subcommand. `prism` alone opens the window (or brings it to
the front); an unknown argument is an error (exit 2) rather than a window. A failure exits with 1.

On the PATH: Linux packages install `prism` in `/usr/bin`; on macOS, **Settings › General ›
Command line › Install** writes a short script in `~/.local/bin` (or `/usr/local/bin`) that runs
the app's binary. `prism completions <zsh|bash|fish>` prints the shell's completions.

Tokens go to the keychain with `prism token clickup|jira|google`: typed hidden on a terminal,
read from standard input otherwise (`pbpaste | prism token jira`). `clickup-token`, `jira-token`
and `google-secret` still work. `hook` and `watch` are Prism's own (an agent's hooks, the
keep-running job): not in the help.

<!-- cli:start (generated: PRISM_WRITE_GUIDE=1 cargo test -p prism-commands guide) -->
| Command | What it does | `prism` | `prism-server` |
|---|---|:-:|:-:|
| `pair` | Pair a phone from this terminal: its QR code here, the code it shows confirmed here | ✓ | ✓ |
| `peer pair` | Accept a computer by the code it is given here | ✓ | ✓ |
| `peer add <name>` | Add a computer by name: its token printed, once | ✓ | ✓ |
| `peer remove <name>` | Revoke a computer: its windows close within ten seconds | ✓ | ✓ |
| `peer list` | The computers added, with their date | ✓ | ✓ |
| `restart` | Restart the profile's Prism, rereading config.toml and its packs (started if not running) | ✓ | ✓ |
| `pack import <source>` | Add a pack: a file, a repository's file at a ref, or a starter | ✓ | ✓ |
| `pack update <name> [ref]` | Update a repository's pack, from its ref or the one given | ✓ | ✓ |
| `pack remove <name>` | Remove a pack | ✓ | ✓ |
| `modules list` | The modules in modules/: their version, where they come from, trusted or not | ✓ | ✓ |
| `modules install <source>` | Install a module without a pack: a folder or .zip, or a repository's folder at a ref | ✓ | ✓ |
| `modules reinstall <id>` | Read a module again from its pack or its source, and put it back whole | ✓ | ✓ |
| `modules remove <id>` | Remove a module (a pack's stays out of that pack's updates until put back) | ✓ | ✓ |
| `modules restore <id>` | Put back a pack's module you removed | ✓ | ✓ |
| `modules reset <id>` | Let a module that stopped Prism run again (its back end restarts with Prism, or from Settings) | ✓ | ✓ |
| `mcp` | Prism's MCP server for an agent CLI (on stdin/stdout), or its installation into one | ✓ | ✓ |
| `mcp install <claude\|codex\|opencode>` | Add Prism's MCP server and skill to an agent CLI's user config | ✓ | ✓ |
| `mcp uninstall <claude\|codex\|opencode>` | Take them out | ✓ | ✓ |
| `mcp status` | Where they are installed | ✓ | ✓ |
| `token <clickup\|jira\|google>` | Store a token in the keychain: typed hidden on a terminal, read from stdin otherwise | ✓ | ✓ |
| `serve` | Run Prism without a window, until stopped (a systemd service) |  | ✓ |
| `completions <bash\|elvish\|fish\|powershell\|zsh>` | Print the completion script of a shell | ✓ | ✓ |
<!-- cli:end -->

## Runtime files

| What | Where |
|---|---|
| Config | `~/.config/prism/config.toml`, or `PRISM_CONFIG`; another profile in `profiles/<id>/` |
| Profile | `prism --profile <id>`, or `PRISM_PROFILE` |
| Logs | `~/Library/Logs/app.prism.desktop/prism.log` (macOS), `~/.local/state/app.prism.desktop/` (Linux), the local data folder (Windows); rotated at 5 MB. `[logs] level`, or `RUST_LOG` |

Settings › General › Logs reveals, opens or copies the last lines of the log, for a bug report;
Settings › General › Dependencies lists the tools Prism runs and their versions (`prism_core::deps`),
which a feedback request attaches too.
