---
title: Write a module
group: Make it yours
related: settings, fun
---
A **module** brings a team's blocks to Prism without rebuilding it: a folder Prism loads at run time, with a web part (the blocks the window shows) and, if it reads something, a back end (their collector, run by Prism in a sandbox). This guide writes one with the module SDK; `examples/modules/hello` in Prism's repository is a complete one.

## What a module is

A folder of `modules/` next to `config.toml` (each profile its own), named after the module's id:

```text
modules/hello/
  module.toml        what it is, what it may reach
  web/defs.js        its blocks' declarations and words
  web/components.js  their components
  backend.wasm       its back end (optional)
```

Prism lists them in **Settings › Packs › Modules**: loaded, or refused with the reason (another module contract, another Svelte, a newer Prism needed, a back end that stopped Prism). A module added or replaced shows once the window reloads (**Reload the window** there).

## The manifest

```toml
[module]
id = "hello"
version = "1.0.0"
contract = 1                  # the module contract it is built for
requires = "0.12"             # the oldest Prism it works with

[web]
dir = "web"

[wasm]
component = "backend.wasm"
tables = ["hello"]            # the only config tables it is given
capabilities = { git = true } # git, gitlab, http = ["host"], exec = ["program"]

[[wasm.blocks]]
id = "hello"
parts = ["hello_reads"]       # its sources read apart (one may be named as the block)
repos = "one"                 # none, one or many: [hello] repo / repos
reloads = true
provides = ["stages", "images"]
remote = { ping = "read" }    # its commands from a phone: read, act or critical
```

The back end reaches nothing it does not declare: GitLab through glab (`gitlab`), git in your clones, reading only (`git`: its blocks' repositories and the others of `[[repos]]`; `branch` and `tag` only to list), a GET to the hosts it names (`http`), the programs it names (`exec`). No file, no network, no environment besides.

## The web part

Its project depends on `@prism/sdk`, Svelte (Prism's minor: Prism refuses another) and Vite; its build is one line:

```ts
// vite.config.mts
import { moduleConfig } from '@prism/sdk/vite'
export default moduleConfig('hello', import.meta.dirname)
```

- `src/defs.ts` exports `prism` and `svelte` (given by the build: `__PRISM_CONTRACT__`, `__SVELTE__`), `words` (its French) and `blocks()`, its blocks' declarations: their help, their item in the status bar, a guide of Help (`guide`) and a page of Settings for their config tables (`settings`) if they have them. It imports `@prism/sdk/words` only.
- `src/components.ts` exports `components`, by block id (`hello:status` for its status-bar item). They import Prism through `@prism/sdk` only: words, the snapshot, a block's commands (`call`), layout, Prism's actions, formats, its icons and controls.

What Prism does not give fails at the build, not in Prism: another Svelte module (Prism gives `svelte` and its `animate`, `easing`, `motion`, `reactivity`, `store`, `transition`…), Prism's sources, a file out of the module's folder.

## The back end

A Rust crate built for WebAssembly, its blocks written as Prism's own: `impl Block`, against `prism-module`'s `sdk`.

```toml
# backend/Cargo.toml
[lib]
crate-type = ["cdylib"]

[dependencies]
prism-module = { git = "https://github.com/Srynetix/prism" }
```

```rust
// backend/src/lib.rs
pub mod blocks { pub use prism_module::sdk; }   // the block reads crate::blocks::sdk
mod hello;                                       // impl Block for Hello { … }

static BLOCKS: &[&dyn prism_module::Block] = &[&hello::HELLO];
prism_module::module!(BLOCKS);
```

Its collector is a loop spawned in `start`, as inside Prism: `loop { read; ctx.publish(&state); ctx.pause(every).await }`. Each `pause` hands control back to Prism, which resumes the loop when that part is due again. Prism runs git, GitLab and programs apart: what a block joins (`join_all`, `buffer_unordered`) goes side by side, eight calls at most at a time, as inside Prism; a module built with an earlier `prism-module` makes them one after the other. A panic, or a call computing for more than two minutes (what Prism does for it, git, GitLab, a program, apart; ten minutes in all), stops the back end: its blocks say so, and Prism starts it again 30 s later. Its `tracing` lines (`tracing::warn!`, `tracing` among its dependencies) go to Prism's log, as a block's inside Prism.

Build: `rustup target add wasm32-wasip2` once, then `cargo build --release --target wasm32-wasip2`. `vite build` copies the component into `dist/<id>/` with `module.toml`.

## Bring it with a pack

A team's pack installs its modules: each `[[modules]]` names one and where it is read, the same sources as the pack's own (the user's glab or gh login).

```toml
[[modules]]
id = "hello"
source = "glab:group/prism-pack:modules/hello@main"   # a folder of a repository
# "gh:owner/repo:dist/hello.zip@v2" (an archive), or a folder or .zip next to the pack
```

Importing or updating the pack puts each module in `modules/<id>/`, whole; removing the pack, or a module from it, removes the folder. The packs' update alert also says when one of its modules changed at its source. A module of the pack you remove in Settings stays out of its updates (`[modules] removed`) until you **Put back** it.

## Trust it

Nothing of a module runs before you trust it: its web part is not loaded, its back end does not start, its blocks say they wait. **Settings › Packs › Modules** lists what its back end may reach (GitLab, git, the hosts and programs it names): **Trust** it (two clicks) and its back end starts at once, its web part with the window's next load. Trust goes by the fingerprint of its folder: a new version waits again, with what it changes (a capability it adds, a block, a config table); so does one trusted when a capability reached less than Prism lets it now.

A new version a pack brings, of a module you trusted from that pack, is trusted for you when it reaches nothing more: the same capabilities and config tables, or fewer. Anything more (a host, a program, a config table, a back end), and it waits for your trust as above. **Trust a pack's updates** (`[modules] trust_pack_updates`, on by default) turns this off; a module installed by hand always asks.

## Install and try it

Without a pack: **Install a module** in Settings › Packs › Modules (or `prism modules install <source>`) from its folder or `.zip` (`dist/<id>`), or a repository's folder at a ref (`gh:owner/repo:dist/hello@v1`). Reload the window, trust it, restart Prism for its back end, then add its blocks to a page (**+ Block**). Its source is kept: **Reinstall** reads it again, and a change there shows in the status bar as a pack's would. Its back end's lines and errors are in Prism's log (`module <id>: …`). In the demo profile, its blocks show `Block::demo`'s state.

## When it crashes

A back end that panics, or computes past its deadline, says so on its blocks and starts again 30 seconds later. After three crashes in five minutes Prism gives up: Settings › Packs › Modules says when and why it last crashed. **Start again** (two clicks) starts it from nothing, its crashes forgotten. One that stopped Prism itself (Prism killed in the middle of a call) is held at the next launch, its blocks saying so, until you start it again. **Reinstall** reads it again from its pack or its source and puts it back whole; **Remove** deletes it and forgets its trust. `prism modules list | reinstall | remove | restore | reset` do the same from a terminal; a running Prism sees them once it restarts.
