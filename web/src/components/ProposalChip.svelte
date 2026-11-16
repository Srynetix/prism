<script lang="ts">
  import type { Agent } from '../types'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'
  import { isDesktop } from '../bridge'
  import { openProposal, proposals } from '../mcp-view.svelte'
  import CountChip from './CountChip.svelte'

  /**
   * "⚙ 1 proposal" on an agent: its changes of Prism (`prism mcp`) waiting for your answer, as a
   * question waits. A click opens the oldest. A span, not a button: it sits inside clickable rows.
   */
  let { agent }: { agent: Agent } = $props()
  const mine = $derived(proposals(live.snapshot).filter((p) => p.agent === agent.id))
  function go() {
    openProposal(mine[0]?.id)
  }
</script>

{#if mine.length && isDesktop()}
  <CountChip tone="src" icon="settings" title={tn(mine.length, 'It proposes a change of Prism: see it, apply or refuse it', 'It proposes {n} changes of Prism: see them', {})} {go}>{tn(mine.length, '{n} proposal', '{n} proposals')}</CountChip>
{:else if mine.length}
  <!-- On the phone, said only: the answer is given on the computer. -->
  <CountChip tone="src" icon="settings" title={t('To answer on the computer')}>{tn(mine.length, '{n} proposal', '{n} proposals')}</CountChip>
{/if}
