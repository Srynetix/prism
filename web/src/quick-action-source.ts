import { escapeRe } from './markdown'

/**
 * A quick action made from a past session (Create a quick action…): what is pure in it. The words
 * a repository's name and its base branch stand for (`{repo}`, `{base}`), its name apart from the
 * others. The prompt its drafting agent is asked, and its reply read back: `prism_agents::draft`.
 * State and actions: `quick-action-draft.svelte.ts`.
 */

/** What a drafting agent's launch says it is (`from`): a copy of `prism_core::agents::DRAFTER` (src-tauri/crates/core/src/agents/mod.rs), never kept as its repository's last session. */
export const DRAFTER = 'quick-action-draft'

/**
 * `text` with the repository's name written `{repo}` and its base branch `{base}`. The name: whole
 * words, not a file's (`app.py`). The branch only where it is one (`origin/main`, `` `main` ``,
 * « the main branch », « branche main »): a word like `main` or `dev` elsewhere stays. What is
 * left, the drafting agent writes.
 */
export function templated(text: string, repo: string, base: string): string {
  let out = text
  if (repo) out = out.replace(new RegExp(`(?<![\\w{/.-])${escapeRe(repo)}(?![\\w}-]|\\.\\w)`, 'g'), '{repo}')
  if (base && base !== repo) {
    const b = escapeRe(base)
    out = out
      .replace(new RegExp(`(?<=origin/)${b}(?![\\w/-])`, 'g'), '{base}')
      .replace(new RegExp(`\`${b}\``, 'g'), '`{base}`')
      .replace(new RegExp(`(?<=\\b(?:branch|branche)\\s+)${b}(?![\\w/-])`, 'gi'), '{base}')
      .replace(new RegExp(`(?<![\\w/-])${b}(?=\\s+branch\\b)`, 'gi'), '{base}')
  }
  return out
}

/** `name`, or `name 2`, `name 3`… when it is taken. */
export function uniqueName(name: string, taken: string[]): string {
  const base = name.trim() || 'Quick action'
  if (!taken.includes(base)) return base
  let n = 2
  while (taken.includes(`${base} ${n}`)) n++
  return `${base} ${n}`
}
