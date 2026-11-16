<script lang="ts">
  import type { Agent } from '../types'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'
  import { openAgents } from '../agents.svelte'
  import { isDesktop } from '../bridge'
  import { openOfAgent, showAsides } from '../asides.svelte'
  import CountChip from './CountChip.svelte'

  /**
   * "↗ n" on an agent: the tasks it flagged in passing, still open. A click opens the « In passing »
   * list on its own. A span, not a button: it sits inside clickable rows.
   */
  let { agent }: { agent: Agent } = $props()
  const open = $derived(openOfAgent(live.snapshot, agent.id))
  function go() {
    // On a phone, its More › In passing (`RemoteApp` follows `asideView`).
    if (isDesktop()) openAgents()
    showAsides(agent.id)
  }
</script>

{#if open.length}
  <CountChip tone="info" icon="aside" title={`${tn(open.length, '{n} task flagged in passing', '{n} tasks flagged in passing')}:\n${open.map((a) => `· ${a.title}`).join('\n')}\n${t('Click: the « In passing » list')}`} {go}>{open.length}</CountChip>
{/if}
