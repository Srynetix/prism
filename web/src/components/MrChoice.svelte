<script lang="ts">
  import { anchored, dismissable } from '../popup'
  import { untrack } from 'svelte'
  import { pushLayer } from '../layers.svelte'
  import { closeMrChoice, mrChoice, openMrOption } from '../mr-choice.svelte'
  import { mrKey } from '../mr-ref'
  import { repoName } from '../repositories'
  import { knownMr, type MrOption } from '../linkify'
  import { live } from '../live.svelte'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The repositories an agent's `!12` may name (it works in several): one line each, those Prism
   * knows first with their title, the others « on its forge ».
   */
  const at = () => ({ getBoundingClientRect: () => new DOMRect(mrChoice.x, mrChoice.y, 0, 0) }) as HTMLElement
  let menu: HTMLDivElement | undefined = $state()
  // Open, a layer on top: Escape closes it first, not the panel the link is in.
  const shown = $derived(!!mrChoice.options)
  $effect(() => {
    if (!shown) return
    return untrack(() => pushLayer({ kind: 'menu', key: 'mr-choice', el: () => menu, close: closeMrChoice, outside: false })).remove
  })
  function pick(o: MrOption) {
    closeMrChoice()
    openMrOption(o)
  }
</script>

{#each mrChoice.options ? [mrChoice.options] : [] as options (options)}
  <div class="mrchoice popover" role="menu" tabindex="-1" bind:this={menu} {@attach anchored(at, { align: 'start', gap: 2 })} {@attach dismissable(closeMrChoice, { resize: true, escape: false })}>
    <div class="hd">{t('Which repository?')}</div>
    {#each options as o (mrKey(o.ref))}
      {@const known = knownMr(o, live.snapshot)}
      <button class="mi" role="menuitem" onclick={() => pick(o)} title={known ? mrKey(o.ref) : o.url}>
        <Icon name="mr" size={11} />
        <b>{repoName(o.ref.repo)}</b>
        <span class="ref">{mrKey(o.ref)}</span>
        <span class="what">{known?.title ?? t('on its forge')}</span>
        {#if !known}<Icon name="external" size={10} />{/if}
      </button>
    {/each}
  </div>
{/each}

<style>
  .mrchoice { position: fixed; min-width: 220px; max-width: 420px; outline: none; }
  .mrchoice .mi { gap: 6px; }
  .ref { color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-meta); }
  .what { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
</style>
