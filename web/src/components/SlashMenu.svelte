<script lang="ts">
  import { agentSlashCommands, type SlashCommand } from '../agents.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The slash commands of an agent's CLI offered while its box holds `/` and a word, as its CLI
   * tells them for its folder (Claude Code: its own, the project's, the user's, skills, plugins;
   * Codex: the user's prompts), asked once per agent, again after a failure. ↑↓
   * pick, Tab or Enter put `/name ` in the box, Esc puts the list away; a click as well. The box
   * hands its keys to `keydown` first, which says whether it took them.
   */
  let { id, text = $bindable(), field }: { id: string; text: string; field?: HTMLTextAreaElement } = $props()

  let commands = $state<SlashCommand[] | null>(null)
  let loadedFor = ''
  let sel = $state(0)
  /** Put away by Esc, for this word. */
  let dismissed = $state<string | null>(null)
  let list = $state<HTMLElement>()

  const query = $derived(/^\/(\S*)$/.exec(text)?.[1] ?? null)
  $effect(() => {
    if (query === null || loadedFor === id) return
    loadedFor = id
    commands = null
    agentSlashCommands(id)
      .then((c) => {
        if (loadedFor === id) commands = c
      })
      .catch(() => {
        commands = []
        loadedFor = ''
      })
  })
  const matches = $derived.by(() => {
    if (query === null || !commands) return []
    const q = query.toLowerCase()
    const rank = (c: SlashCommand) => {
      const n = c.name.toLowerCase()
      // A plugin's command by its own name too (`wip` finds `acme-flow:wip`).
      const own = n.slice(n.indexOf(':') + 1)
      return n.startsWith(q) || own.startsWith(q) ? 0 : n.includes(q) ? 1 : c.description.toLowerCase().includes(q) ? 2 : 3
    }
    return commands
      .map((c) => [rank(c), c] as const)
      .filter(([r]) => r < 3)
      .sort(([a, x], [b, y]) => a - b || x.name.localeCompare(y.name))
      .map(([, c]) => c)
  })
  const open = $derived(matches.length > 0 && dismissed !== text && !(matches.length === 1 && matches[0].name === query))
  $effect(() => {
    void query
    sel = 0
  })
  $effect(() => {
    list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' })
  })

  function pick(c: SlashCommand) {
    text = `/${c.name} `
    field?.focus()
  }

  export function keydown(e: KeyboardEvent): boolean {
    if (!open || e.isComposing) return false
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const n = matches.length
      sel = (sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
    } else if ((e.key === 'Tab' && !e.shiftKey) || (e.key === 'Enter' && !e.shiftKey)) {
      pick(matches[sel])
    } else if (e.key === 'Escape') {
      dismissed = text
    } else {
      return false
    }
    e.preventDefault()
    e.stopPropagation()
    return true
  }
</script>

{#if query !== null && commands === null && dismissed !== text}
  <div class="slash wait">{t('Reading its commands…')}</div>
{:else if open}
  <div class="slash" role="listbox" aria-label={t('Slash commands')} bind:this={list}>
    {#each matches as c, i (c.name)}
      <button type="button" role="option" aria-selected={i === sel} class:on={i === sel} onmousedown={(e) => e.preventDefault()} onclick={() => pick(c)} onmouseenter={() => (sel = i)}>
        <span class="name">/{c.name}{#if c.hint}<span class="hint">{' '}{c.hint}</span>{/if}</span>
        <span class="desc">{c.description}</span>
        {#if c.source !== 'built-in'}<span class="src">{c.source}</span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .slash { position: absolute; left: 0; right: 0; bottom: 100%; z-index: 20; max-height: 280px; overflow-y: auto; margin-bottom: 4px; padding: 4px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); box-shadow: 0 6px 20px rgb(0 0 0 / 0.18); }
  .wait { padding: 7px 12px; color: var(--text-muted); font-size: var(--fs-meta, 12px); }
  button { display: grid; grid-template-columns: minmax(0, max-content) minmax(0, 1fr) auto; align-items: baseline; gap: 10px; width: 100%; padding: 5px 8px; border: none; border-radius: 6px; background: none; color: var(--text); font: inherit; font-size: 13px; text-align: left; cursor: pointer; }
  button.on { background: var(--surface-muted); }
  .name { font-family: var(--font-mono, monospace); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .hint { color: var(--text-muted); }
  .desc { color: var(--text-muted); font-size: var(--fs-meta, 12px); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .src { color: var(--info); font-size: var(--fs-tag, 11px); white-space: nowrap; }
</style>
