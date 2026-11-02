<script lang="ts">
  import { t as tr, tc, tn } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { conventions } from '../conventions'
  import { cliOf } from '../agent-cli'
  import { hours } from '../retro'
  import FoldTitle from './FoldTitle.svelte'
  import { actualOf, agentsByTicket, mineMs, pct, shortModel, tokensShort, tone } from '../actuals'
  import { useAgentCosts } from '../agent-costs.svelte'

  /**
   * The ticket panel's Actual section (`[tracker] actuals`, see actuals.ts): its estimate against its
   * actual — the time logged, by me and by the others when the tracker sums everyone's, and its agents'
   * active time — then each agent with its kind, CLI, model, time and cost, closed ones too. Shown as
   * soon as the ticket has time logged or an agent.
   */
  let { id, estimate, spentMs, phone = false }: { id: string; estimate: number | null; spentMs: number | null | undefined; phone?: boolean } = $props()

  const s = $derived(live.snapshot)
  const costs = useAgentCosts()
  const agents = $derived(s ? (agentsByTicket(costs.list, s.merge_requests).get(id) ?? []) : [])
  const loggedOnly = $derived(conventions().actuals_logged_only)
  const a = $derived(actualOf({ estimate, spentMs, mineMs: mineMs(s?.time.entries ?? [], id), agents, loggedOnly }))
  const shown = $derived(conventions().actuals && (a.logged > 0 || a.agents.length > 0))
  /** The longer of the two bars fills the row; the estimate's mark sits on the actual's. */
  const scale = $derived(Math.max(a.estimate ?? 0, a.actual, 1))
  const w = (min: number) => `${Math.min(100, (100 * min) / scale)}%`
  const money = (n: number) => `$${n.toFixed(2)}`
  /** Folded until you open it (kept for every ticket). */
  let open = $state(false)
  const runs = $derived([...a.agents].sort((x, y) => y.started.localeCompare(x.started)))
</script>

{#if shown}
  <section class="actual" class:phone>
    <FoldTitle label={tr('Actual vs estimate')} key="ticket.actualOpen" bind:open hint={a.gap === null ? hours(a.actual) : `${hours(a.actual)} · ${pct(a.gap)}`} />
    {#if open}
    <div class="bars">
      <span class="l">{tc('noun', 'Estimate')}</span>
      <span class="bar">{#if a.estimate}<i class="est" style:width={w(a.estimate)}></i>{/if}</span>
      <span class="r">{a.estimate ? hours(a.estimate) : '—'}</span>
      <span class="l">{tr('Actual')}</span>
      <span class="bar" title={loggedOnly ? tr('Time logged only (Settings › Tracker)') : tr('Time logged, and the agents’ active time')}>
        <i class="real {tone(a.gap)}" style:width={w(a.actual)}></i>
        {#if !loggedOnly && a.agentMinutes}<i class="agents" style:left={w(a.logged)} style:width={w(a.agentMinutes)}></i>{/if}
        {#if a.estimate}<b class="mark" style:left={w(a.estimate)}></b>{/if}
      </span>
      <span class="r">{hours(a.actual)}</span>
    </div>
    <p class="verdict {tone(a.gap)}">
      {#if a.gap === null}{tr('Not estimated: nothing to compare the actual with.')}
      {:else if tone(a.gap) === 'ok'}{tr('Within {gap} of the estimate.', { gap: pct(a.gap).replace(/^[+−]/, '') })}
      {:else if a.gap > 0}{tr('{gap} over the estimate.', { gap: pct(a.gap) })}
      {:else}{tr('{gap} under the estimate.', { gap: pct(a.gap) })}{/if}
    </p>
    <div class="split">
      <div class="card">
        <small>{tr('Time logged')}</small>
        <b>{a.logged ? hours(a.logged) : '—'}</b>
        <small>{#if a.others !== null}{tr('me {mine} · others {others}', { mine: hours(a.mine), others: hours(a.others) })}{:else if a.logged}{tr('by me')}{:else}{tr('none yet')}{/if}</small>
      </div>
      <div class="card">
        <small>{tr('Agents')}</small>
        <b>{a.agentCost !== null ? money(a.agentCost) : a.agents.length ? '—' : '0'}</b>
        <small>{#if a.agents.length}{tn(a.agents.length, '{n} session', '{n} sessions')} · {hours(a.agentMinutes)} · {tr('{n} tokens', { n: tokensShort(a.tokens) })}{:else}{tr('none linked')}{/if}</small>
      </div>
    </div>
    {#if runs.length}
      <ul class="runs">
        {#each runs as r (r.id)}
          <li title={r.title}>
            <span class="k">{r.kind ?? tr('Agent')}</span>
            <span class="m">{cliOf(r.provider).name}{#if r.model}{' '}· {shortModel(r.model)}{/if}</span>
            <span class="v">{hours(Math.round(r.active_ms / 60_000))}{#if r.cost !== null}{' '}· {money(r.cost)}{/if}</span>
          </li>
        {/each}
      </ul>
    {/if}
    {/if}
  </section>
{/if}

<style>
  section { padding-top: 12px; margin-top: 12px; border-top: 1px solid var(--border); }
  .bars { display: grid; grid-template-columns: max-content minmax(0, 1fr) max-content; gap: 6px 10px; align-items: center; font-size: var(--fs-meta); }
  .l { color: var(--text-muted); }
  .r { font-family: var(--mono); text-align: right; font-variant-numeric: tabular-nums; }
  .bar { position: relative; height: 10px; border-radius: 5px; background: var(--surface-muted); }
  .bar i { position: absolute; top: 0; bottom: 0; left: 0; border-radius: 5px; }
  .bar .est { background: color-mix(in srgb, var(--info) 55%, var(--surface-muted)); }
  .bar .real { background: var(--ok); }
  .bar .real.warn { background: var(--warn); }
  .bar .real.ko { background: var(--ko); }
  /* The agents' share of the actual, hatched over its end. */
  .bar .agents { border-radius: 0 5px 5px 0; background: repeating-linear-gradient(135deg, transparent 0 3px, color-mix(in srgb, var(--surface) 55%, transparent) 3px 5px); }
  .bar .mark { position: absolute; top: -3px; bottom: -3px; width: 2px; margin-left: -1px; background: var(--info); }
  .verdict { margin: 10px 0 0; padding: 6px 9px; border-radius: var(--r7); font-size: var(--fs-meta); background: color-mix(in srgb, var(--c, var(--text-muted)) 10%, var(--surface)); border: 1px solid color-mix(in srgb, var(--c, var(--text-muted)) 35%, var(--border)); }
  .ok { --c: var(--ok); } .warn { --c: var(--warn); } .ko { --c: var(--ko); }
  .split { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-top: 10px; }
  .card { padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: color-mix(in srgb, var(--surface-muted) 50%, transparent); min-width: 0; }
  .card small { display: block; color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .card b { display: block; font-size: 16px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .runs { list-style: none; margin: 8px 0 0; padding: 0; font-size: var(--fs-meta); }
  .runs li { display: grid; grid-template-columns: minmax(70px, max-content) minmax(0, 1fr) max-content; gap: 8px; padding: 4px 0; border-top: 1px solid color-mix(in srgb, var(--border) 60%, transparent); }
  .runs .k { color: var(--text-muted); }
  .runs .m { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .runs .v { color: var(--text-muted); font-family: var(--mono); font-variant-numeric: tabular-nums; }
  .phone .bars, .phone .runs { font-size: 14px; }
  .phone .runs li { padding: 6px 0; }
</style>
