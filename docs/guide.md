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
| `prism-blocks` | Prism's own blocks, and the SDK a team's blocks use |
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
| Windows | `nsis/*-setup.exe` | per-user installer; installs WebView2 if missing |

Each system builds its own bundle, no cross-compilation, except Linux through Docker:

- **`just linux`** (`scripts/linux.sh`) builds `dist/Prism_<version>_amd64.deb` and `.AppImage`,
  with their `.sha256`, in an Ubuntu 22.04 container: they run on any distribution as recent (the
  AppImage needs `libfuse2`). `--arch arm64` for ARM (native on Apple silicon), `--bundles
  deb,appimage,rpm` adds an `.rpm`, `pacman` an Arch Linux package (`.pkg.tar.zst`, `amd64`: the
  `.deb` repackaged by `makepkg` in an `archlinux` container; `sudo pacman -U` pulls in `tmux` and
  WebKitGTK). Later builds reuse the Docker volume `prism-linux-<arch>`.
- **`just dmg`** (`scripts/dmg.sh`) builds `dist/Prism_<version>_<commit>_universal.dmg` and its
  `.sha256`: checks (frontend types, Rust tests), a universal release build (Apple silicon and
  Intel), then a disk image with `Prism.app` and a link to `/Applications`. `--native` builds for
  this Mac only.

Both refuse uncommitted changes to tracked files, so the app names its commit (`--allow-dirty`
anyway), and run the checks first (`--skip-checks` not). Nothing personal goes into a package:
config, packs and tokens stay in `~/.config/prism` and the keychain of each machine.

### Without a window (a server)

`prism-server` is Prism without its window, for a machine reached from a phone only (a Linux
server, typically): the same collectors, agents, chains and remote, no WebKit, the phone's page
carried in the binary. Build the page first, then the binary, on the server itself:

```bash
pnpm --dir web install && pnpm --dir web build
cargo build --release -p prism-server --manifest-path src-tauri/Cargo.toml
install src-tauri/target/release/prism-server ~/.local/bin/
```

A team's blocks (`PRISM_EXTRA_BLOCKS`) are built in as in the app. The config is the same file
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

What only a window or a Mac has is not there: profiles' switcher, clipboard, Audio, Now Playing,
captures, the Mac's power and `keep_running`. Nor what the window runs on its timers: note and
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
a question; quiet hours and the digest apply), counted in the menu bar too.

What that machine classes critical (a merge, a manual job, an agent stopped, a worktree removed,
a file of an agent saved or a secret of its folder downloaded, a command of one of its own blocks)
first asks the person at the desk: Touch ID or the account's password on a Mac,
polkit's agent of the desktop on Linux (`pkcheck`, the password or a fingerprint where PAM takes
one; refused without polkit). Asked by the app, never by the page; `skip_confirm = true` in the
machine's `[[machines]]` goes without.

A machine with several profiles (each its own `prism-server --profile <id> serve`, the others
with `[remote] tunnel = "relay"` behind the one with the tunnel, as for a phone) lists them under
its name in the profile menu: each opens in a window of its own, reached through the hub, which
checks the token; each keeps its own `[remote.peers]` rights. Its chip and notifications count
every profile's agents.

A token is worth that machine's account: keep it as an SSH key. What a desk may do there is
`[remote.peers]` on that machine (Settings › Remote › Machines): `actions`, `critical`, `typing`,
`settings`, all on by default; it always reads, and each call that changes something is logged
with the desk's name. What opens on a screen (Finder,
Terminal, an app) stays on the desk or is not offered; the editor opens the machine's folders over
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
update replaces) and, with `--linux`, the `amd64` `.deb`, AppImage and Arch package, each with its `.sha256`
and its `.minisig`.

- Signed with minisign (`brew install minisign`): the key `PRISM_SIGN_KEY` names (environment or
  `.env`; `~/.minisign/prism.key` by default), its password asked once per file. Its public half
  is `src-tauri/update-key.pub`, built into the app: an installed Prism installs only a file
  signed with it, under its own name, whatever `[updates] source` says (a pack can set it). Made
  once with `minisign -G -p src-tauri/update-key.pub -s ~/.minisign/prism.key`; keep the secret
  key and its password safe: without them no update installs anymore. A fork that publishes
  its own releases commits its own `update-key.pub`.

