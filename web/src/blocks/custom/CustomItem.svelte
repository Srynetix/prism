<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { showBlock } from '../../layout.svelte'
  import { openSettings } from '../../settings.svelte'
  import type { StatusProps } from '../block'
  import { customState } from './types'
  import { t, tn } from '../../i18n.svelte'

  /**
   * The status bar's item: each custom block's `status` (its color its state; a click shows the
   * block), and the blocks waiting for trust.
   */
  let { snap }: StatusProps = $props()

  const blocks = $derived(customState(snap)?.blocks ?? [])
  const pending = $derived(blocks.filter((b) => b.pending))
  const shown = $derived(blocks.filter((b) => !b.pending && b.output?.status))
</script>

{#if pending.length}
  <button class="sbi pend" onclick={() => openSettings('custom-blocks')} title={pending.map((b) => b.label).join(', ')}>
    <span class="g"><Icon name="warning" /></span>{tn(pending.length, '{n} block to check', '{n} blocks to check')}
  </button>
{/if}
{#each shown as b (b.id)}
  {@const s = b.output!.status!}
  <button class="sbi cst {s.state ?? ''}" onclick={() => showBlock(`custom:${b.id}`)} title={t('{name} · open the block', { name: b.label })}><b>{b.label}</b> {s.text}</button>
{/each}

<style>
  .pend { color: var(--warn); }
  .cst b { font-weight: 500; color: var(--text-muted); margin-right: 3px; }
  .cst.ok { color: var(--ok); }
  .cst.warn { color: var(--warn); }
  .cst.ko { color: var(--ko); }
  .cst.info { color: var(--info); }
</style>
