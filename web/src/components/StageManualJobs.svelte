<script lang="ts">
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import PlayJob from './PlayJob.svelte'
  import { anchored, dismissable, exclusive } from '../popup'
  import type { ManualJob } from '../types'

  /** Manual jobs in a dropdown menu: those of a stage, or a block's tag jobs. */
  let {
    stage, jobs, button: buttonText, heading, labelOf, risky = false,
  }: { stage: string; jobs: ManualJob[]; button?: string; heading?: string; labelOf?: (j: ManualJob) => string; risky?: boolean } = $props()

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()

  function toggle() {
    open = !open
  }
</script>

<button bind:this={button} class="manual" class:open onclick={toggle} aria-expanded={open} title={t('Manual jobs of {stage}', { stage })}><Icon name="play" /> {buttonText ?? jobs.length}</button>
{#if open}
  <!-- Fixed position: the cell or title holding the button would clip what overflows. -->
  <div class="menu" role="menu" tabindex="-1" {@attach anchored(() => button, { align: 'center' })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true })}>
    <span class="head">{heading ?? t('{stage} · manual jobs', { stage })}</span>
    {#each jobs as j (j.id)}<PlayJob job={j} label={labelOf?.(j)} {risky} />{/each}
  </div>
{/if}

<style>
  .manual { margin-top: 1px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); line-height: 1.5; cursor: pointer; }
  .manual:hover, .manual.open { color: var(--accent); border-color: var(--accent); }
  .menu { position: fixed; z-index: 40; display: flex; flex-direction: column; align-items: stretch; gap: 4px; min-width: 180px; overflow: auto; padding: 6px; text-align: left; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); }
  .head { padding: 0 2px 2px; font-size: var(--fs-tag); color: var(--text-muted); }
  .menu :global(.play) { justify-content: flex-start; }
</style>
