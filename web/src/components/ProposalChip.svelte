<script lang="ts">
  import type { Agent } from '../types'
  import { live } from '../live.svelte'
  import { tn } from '../i18n.svelte'
  import { isDesktop } from '../bridge'
  import { openProposal, proposals } from '../mcp-view.svelte'
  import Icon from './Icon.svelte'

  /**
   * "⚙ 1 proposal" on an agent: its changes of Prism (`prism mcp`) waiting for your answer, as a
   * question waits. A click opens the oldest. A span, not a button: it sits inside clickable rows.
   */
  let { agent }: { agent: Agent } = $props()
  const mine = $derived(proposals(live.snapshot).filter((p) => p.agent === agent.id))
  function go(e: Event) {
    e.stopPropagation()
    e.preventDefault()
    if (isDesktop()) openProposal(mine[0]?.id)
  }
</script>

{#if mine.length}
  <span class="chip" role="button" tabindex="0" title={tn(mine.length, 'It proposes a change of Prism: see it, apply or refuse it', 'It proposes {n} changes of Prism: see them', {})} onclick={go} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && go(e)}>
    <Icon name="settings" size={11} />{tn(mine.length, '{n} proposal', '{n} proposals')}
  </span>
{/if}

<style>
  .chip { display: inline-flex; align-items: center; gap: 3px; padding: 0 7px; border-radius: var(--r9); background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); font-size: var(--fs-tag); font-weight: 700; line-height: 17px; cursor: pointer; white-space: nowrap; vertical-align: middle; }
  .chip:hover { filter: brightness(1.1); }
</style>
