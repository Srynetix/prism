<script lang="ts">
  import { t } from '../i18n.svelte'
  import BlockControls from './BlockControls.svelte'
  import Logo from './Logo.svelte'
  import JourneyStrip from './JourneyStrip.svelte'
  import JourneyAction from './JourneyAction.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { filterJourneys, journeys, where, type Journey, type JourneyFilter, type Role } from '../journey'
  import { journeyAction } from '../journey-actions'
  import { canPreview, openTicket } from '../ticket-view.svelte'
  import { openMr } from '../mr-view.svelte'
  import { ticketKey, trackerLogo } from '../tracker'
  import { ago } from '../time'
  import { clock } from '../clock.svelte'
  import { mrWord } from '../forge'
  import type { Snapshot } from '../types'

  /**
   * In progress: my active tickets, one row each — those of the sprint assigned to me and not done,
   * those with an open MR or a running agent of mine — and the MRs asking for my review. Each with
   * its seven steps (journey.ts), why it stands there, my part in it and the one action that moves
   * it on. Someone waiting on me first, then what failed, then the normal flow, then what has not
   * started. Filtered on my move, on others', or all (`wip.filter`, shared with the phone).
   */
  let { snap }: { snap: Snapshot } = $props()

  let filter = $state<JourneyFilter>(pref<JourneyFilter>('wip.filter', 'all'))
  const setFilter = (f: JourneyFilter) => setPref('wip.filter', (filter = f))
  // Read again each minute too: "since" moves on.
  const all = $derived((void clock(60_000).now, journeys(snap)))
  const rows = $derived(filterJourneys(all, filter))
  const mine = $derived(all.filter((j) => j.move === 'me').length)
  const ROLE: Record<Role, string> = { author: t('author'), reviewer: t('reviewer'), assignee: t('assignee') }
  const ROLE_HINT: Record<Role, string> = { author: t('Your {mr}', { mr: mrWord() }), reviewer: t('Your review is asked'), assignee: t('Assigned to you') }

  function open(j: Journey) {
    if (j.ticket && canPreview()) openTicket(ticketKey(j.ticket))
    else if (j.mr) openMr(j.mr)
  }
  const EMPTY: Record<JourneyFilter, string> = { me: t('Nothing waits on you.'), others: t('You wait on nobody.'), all: t('No ticket in progress.') }
</script>

{#snippet title(j: Journey)}
  <button class="id mono" onclick={() => open(j)} title={j.ticket ? t('Preview {id}', { id: j.id }) : t('Open {name}', { name: j.id })}>{j.id}</button>
{/snippet}

<section class="panel" style="--src: var(--src-clickup)" class:collapsed={isCollapsed('wip')}>
  <h2>
    <span class="title"><Logo name={trackerLogo()} />{t('In progress')}</span>
    {#if !isShort('wip')}
      <span class="seg">
        <button class:on={filter === 'me'} onclick={() => setFilter('me')} title={t('Tickets where the next move is yours')}>{t('My move')} {mine}</button>
        <button class:on={filter === 'others'} onclick={() => setFilter('others')} title={t('Tickets waiting on someone else: a review, the CI, the merge queue, an agent at work')}>{t('Waiting on others')} {all.length - mine}</button>
        <button class:on={filter === 'all'} onclick={() => setFilter('all')}>{t('All')} {all.length}</button>
      </span>
    {:else}
      <small>{t('{n} yours to move', { n: mine })}</small>
    {/if}
    <BlockControls id="wip" />
  </h2>
  {#if !snap.tracker_me && !snap.me}
    <div class="empty">{t('Prism does not know you in the tracker nor on the forge yet: your tickets show once it does.')}</div>
  {:else if isShort('wip')}
    <div class="short-body">
      {#each all.filter((j) => j.move === 'me') as j (j.key)}
        {@const action = journeyAction(j, snap)}
        <div class="srow">
          {@render title(j)}
          <span class="t" title="{j.title} — {j.why}">{j.why}</span>
          {#if action}<JourneyAction primary={action} />{/if}
        </div>
      {:else}
        <span class="more">{EMPTY.me}</span>
      {/each}
    </div>
  {:else}
    <div class="scroll">
      {#each rows as j (j.key)}
        {@const action = journeyAction(j, snap)}
        <div class="row" class:ko={j.failed} class:waits={j.urgency === 0 && j.move === 'me'}>
          <div class="main">
            <div class="head">
              {@render title(j)}
              <button class="name" onclick={() => open(j)} title={j.title}>{j.title}</button>
            </div>
            <div class="why">{j.why}{#if j.move === 'others' && j.since}<span class="since">{' '}· {t('for {ago}', { ago: ago(j.since) })}</span>{/if}</div>
          </div>
          <div class="steps">
            <JourneyStrip journey={j} mini />
            <small>{where(j)}</small>
          </div>
          <span class="who" class:me={j.move === 'me'} title={ROLE_HINT[j.role]}>{ROLE[j.role]}</span>
          <div class="act">{#if action}<JourneyAction primary={action} />{:else if j.move === 'others'}<span class="wait">{t('waiting')}</span>{/if}</div>
        </div>
      {:else}
        <div class="empty">{EMPTY[filter]}</div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { font: inherit; font-size: var(--fs-meta); padding: 2px 8px; border: none; background: none; color: var(--text-muted); cursor: pointer; white-space: nowrap; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: var(--surface-muted); color: var(--text); }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto 76px 110px; align-items: center; gap: 4px 12px; padding: 7px 12px; border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent); }
  .row.waits { box-shadow: inset 2px 0 var(--accent); }
  .row.ko { box-shadow: inset 2px 0 var(--ko); }
  .main { min-width: 0; }
  .head { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
  .mono { font-family: var(--mono); }
  .id { flex-shrink: 0; border: none; background: none; padding: 0; color: var(--info); font-size: var(--fs-meta); font-weight: 600; cursor: pointer; }
  .name { min-width: 0; overflow: hidden; border: none; background: none; padding: 0; color: var(--text); font: inherit; font-size: var(--fs-body); font-weight: 500; text-align: left; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  .id:hover, .name:hover { text-decoration: underline; }
  .why { overflow: hidden; color: var(--text-muted); font-size: var(--fs-meta); text-overflow: ellipsis; white-space: nowrap; }
  .row.ko .why { color: color-mix(in srgb, var(--ko) 75%, var(--text)); }
  .steps { display: flex; flex-direction: column; gap: 3px; }
  .steps small { color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .who { justify-self: start; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r8); color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .who.me { border-color: var(--accent); color: var(--text); }
  .act { display: flex; justify-content: flex-end; }
  .wait { color: var(--text-muted); font-size: var(--fs-tag); }
  .srow .id { font-size: var(--fs-meta); }
  /* The phone: the same list, the action the whole width under each row. */
  :global(html.remote) .row { grid-template-columns: minmax(0, 1fr) auto; padding: 10px 12px; }
  :global(html.remote) .row .act { grid-column: 1 / -1; }
  :global(html.remote) .row .act :global(.go) { width: 100%; padding: 9px 12px; border-radius: var(--r10); font-size: 15px; }
  :global(html.remote) .row .steps { grid-column: 1; grid-row: 2; }
  :global(html.remote) .row .who { grid-column: 2; grid-row: 1; align-self: start; }
  :global(html.remote) .name { white-space: normal; }
</style>
