---
name: prism
description: Understand and configure Prism, the developer dashboard (blocks, pages, settings, packs, custom blocks). Use when asked how Prism shows something, to change its config.toml, to add or fix a block, or to explain a Prism setting or problem. Works with the `prism` MCP server (`prism mcp`).
---

# Prism

Prism is a desktop dashboard for a developer's day: merge requests, pipelines, tickets, agents,
worktrees, clusters, agenda. Everything it shows comes from **blocks** placed on **pages**, and
everything it does is set in one file, `config.toml` (`~/.config/prism/config.toml`, or the
profile's), mirrored field by field in its Settings view.

## Vocabulary

- **Block**: one thing on a page (MRs, Pipelines, Agents, Docker…), with its own config table
  (`[docker]`). A block absent from the state is off.
- **Page** (`[[views]]`): a rail entry (Dashboard, CI…), its blocks in columns; a `---` item starts
  a band (a full-width row).
- **Pack**: a team's config file layered under `config.toml` (`packs = ["team"]`). `config.toml`
  wins over its packs. Never edit a pack: override its keys in `config.toml`.
- **Profile**: a separate Prism (its own config, data, agents), `--profile <id>`.
- **Repository** (`[[repos]]`): a clone Prism knows; `forge = "github"` puts one on the other forge.
- **Worktree**, **agent**, **kind** (`[[agents.kinds]]`), **chain** (an agent's "Then" steps),
  **quick action** (`[[agents.quick_actions]]`), **routine** (`[[agents.routines]]`), **pod**
  (`[[pods]]`).
- **Custom block** (`[[custom_blocks]]`): a command or an inline script that prints JSON, made a
  block without Rust or Svelte (`custom:<id>` in `[[views]]`).

## Tools (MCP server `prism`)

Read freely (nothing secret, nothing of the user's work):

- `prism_search_docs(query)`: help pages, the commented reference (`config.example.toml`), the schema.
- `prism_explain_key(path)`: a key's type, default, current value, where it comes from (pack or
  config.toml), the block that reads it and its Settings page. Use before changing a key.
- `prism_problems()`: Prism's current warnings and errors (a key ignored, a block that fails).
- `prism_layout()`: the pages and their blocks.
- Resources: `prism://config/current`, `prism://config/merged` (with the packs),
  `prism://config/schema`, `prism://config/example`, `prism://blocks`, `prism://packs`,
  `prism://help/<page>`, `prism://guide/modules`.

Change only through proposals:

- `prism_validate(toml)`: check a fragment or a whole file without writing it. Always first.
- `prism_propose_config(toml, why)`: the user sees a diff in Prism and applies it (or refuses,
  with a reason, or edits it). The call waits for that answer. Never write `config.toml` yourself.
- `prism_propose_layout(views, why)`: the same for `[[views]]`.
- `prism_run_custom_block(spec)`: run a custom block once in a sandbox (no home, no token, a
  temporary folder, outbound network allowed, 20 s): its JSON and whether it follows its `kind`.
- `prism_create_custom_block(spec, why)`: propose it; the user trusts it in Prism when adding it.

With Prism stopped, only the docs, the reference and the schema answer: say so, and give the TOML
to paste instead.

## How to work

1. Find the setting: `prism_search_docs`, then `prism_explain_key` on the key.
2. Write the smallest TOML fragment that does it; `prism_validate` it; fix what it says.
3. `prism_propose_config` with a one-sentence reason in the user's words.
4. Once applied, check `prism_problems()` and `prism://blocks`: the change applies live.

Conventions: one setting per feature (no presets); no secret in the config (tokens live in the
system keychain: `prism clickup-token` and the like); a key a pack sets is overridden in
`config.toml`, said so in the reason.

## A new block: custom or module?

A **custom block** is enough when a command can print the state as JSON (`{"kind": "list",
"items": [...]}`, or `stats`, `table`, `markdown`; `alerts` and `status` beside it), read every so often, with optional actions on its
lines: a certificate expiry, a queue length, a service's health. Write the script, iterate with
`prism_run_custom_block` until its JSON follows the `kind`, then `prism_create_custom_block`.
A block that needs the home folder or a token fails in the sandbox: say so, it runs once trusted.

A **module** (a team's blocks loaded at run time: a Rust back end built to WebAssembly, Svelte
components) is needed for state kept between reads, a continuous collector, commands of its own, or
its own component. Read `prism://guide/modules`, the SDKs (`prism://guide/sdk` for the back end,
`prism://guide/web-sdk` for the window side) and the example module
(`prism://guide/example-block/…`); Help › Write a module is the author's guide.
