# Example module: hello

A module is a team's blocks that Prism loads at run time, without being rebuilt: a folder of
`modules/` next to `config.toml` (each profile its own). This one has one block, **Hello**, its
status-bar item, and a **Fail** button that throws on purpose: the block says so, with Retry, and
the rest of the window goes on. Its back end (`backend/`, a WebAssembly component) reads its
repository's last commit with git, answers **Ping**, and panics on **Crash the back end**: the
block says so, and Prism starts the component again 30 s later.

Build it, then install it:

```sh
rustup target add wasm32-wasip2          # once: the back end's target
just module-example                      # → examples/modules/hello/dist/hello/
cp -R examples/modules/hello/dist/hello ~/.config/prism/modules/
```

Reload Prism's window (Settings › Packs › Modules › Reload the window), then add **Hello** to a
page (+ Block › Fun).

It is built with the module SDK alone (Help › Write a module):

- `package.json`, `vite.config.mts`: the web part, with `@prism/sdk` (`sdk/web`; outside Prism's
  repository, a git dependency on it), Svelte at Prism's minor and Vite;
- `backend/`: the back end, with `prism-module` (`src-tauri/crates/module`). Its block,
  `backend/src/hello.rs`, is written as a block of Prism (`impl Block` against
  `crate::blocks::sdk`): the same file compiles inside Prism, a test of Prism checks it.

`just module-sdk-check` builds it with the SDK alone, in a folder of its own. The browser preview reads modules too: `PRISM_MODULES_DIR=examples/modules/hello/dist just web`.

What a module is made of:

- `module.toml`: `[module]` id (its folder's name), version, `contract` (1), `requires` (the oldest
  Prism it works with); `[web] dir`.
- `web/defs.js`: `prism` (the contract), `svelte` (the version it was compiled with: Prism refuses
  another minor one), `words` (its French), `blocks()` (its blocks' declarations, as Prism's
  `BlockDef`). It may import `@prism/sdk/words` only.
- `backend.wasm` (`[wasm]`): its back end, built from `backend/` (the `prism:module` world of
  `src-tauri/crates/module/wit/prism-module.wit`); `tables`, `capabilities` and `[[wasm.blocks]]`
  in `module.toml` say what it is given and may reach.
- `web/components.js`: `components`, by block id (`<id>:status` for a status-bar item). They
  import Prism through `@prism/sdk` (`web/src/modules/sdk.ts`) and Svelte, both Prism's own at
  run time.
