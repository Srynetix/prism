<script lang="ts">
  import { t } from '../i18n.svelte'
  import { keys } from '../platform'
  import { agentsAvailable } from '../agents.svelte'
  import { hasWorktrees } from '../repositories'
  import { startWorkstream } from '../project-actions'
  import Icon from './Icon.svelte'
  import type { Ticket } from '../types'

  /**
   * « New workstream » in a project: what to do, in words or as a ticket; Start makes its worktree
   * (where the repository allows them, its clone otherwise) and launches the agent; More options
   * opens the launcher, prefilled. On its own (`project-actions.ts startWorkstream` does the work):
   * the Projects page shows it, the assistant can too. `direct` false (the phone): Start opens the
   * launcher, prefilled, rather than launching.
   */
  let { repo, tickets = [], direct = true, onstarted }: { repo: string; tickets?: Ticket[]; direct?: boolean; onstarted?: (id: string) => void } = $props()

  let prompt = $state('')
  let ticket = $state('')
  let busy = $state(false)
  const chosen = $derived(tickets.find((x) => x.id === ticket))
  const ready = $derived(!!chosen || !!prompt.trim())
  const where = $derived(hasWorktrees(repo) ? t('a new worktree, its agent launched') : t('in its clone, its agent launched'))

  async function start(now: boolean) {
    if (busy || (now && !ready) || !agentsAvailable()) return
    busy = true
    try {
      const id = await startWorkstream(repo, chosen ? { ticket: chosen } : { prompt }, now && direct)
      if (id || !(now && direct)) {
        prompt = ''
        ticket = ''
      }
      if (id) onstarted?.(id)
    } finally {
      busy = false
    }
  }
  function onkey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      void start(true)
    }
  }
</script>

<form class="nw" onsubmit={(e) => { e.preventDefault(); void start(true) }}>
  <span class="lead"><Icon name="plus" stroke={2.5} />{t('New workstream')}</span>
  <textarea bind:value={prompt} rows="1" disabled={!!chosen} placeholder={chosen ? t('{ticket}: its Implement', { ticket: chosen.id }) : t('Describe the work, or pick a ticket…')} onkeydown={onkey} aria-label={t('What the workstream is about')}></textarea>
  {#if tickets.length}
    <select bind:value={ticket} aria-label={t('Start from a ticket')} title={t('Start from a ticket: its Implement, which takes it')}>
      <option value="">{t('Ticket…')}</option>
      {#each tickets as x (x.id)}<option value={x.id}>{x.id} · {x.name}</option>{/each}
    </select>
  {/if}
  <button type="submit" class="go" disabled={!ready || busy || !agentsAvailable()} title={keys(t('Start: {where} (↵)', { where }))}><Icon name="play" size={11} />{t('Start')}</button>
  {#if direct}<button type="button" class="more" onclick={() => start(false)} disabled={busy || !agentsAvailable()} title={t('The launcher, prefilled: CLI, model, place…')}>{t('More options…')}</button>{/if}
  <small class="hint">{where}</small>
</form>

<style>
  .nw { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 8px 10px; border: 1px dashed color-mix(in srgb, var(--accent) 55%, var(--border)); border-radius: var(--r8); background: color-mix(in srgb, var(--accent) 5%, var(--surface)); }
  .lead { display: inline-flex; align-items: center; gap: 5px; font-weight: 600; color: var(--accent); white-space: nowrap; }
  textarea { flex: 1 1 260px; min-width: 0; resize: none; padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; field-sizing: content; max-height: 8lh; }
  select { max-width: 260px; padding: 3px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); }
  button { display: inline-flex; align-items: center; gap: 4px; padding: 3px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  button:hover:not(:disabled) { background: var(--surface-muted); }
  button:disabled { opacity: 0.5; cursor: default; }
  .go { border-color: var(--accent); background: var(--accent); color: var(--on-accent); }
  .go:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 85%, black); }
  .hint { flex-basis: 100%; color: var(--text-muted); font-size: var(--fs-tag); }
</style>
