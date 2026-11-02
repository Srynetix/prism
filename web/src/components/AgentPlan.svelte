<script lang="ts">
  import PlanCard from './chat/PlanCard.svelte'
  import { agentConversation, type ProposedPlan } from '../agents.svelte'
  import { isDesktop } from '../bridge'
  import { socketState } from '../remote/socket.svelte'
  import type { Agent } from '../types'
  import { pollsShown } from '../polls.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The Plan tab: the plans the agent proposed (plan mode), the last one wide to read, comment on
   * and answer (PlanCard, as in its Chat), the earlier versions folded under it. Read again every
   * few seconds from its transcript. From a phone, answered with `[remote] actions`, its words with
   * `[remote] typing`.
   */
  let { agent }: { agent: Agent } = $props()

  const running = $derived(agent.state === 'running')
  const acts = $derived(running && (isDesktop() || (socketState.hello?.classes.includes('act') ?? false)))
  const typing = $derived(running && (isDesktop() || (socketState.hello?.typing ?? false)))

  let plans = $state<ProposedPlan[] | null>(null)
  let error = $state<string | null>(null)
  async function load() {
    try {
      const c = await agentConversation(agent.id, 30)
      const next = (c?.turns ?? []).flatMap((turn) => turn.entries.flatMap((e) => (e.type === 'tool' && e.proposal ? [e.proposal] : [])))
      if (JSON.stringify(next) !== JSON.stringify(plans)) plans = next
      error = null
    } catch (e) {
      error = String(e)
    }
  }
  const shown = pollsShown()
  $effect(() => {
    void agent.id
    plans = null
  })
  $effect(() => {
    void agent.id
    // Hidden (another view, the window): read again once shown.
    if (!shown()) return
    void load()
    const timer = setInterval(load, 3000)
    return () => clearInterval(timer)
  })
  const latest = $derived(plans?.at(-1))
  const earlier = $derived(plans ? plans.slice(0, -1).reverse() : [])
  const before = (i: number) => (plans && i > 0 && plans[i - 1].text ? plans[i - 1].text : null)
</script>

<div class="plans">
  {#if error}
    <p class="muted err">{error}</p>
  {:else if plans === null}
    <p class="muted">…</p>
  {:else if !latest}
    <p class="muted">{t('No plan in its last turns. An agent in plan mode (launched in Plan mode, or Shift+Tab in its terminal) proposes a plan before it edits anything: it shows here, to comment on passage by passage and answer.')}</p>
  {:else}
    <PlanCard id={agent.id} plan={latest} previous={before(plans.length - 1)} latest {acts} {typing} wide />
    {#if earlier.length}
      <h4>{t('Earlier versions')}</h4>
      {#each earlier as p (p.call)}
        <PlanCard id={agent.id} plan={p} previous={before(plans.indexOf(p))} latest={false} acts={false} typing={false} />
      {/each}
    {/if}
  {/if}
</div>

<style>
  .plans { flex: 1; min-height: 0; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 10px; }
  .plans > :global(*) { flex-shrink: 0; }
  .muted { margin: 4px 2px; color: var(--text-muted); font-size: 12.5px; }
  .err { color: var(--ko); }
  h4 { margin: 6px 0 0; font-size: 12px; color: var(--text-muted); font-weight: 600; }
</style>
