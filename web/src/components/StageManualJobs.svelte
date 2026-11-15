<script lang="ts">
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import PlayJob from './PlayJob.svelte'
  import { anchored, dismissable, exclusive } from '../popup'
  import { untrack } from 'svelte'
  import { pushLayer } from '../layers.svelte'
  import type { ManualJob, PlayedJob } from '../types'

  /**
   * Manual jobs in a dropdown menu: those of a stage, or a block's tag jobs. `played`: those
   * already run, below, to run again (a tag set again after a new base image), under `playedHeading`.
   */
  let {
    stage, jobs, played = [], button: buttonText, heading, playedHeading, labelOf, risky = false,
  }: { stage: string; jobs: ManualJob[]; played?: PlayedJob[]; button?: string; heading?: string; playedHeading?: string; labelOf?: (j: ManualJob | PlayedJob) => string; risky?: boolean } = $props()

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()

  let menu: HTMLDivElement | undefined = $state()
  // Open, a layer on top: Escape closes it first.
  $effect(() => {
    if (!open) return
    return untrack(() => pushLayer({ kind: 'menu', key: 'manual-jobs', el: () => menu, close: () => (open = false), outside: false })).remove
  })

  function toggle() {
    open = !open
  }
</script>

<button bind:this={button} class="manual" class:open onclick={toggle} aria-expanded={open} title={t('Manual jobs of {stage}', { stage })}><Icon name="play" /> {buttonText ?? jobs.length + played.length}</button>
{#if open}
  <!-- Fixed position: the cell or title holding the button would clip what overflows. -->
  <div class="menu popover" role="menu" tabindex="-1" bind:this={menu} {@attach anchored(() => button, { align: 'center' })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true, escape: false })}>
    <span class="hd">{heading ?? t('{stage} · manual jobs', { stage })}</span>
    {#each jobs as j (j.id)}<PlayJob job={j} label={labelOf?.(j)} {risky} />{/each}
    {#if played.length}
      <span class="hd again">{playedHeading ?? t('Already run · run again')}</span>
      {#each played as j (j.id)}<PlayJob job={j} label={labelOf?.(j)} {risky} />{/each}
    {/if}
  </div>
{/if}

<style>
  .manual { margin-top: 1px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); line-height: 1.5; cursor: pointer; }
  .manual:hover, .manual.open { color: var(--accent); border-color: var(--accent); }
  .menu { position: fixed; display: flex; flex-direction: column; align-items: stretch; gap: 4px; overflow: auto; text-align: left; }
  .hd.again { margin-top: 4px; padding-top: 6px; border-top: 1px solid var(--border); }
  .menu :global(.play) { justify-content: flex-start; }
</style>
