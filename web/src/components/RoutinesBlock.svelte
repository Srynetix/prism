<script lang="ts">
  import { live } from '../live.svelte'
  import { isCollapsed } from '../layout.svelte'
  import { isDesktop } from '../bridge'
  import { openSettings } from '../settings.svelte'
  import { t } from '../i18n.svelte'
  import { reviewPile, routinesOf } from '../routines.svelte'
  import BlockControls from './BlockControls.svelte'
  import RoutineReview from './RoutineReview.svelte'
  import type { RoutineRun } from '../bindings/RoutineRun'

  /**
   * The Routines block: "To review" placed in a page, the same pile as at the head of the Agents
   * view, and the routines' next times.
   */
  let { onrerun }: { onrerun?: (run: RoutineRun) => void } = $props()
  const n = $derived(reviewPile(live.snapshot).length)
  const none = $derived(!routinesOf(live.snapshot).routines.length)
</script>

<section class="panel" style="--src: var(--src-claude)" class:collapsed={isCollapsed('routines')}>
  <h2>
    <span class="title">{t('Routines')}</span>
    {#if n}<small>{t('{n} to review', { n })}</small>{/if}
    <BlockControls id="routines" />
  </h2>
  <div class="scroll">
    {#if none}
      <div class="empty">{t('No routine yet: an agent launched on its own, at fixed times ([[agents.routines]]).')}{#if isDesktop()}{' '}<button class="lnk" onclick={() => openSettings('routines')}>{t('Settings › Routines')}</button>{/if}</div>
    {:else if !n && !routinesOf(live.snapshot).routines.some((r) => r.next.length)}
      <div class="empty">{t('Nothing to review.')}</div>
    {:else}
      <RoutineReview {onrerun} />
    {/if}
  </div>
</section>

<style>
  .scroll { overflow: auto; padding: 6px 8px; }
  .scroll :global(.review) { border: none; padding: 0; background: none; }
  .empty { padding: 10px 12px; color: var(--text-muted); font-size: var(--fs-body); }
  .lnk { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; text-decoration: underline; }
</style>
