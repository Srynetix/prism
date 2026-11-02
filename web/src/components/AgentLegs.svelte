<script lang="ts">
  import Icon from './Icon.svelte'
  import { CLIS } from '../agent-cli'
  import { backTo } from '../agent-switch.svelte'
  import { dateFormat } from '../locale'
  import { t, tn } from '../i18n.svelte'
  import type { Agent, AgentProvider } from '../types'

  /**
   * The CLIs an agent ran (Continue with…), in its Activity tab: its cost split between them, each
   * stretch with its time, replies and cost, and Back to a CLI it left.
   */
  let { agent }: { agent: Agent } = $props()

  interface Row { provider: AgentProvider; model?: string | null; from: string; to: string | null; replies: number; cost: number | null; why: string | null }
  const rows = $derived<Row[]>([
    ...(agent.legs ?? []).map((l) => ({ provider: l.provider, model: l.model, from: l.started, to: l.ended, replies: l.usage?.replies ?? 0, cost: l.usage?.cost ?? null, why: l.reason === 'limit' ? t('stopped at its limit') : t('handed over') })),
    { provider: agent.provider, model: agent.model, from: (agent.legs ?? []).at(-1)?.ended ?? agent.created_at, to: null, replies: agent.usage?.replies ?? 0, cost: agent.usage?.cost ?? null, why: null },
  ])
  const total = $derived(rows.reduce((s, r) => s + (r.cost ?? 0), 0))
  const time = (iso: string) => dateFormat({ hour: '2-digit', minute: '2-digit' }).format(new Date(iso))
  const left = $derived([...new Set((agent.legs ?? []).map((l) => l.provider))].filter((p) => p !== agent.provider))
</script>

{#if agent.legs?.length}
  <section class="legs">
    <div class="head"><b>{t('CLIs')}</b>{#if total > 0}<span class="muted">{t('cost of the agent · ${cost}', { cost: total.toFixed(2) })}</span>{/if}</div>
    {#if total > 0}
      <div class="bar">{#each rows as r, i (i)}{#if r.cost}<i style:width="{(r.cost / total) * 100}%" style:background={CLIS[r.provider].color} title="{CLIS[r.provider].name} · ${r.cost.toFixed(2)}"></i>{/if}{/each}</div>
    {/if}
    {#each rows as r, i (i)}
      <div class="leg" class:now={!r.to}>
        <Icon name={CLIS[r.provider].icon} />
        <div class="what">
          {CLIS[r.provider].cli}{#if r.model}<span class="muted"> · {r.model}</span>{/if}
          <small>{time(r.from)} → {r.to ? time(r.to) : t('now')} · {tn(r.replies, '{n} reply', '{n} replies')}{#if r.why} · {r.why}{/if}</small>
        </div>
        <span class="amt">{r.cost != null ? `$${r.cost.toFixed(2)}` : '—'}</span>
      </div>
    {/each}
    {#if agent.state !== 'prepared'}
      <div class="back">
        {#each left as p (p)}<button onclick={() => backTo(agent, p)} title={CLIS[p].resumes_with_prompt ? t('{cli} picks its conversation up again, told what happened since', { cli: CLIS[p].name }) : t('{cli} starts a new conversation from a relay', { cli: CLIS[p].cli })}><Icon name="retry" size={11} />{t('Back to {cli}', { cli: CLIS[p].name })}</button>{/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  .legs { display: flex; flex-direction: column; gap: 4px; padding: 8px 0 10px; border-bottom: 1px solid var(--border); margin-bottom: 8px; }
  .head { display: flex; align-items: baseline; gap: 8px; font-size: var(--fs-meta); }
  .muted { color: var(--text-muted); }
  .bar { display: flex; height: 7px; border-radius: 4px; overflow: hidden; background: var(--surface-muted); }
  .leg { display: flex; align-items: center; gap: 8px; padding: 4px 0; font-size: var(--fs-meta); }
  .leg.now .what { font-weight: 500; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .what small { color: var(--text-muted); font-weight: 400; }
  .amt { color: var(--text-muted); font-size: var(--fs-tag, 11px); }
  .back { display: flex; gap: 6px; flex-wrap: wrap; }
  .back button { display: inline-flex; align-items: center; gap: 5px; padding: 2px 9px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
</style>
