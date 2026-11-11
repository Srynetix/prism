<script lang="ts">
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'
  import { asideView, asidesOf, byRepo, FILTERS, inFilter, isStale } from '../asides.svelte'
  import AsideCard from './AsideCard.svelte'
  import Icon from './Icon.svelte'

  /**
   * « In passing »: what the agents flagged out of their task, one line per subject, by
   * repository. Open (the old ones folded under Older), Launched, Tickets, Dismissed; one agent's
   * only from its chip. The Agents view's tab, the `aside` block (`block`: without the filters'
   * agent), the phone's More › In passing.
   */
  let { block = false }: { block?: boolean } = $props()
  const all = $derived(asidesOf(live.snapshot))
  const agent = $derived(block ? '' : asideView.agent)
  // Its own: the ones it saw, and those sent to its session (a note of the user's among them).
  const mine = $derived(agent ? all.filter((a) => a.agent === agent || a.seen.some((s) => s.agent === agent)) : all)
  const shown = $derived(mine.filter((a) => inFilter(a, asideView.filter)))
  const fresh = $derived(shown.filter((a) => !isStale(a)))
  const older = $derived(shown.filter((a) => isStale(a)))
  let showOlder = $state(false)
  const agentTitle = $derived(live.snapshot?.agents?.find((a) => a.id === agent)?.title ?? agent)
</script>

<div class="asides">
  <div class="filters" role="tablist">
    {#each FILTERS as [f, label] (f)}
      {@const n = mine.filter((a) => inFilter(a, f)).length}
      <button role="tab" class:on={asideView.filter === f} aria-selected={asideView.filter === f} onclick={() => (asideView.filter = f)}>{label} <span>{n}</span></button>
    {/each}
  </div>
  {#if agent}
    <div class="only"><span>{t('Seen by {agent}', { agent: agentTitle })}</span><button onclick={() => (asideView.agent = '')} aria-label={t('All agents')} title={t('All agents')}><Icon name="close" size={11} /></button></div>
  {/if}
  {#each byRepo(fresh) as [repo, list] (repo)}
    <div class="grp">{repo || t('No repository')}<span>{list.length}</span></div>
    {#each list as a (a.id)}<AsideCard {a} />{/each}
  {:else}
    {#if !older.length}
      <p class="empty">{asideView.filter === 'open' ? t('Nothing flagged in passing. An agent Prism launches flags here what it sees out of its task (suggest_task), rather than in a sentence of its summary.') : t('None.')}</p>
    {/if}
  {/each}
  {#if older.length}
    <button class="grp fold" onclick={() => (showOlder = !showOlder)} aria-expanded={showOlder}><Icon name={showOlder ? 'chevronDown' : 'chevronRight'} size={11} />{t('Older')}<span>{tn(older.length, '{n} line', '{n} lines')}</span></button>
    {#if showOlder}{#each older as a (a.id)}<AsideCard {a} />{/each}{/if}
  {/if}
</div>

<style>
  .asides { display: flex; flex-direction: column; gap: 6px; padding: 6px 8px 10px; }
  .filters { display: flex; flex-wrap: wrap; gap: 2px; }
  .filters button { padding: 2px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .filters button.on { background: var(--surface-muted); color: var(--text); }
  .filters span { opacity: 0.7; }
  .only { display: flex; align-items: center; gap: 6px; padding: 2px 8px; border-radius: var(--r6); background: color-mix(in srgb, var(--info) 10%, transparent); color: var(--info); font-size: var(--fs-meta); }
  .only span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .only button { padding: 0; border: none; background: none; color: inherit; cursor: pointer; }
  .grp { display: flex; align-items: center; gap: 5px; margin-top: 4px; color: var(--text-muted); font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em; }
  .grp span { font-weight: 400; }
  .grp.fold { padding: 0; border: none; background: none; font: inherit; font-size: var(--fs-tag); cursor: pointer; text-align: left; }
  .empty { margin: 8px 4px; color: var(--text-muted); font-size: var(--fs-body); }
</style>
