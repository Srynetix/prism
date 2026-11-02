<script lang="ts" module>
  /** An image of a prompt: its name, and how to get it (a `data:` URL). */
  export interface PromptImage {
    key: string
    name: string
    load: () => Promise<string>
  }
</script>

<script lang="ts">
  import { t } from '../../i18n.svelte'
  /**
   * The images of a prompt, in its bubble: attached by Prism or pasted in its CLI, each a
   * thumbnail; a click (a tap) shows it whole over the conversation, a click or Esc closes it. One
   * no longer there (or not from where attachments go) reads as its name.
   */
  let { images }: { images: PromptImage[] } = $props()

  let shown = $state<string | null>(null)
</script>

<svelte:window onkeydown={(e) => shown && e.key === 'Escape' && (shown = null)} />

<div class="imgs">
  {#each images as image (image.key)}
    {#await image.load()}
      <span class="ph" title={image.name}></span>
    {:then url}
      <button class="thumb" onclick={() => (shown = url)} title={t('{name} · show it whole', { name: image.name })}><img src={url} alt={image.name} /></button>
    {:catch e}
      <span class="gone" title={String(e)}>{image.name}</span>
    {/await}
  {/each}
</div>
{#if shown}
  <button class="whole" onclick={() => (shown = null)} aria-label={t('Close the image')}><img src={shown} alt="" /></button>
{/if}

<style>
  .imgs { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; margin: 2px 0 6px; }
  .thumb { display: block; padding: 0; border: none; border-radius: 10px; background: none; cursor: zoom-in; overflow: hidden; }
  .thumb img { display: block; max-width: min(260px, 100%); max-height: 180px; object-fit: contain; }
  .ph { width: 120px; height: 80px; border-radius: 10px; background: color-mix(in srgb, var(--on-accent) 15%, transparent); }
  .gone { padding: 2px 8px; border-radius: 8px; background: color-mix(in srgb, var(--on-accent) 15%, transparent); font: var(--fs-tag) var(--mono); opacity: 0.8; }
  /* Over the whole window, above the drawers. */
  .whole { position: fixed; inset: 0; z-index: 95; display: grid; place-items: center; padding: 24px; border: none; background: rgba(10, 10, 10, 0.78); cursor: zoom-out; }
  .whole img { max-width: 100%; max-height: 100%; object-fit: contain; border-radius: 6px; box-shadow: var(--shadow-pop); }
</style>
