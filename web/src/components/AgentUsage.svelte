<script lang="ts">
  import type { AgentUsage } from '../types'
  import { agentsSettings } from '../agents.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * What an agent's conversation consumed (Claude Code, Codex), next to its context bar: the tokens of
   * the whole session and their cost at the API prices. A click opens the detail: the replies, the
   * tokens per reply (cache reads apart), and the split between cache reads, writes, input and output.
   * `compact`: the cost alone, for the list. `short`, on a phone: its tokens and cost without
   * the words, the detail still a tap away. The cost alert (`[agents.claude] cost_alert`) is Claude Code's.
   */
  let { usage, provider = 'claude', compact = false, short = false }: { usage: AgentUsage; provider?: string; compact?: boolean; short?: boolean } = $props()

  const total = $derived(usage.input + usage.output + usage.cache_read + usage.cache_write)
  const tokens = (n: number) => (n >= 1_000_000 ? `${+(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : `${n}`)
  const dollars = (n: number) => (n < 10 ? `$${n.toFixed(2)}` : `$${Math.round(n)}`)
  const limit = $derived(provider === 'claude' ? (agentsSettings().claude?.cost_alert ?? 0) : 0)
  const over = $derived(limit > 0 && (usage.cost ?? 0) >= limit)
  const title = $derived(usage.cost !== null ? t('{tokens} tokens over {n} replies, about {cost} at the API prices of {model}', { tokens: tokens(total), n: usage.replies, cost: dollars(usage.cost), model: usage.model ?? t('the model') }) : t('{tokens} tokens over {n} replies', { tokens: tokens(total), n: usage.replies }))

  let open = $state(false)
  const peak = $derived(Math.max(1, ...usage.recent))
  const split = $derived([
    [t('Cache read'), usage.cache_read, 'read'],
    [t('Cache write'), usage.cache_write, 'write'],
    [t('Input'), usage.input, 'in'],
    [t('Output'), usage.output, 'out'],
  ] as const)
</script>

{#if compact}
  {#if usage.cost !== null}<span class="cost" class:over {title}>{dollars(usage.cost)}</span>{/if}
{:else}
  <span class="wrap">
    <button class="chip" class:over class:on={open} onclick={() => (open = !open)} {title} aria-expanded={open}>
      {#if !short}{`${t('Session')} `}{/if}<b>{tokens(total)}</b>{#if !short}{` ${t('tokens')}`}{/if}{#if usage.cost !== null}{' · '}<b>≈ {dollars(usage.cost)}</b>{/if}
    </button>
    {#if open}
      <div class="pop" role="dialog" aria-label={t('What this agent consumed')}>
        <div class="kpis">
          <div><span>{t('Tokens')}</span><b>{tokens(total)}</b><small>{tn(usage.replies, '{n} reply', '{n} replies')}</small></div>
          <div><span>{t('Cost')}</span><b>{usage.cost !== null ? dollars(usage.cost) : '—'}</b><small>{usage.cost !== null ? `${t('API price')} · ${usage.model?.replace(/^claude-/, '') ?? ''}` : t('model price unknown')}</small></div>
          <div><span>{t('Per reply')}</span><b>{tokens(Math.round(total / Math.max(1, usage.replies)))}</b><small>{t('on average')}</small></div>
        </div>
        <p class="lbl">{t('Last {n} replies', { n: usage.recent.length })}</p>
        <div class="bars">{#each usage.recent as n, i (i)}<i style:height="{Math.max(3, (n / peak) * 100)}%" title={t('{n} tokens', { n: tokens(n) })}></i>{/each}</div>
        <div class="split">
          {#each split as [name, n, k] (k)}
            <div class="line"><span>{name}</span><span class="track"><i class={k} style:width="{total ? (n / total) * 100 : 0}%"></i></span><span class="n">{tokens(n)}</span></div>
          {/each}
        </div>
        <p class="note">{t('Estimated from the transcript’s usage figures at the public API prices; on a subscription, a figure of comparison, not a bill.')}{#if limit}{' '}{t('Flagged in Alerts beyond ${limit} ([agents.claude] cost_alert).', { limit })}{/if}</p>
      </div>
    {/if}
  </span>
{/if}

<style>
  .cost { font-size: var(--fs-tag); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .over { color: var(--warn) !important; }
  .wrap { position: relative; display: inline-flex; }
  .chip { padding: 0 7px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); white-space: nowrap; cursor: pointer; font-variant-numeric: tabular-nums; }
  .chip:hover, .chip.on { border-color: var(--border); background: var(--surface-muted); }
  .chip b { color: var(--text); font-weight: 600; }
  .chip.over b { color: var(--warn); }
  .pop { position: absolute; right: 0; bottom: calc(100% + 8px); z-index: 30; width: 340px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); box-shadow: var(--shadow-pop); }
  .kpis { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-bottom: 8px; }
  .kpis span { display: block; font-size: 9.5px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .kpis b { font-size: 15px; }
  .kpis small { display: block; font-size: var(--fs-tag); color: var(--text-muted); }
  .lbl { margin: 0 0 4px; font-size: var(--fs-tag); color: var(--text-muted); }
  .bars { display: flex; align-items: flex-end; gap: 2px; height: 48px; }
  .bars i { flex: 1; border-radius: var(--r2) var(--r2) 0 0; background: color-mix(in srgb, var(--src-claude) 70%, transparent); }
  .split { margin-top: 8px; }
  .line { display: grid; grid-template-columns: 80px 1fr 48px; gap: 8px; align-items: center; padding: 2px 0; font-size: var(--fs-meta); color: var(--text-muted); }
  .line .n { text-align: right; color: var(--text); font-variant-numeric: tabular-nums; }
  .track { height: 5px; border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .track i { display: block; height: 100%; }
  .track i.read { background: var(--info); }
  .track i.write { background: color-mix(in srgb, var(--info) 55%, var(--src-claude)); }
  .track i.in { background: var(--src-claude); }
  .track i.out { background: var(--accent); }
  .note { margin: 8px 0 0; font-size: var(--fs-tag); color: var(--text-muted); line-height: 1.4; }
</style>
