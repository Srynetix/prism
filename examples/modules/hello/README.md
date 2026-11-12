# Example module: hello

A module is a team's blocks that Prism loads at run time, without being rebuilt: a folder of
`modules/` next to `config.toml` (each profile its own). This one has one block, **Hello**, its
status-bar item, and a **Fail** button that throws on purpose: the block says so, with Retry, and
the rest of the window goes on.

Build it, then install it:

```sh
just module-example                      # → examples/modules/hello/dist/hello/
cp -R examples/modules/hello/dist/hello ~/.config/prism/modules/
```

Reload Prism's window (Settings › Packs › Modules › Reload the window), then add **Hello** to a
page (+ Block › Fun). The browser preview reads modules too: `PRISM_MODULES_DIR=examples/modules/hello/dist just web`.

What a module is made of:

- `module.toml`: `[module]` id (its folder's name), version, `contract` (1), `requires` (the oldest
  Prism it works with); `[web] dir`.
- `web/defs.js`: `prism` (the contract), `svelte` (the version it was compiled with: Prism refuses
  another minor one), `words` (its French), `blocks()` (its blocks' declarations, as Prism's
  `BlockDef`). It may import `@prism/sdk/words` only.
- `web/components.js`: `components`, by block id (`<id>:status` for a status-bar item). They
  import Prism through `@prism/sdk` (`web/src/modules/sdk.ts`) and Svelte, both Prism's own at
  run time.
