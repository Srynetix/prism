<script lang="ts">
  import type { Agent } from '../types'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'
  import { openAgents } from '../agents.svelte'
  import { isDesktop } from '../bridge'
  import { openOfAgent, showAsides } from '../asides.svelte'
  import Icon from './Icon.svelte'

  /**
   * "↗ n" on an agent: the tasks it flagged in passing, still open. A click opens the « In passing »
   * list on its own. A span, not a button: it sits inside clickable rows.
   */
  let { agent }: { agent: Agent } = $props()
  const open = $derived(openOfAgent(live.snapshot, agent.id))
  function go(e: Event) {
    e.stopPropagation()
    e.preventDefault()
    // On a phone, its More › In passing (`RemoteApp` follows `asideView`).
    if (isDesktop()) openAgents()
    showAsides(agent.id)
  }
</script>

{#if open.length}
  <span class="chip" role="button" tabindex="0" title={`${tn(open.length, '{n} task flagged in passing', '{n} tasks flagged in passing')}:\n${open.map((a) => `· ${a.title}`).join('\n')}\n${t('Click: the « In passing » list')}`} onclick={go} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && go(e)}>
    <Icon name="aside" size={11} />{open.length}
  </span>
{/if}

<style>
  .chip { display: inline-flex; align-items: center; gap: 3px; padding: 0 7px; border-radius: var(--r9); background: color-mix(in srgb, var(--info) 14%, transparent); color: var(--info); font-size: var(--fs-tag); font-weight: 700; line-height: 17px; cursor: pointer; white-space: nowrap; vertical-align: middle; }
  .chip:hover { filter: brightness(1.1); }
</style>
