<script lang="ts">
  import { anchored, dismissable } from '../popup'
  import { closeMrChoice, mrChoice, openMrOption } from '../mr-choice.svelte'
  import { mrKey } from '../mr-ref'
  import { repoName } from '../repositories'
  import type { MrOption } from '../linkify'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The repositories an agent's `!12` may name (it works in several): one line each, its title
   * when Prism knows the MR, otherwise « on its forge ».
   */
  const at = () => ({ getBoundingClientRect: () => new DOMRect(mrChoice.x, mrChoice.y, 0, 0) }) as HTMLElement
  function pick(o: MrOption) {
    closeMrChoice()
    openMrOption(o)
  }
</script>

{#each mrChoice.options ? [mrChoice.options] : [] as options (options)}
  <div class="mrchoice" role="menu" tabindex="-1" {@attach anchored(at, { align: 'start', gap: 2 })} {@attach dismissable(closeMrChoice, { resize: true })}>
    <div class="hd">{t('Which repository?')}</div>
    {#each options as o (mrKey(o.ref))}
      <button role="menuitem" onclick={() => pick(o)} title={o.url ?? mrKey(o.ref)}>
        <Icon name="mr" size={11} />
        <b>{repoName(o.ref.repo)}</b>
        <span class="k">{mrKey(o.ref)}</span>
        <span class="what">{o.title ?? t('on its forge')}</span>
        {#if o.url}<Icon name="external" size={10} />{/if}
      </button>
    {/each}
  </div>
{/each}

<style>
  .mrchoice { position: fixed; z-index: 200; min-width: 220px; max-width: 420px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); color: var(--text); font-size: var(--fs-body); outline: none; }
  .hd { padding: 2px 8px 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  button { display: flex; align-items: center; gap: 6px; width: 100%; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  button:hover { background: var(--surface-muted); }
  .k { color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-meta); }
  .what { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
</style>
