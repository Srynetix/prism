<script lang="ts">
  import BlockHelpContent from './BlockHelpContent.svelte'
  import { anchored, dismissable, exclusive } from '../popup'
  import { BLOCK_HELP } from '../block-help'
  import { BLOCK_LABELS, type BlockId } from '../views'
  import { feedbackOn, openFeedback } from '../feedback.svelte'
  import { t } from '../i18n.svelte'

  /**
   * "?" in a block's title: its contextual help (what it is for, what it shows, where
   * its data comes from) in a fixed-position bubble, so it is not clipped by the block.
   * Escape, a click elsewhere or a second click closes it.
   */
  let { id }: { id: string } = $props()
  const help = $derived(BLOCK_HELP[id as BlockId])

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()

  function toggle() {
    open = !open
  }
  /** The bubble lives at page level: the title controls fade out when not hovered. */
  function portal(node: HTMLElement) {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }
</script>

{#if help}
  <button bind:this={button} class="help" class:on={open} onclick={toggle} aria-expanded={open} aria-label={t('About this block')} title={t('About this block')}>?</button>
  {#if open}
    <div use:portal {@attach anchored(() => button, { gap: 6 })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true })} class="bubble" role="dialog" aria-label={t('About {name}', { name: t(BLOCK_LABELS[id as BlockId]) })}>
      <BlockHelpContent id={id as BlockId} />
      {#if feedbackOn()}<button class="suggest" onclick={() => ((open = false), openFeedback({ where: `block ${BLOCK_LABELS[id as BlockId] ?? id} (${id})` }))}>{t('Suggest something for this block…')}</button>{/if}
    </div>
  {/if}
{/if}

<style>
  .help { display: inline-flex; align-items: center; justify-content: center; width: 20px; height: 20px; padding: 0; border: none; border-radius: 50%; background: none; color: var(--text-muted); font: inherit; font-size: 11px; font-weight: 600; line-height: 1; cursor: pointer; }
  .help:hover, .help.on { background: var(--surface-muted); color: var(--text); }
  .bubble { position: fixed; z-index: 60; width: min(360px, calc(100vw - 16px)); overflow: auto; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-pop); color: var(--text); font-size: var(--fs-body); font-weight: 400; line-height: 1.45; text-transform: none; letter-spacing: 0; white-space: normal; cursor: auto; }
  .suggest { display: block; margin-top: 8px; padding: 0; border: none; background: none; color: var(--accent); font: inherit; font-size: var(--fs-meta); font-weight: 600; cursor: pointer; }
</style>