- `--to glab:group/project`: the project's generic packages, linked from a GitLab release;
  `--to gh:owner/repo`: a GitHub release. `PRISM_RELEASE_TO` (environment or `.env`) is the default.
- `--dry-run` builds without publishing. A build of the same commit and blocks already in `dist/`
  is published as is; `--rebuild` builds again.
- The notes are the changelog (`scripts/changelog.sh`): the Conventional commits since the newest
  `prism-v*` tag, `feat` under Features, `fix` under Fixes, `perf` under Performance, `!` or
  `BREAKING CHANGE` under "Read first". `--notes` or `--notes-file` replace them. The script tags
  the commit it publishes, in this clone (`--no-tag`: not).

Installed Prisms follow `[updates] source` (the same `glab:`/`gh:` form, read with glab or gh):
they check at launch and every 6 hours, and Settings › General › Updates installs the new
version. A team's build (with its blocks) goes to the team's private repository, Prism alone
(`--no-extra`) to the public one: one channel each.

## Blocks of another repository

A team can keep its own blocks in a repository of its own, compiled into Prism when
`PRISM_EXTRA_BLOCKS` names its folder (absolute, or relative to Prism's), in the environment or
in a git-ignored `.env` at Prism's root, read by `task`:

```bash
PRISM_EXTRA_BLOCKS=../team-blocks just build
```

`NO_EXTRA=1` leaves the `.env`'s blocks out of one command, to build Prism alone without touching
the `.env` (`NO_EXTRA=1 just build`, `NO_EXTRA=1 just dmg`…; `--no-extra` for the scripts). A
build with them says so after its version: `0.1.0 · 29f0690 + team-blocks 5786d86`, their name
(`PRISM_EXTRA_NAME`, otherwise their folder's) and the commit they were built from; packages
carry them too (`Prism_0.1.0_2cac323_team-blocks-b2717e7_universal.dmg`).

Such a repository holds:

- `src-tauri/mod.rs`: becomes the module `blocks::extra` of the app's crate and lists its blocks
  (`BLOCKS`, implementations of `blocks::Block`); its Rust reaches Prism through `blocks::sdk`
  only, and uses the app's crates only;
- `web/index.ts`: their declarations for the window (`EXTRA_BLOCKS`), importing nothing of Prism
  at run time (types aside);
- `web/components.ts`: their components (`EXTRA_COMPONENTS`), which reach Prism's code through
  `$prism/…`.

`web/src/blocks/extra-none.ts` shows the shape. A block brings everything of its own (config
table, collector, state, commands, Settings page, help, alerts, status item, demo data): the core
names none of them. `just check` also type-checks their window side against Prism's.

A block that reads a repository declares it (`Block::repos`: `RepoUse::One`, or `Many`): its
table's `repo` (or `repos`) names it among the `[[repos]]`, otherwise the one bare MR references
name (`[forge] bare_mr_repo`); `ctx.env()` gives them (`repos`, the first one's `project`, `clone`
and `gitlab`), and Settings › Code › Repositories lists the block under *Blocks that read a
repository*. A block that declares none gets the bare references' repository in `ctx.env()`.

## Runtime files

| What | Where |
|---|---|
| Config | `~/.config/prism/config.toml`, or `PRISM_CONFIG`; another profile in `profiles/<id>/` |
| Profile | `prism --profile <id>`, or `PRISM_PROFILE` |
| Logs | `~/Library/Logs/app.prism.desktop/prism.log` (macOS), `~/.local/state/app.prism.desktop/` (Linux), the local data folder (Windows); rotated at 5 MB. `[logs] level`, or `RUST_LOG` |

Settings › General › Logs reveals, opens or copies the last lines of the log, for a bug report;
Settings › General › Dependencies lists the tools Prism runs and their versions (`prism_core::deps`),
which a feedback request attaches too.
