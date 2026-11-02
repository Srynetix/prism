<script lang="ts">
  import { anchored, dismissable } from '../popup'
  import { arrangeAlerts, BAND_LABEL } from '../alert-bands'
  import { alertMinutes, isSnoozed } from '../alert-snooze.svelte'
  import { alertSettings } from '../conventions'
  import { isSeen } from '../seen.svelte'
  import type { Alert } from '../rules'
  import Alerts from './Alerts.svelte'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'

  /**
   * What waits for you, in the status bar: a dot and a count per band (Now, Today, When you can),
   * the alerts neither seen nor put off. A click opens the same list as the Alerts block, from any
   * page; nothing to say, the item is not there.
   */
  let { alerts, now }: { alerts: Alert[]; now: number } = $props()

  const bands = $derived(arrangeAlerts(alerts.filter((a) => !isSeen(a) && !isSnoozed(a, now)), alertSettings(), (a) => alertMinutes(a, now), false))
  const counts = $derived(bands.map((b) => ({ band: b.band, n: b.groups.length })))
  const title = $derived(counts.map((c) => `${BAND_LABEL[c.band]}: ${c.n}`).join('\n'))
  let button = $state<HTMLElement>()
  let open = $state(false)
</script>

{#if counts.length}
  <button bind:this={button} class="sbi waits" class:open onclick={(e) => { e.stopPropagation(); open = !open }} {title} aria-expanded={open} aria-label={t('What waits for you')}>
    <span class="g"><Icon name="warning" /></span>
    {#each counts as c (c.band)}<i class={c.band}><b>{c.n}</b></i>{/each}
  </button>
{/if}

{#if open && counts.length}
  <div class="pop" role="dialog" aria-label={t('What waits for you')} {@attach anchored(() => button, { align: 'start' })} {@attach dismissable(() => (open = false), { trigger: () => button })}>
    <p class="head">{t('What waits for you')}</p>
    <Alerts {alerts} popup />
  </div>
{/if}

<style>
  .waits { gap: 6px; }
  .waits.open { background: var(--surface-muted); color: var(--text); }
  i { display: inline-flex; align-items: center; gap: 3px; font-style: normal; }
  i::before { content: ''; width: 7px; height: 7px; border-radius: 50%; background: var(--info); }
  i.now::before { background: var(--ko); }
  i.today::before { background: var(--warn); }
  .pop { position: fixed; z-index: 45; width: min(520px, calc(100vw - 24px)); padding: 4px 0; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); color: var(--text); font-size: var(--fs-body); white-space: normal; }
  .head { margin: 4px 12px 2px; font-weight: 600; }
</style>
