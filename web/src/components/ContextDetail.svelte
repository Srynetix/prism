<script lang="ts">
  import type { Agent } from '../types'
  import { dismissable } from '../popup'
  import { agentContextDetail, clearAgent, compactAgent, type AgentContextDetail } from '../agents.svelte'
  import { armer } from '../armed.svelte'
  import { cliOf } from '../agent-cli'
  import { toast } from '../toast.svelte'
  import ContextGauge from './ContextGauge.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The context gauge under an agent's terminal, which opens what its context is made of: the base
   * the CLI sends before your first word (measured), then the conversation since the last
   * compaction by kind and by tool (estimated, scaled to the last reply), and the heaviest files
   * read, commands run, pages fetched — their names, never their content. Claude Code and Codex;
   * the others, the gauge alone. Compact types `/compact` in the agent once its turn is over, Clear
   * `/clear` (a new conversation, the transcript kept).
   */
  let { agent }: { agent: Agent } = $props()

  const detailed = $derived(cliOf(agent.provider).transcript)
  let open = $state(false)
  let detail = $state<AgentContextDetail | null>(null)
  let error = $state('')
  let loaded = $state(false)
  let wrap = $state<HTMLElement>()
  // Each refresh hands a new `agent` object: what follows hangs on its id and its size only.
  const agentId = $derived(agent.id)
  // Read again on opening, and at each reply while open.
  const used = $derived(agent.context?.used)
  $effect(() => {
    if (!open) return
    void used
    const id = agentId
    agentContextDetail(id)
      .then((d) => {
        if (id === agent.id) [detail, error, loaded] = [d, '', true]
      })
      .catch((e) => (error = String(e)))
  })
  $effect(() => {
    void agentId
    open = false
    detail = null
    loaded = false
  })

  const LABELS: Record<AgentContextDetail['parts'][number]['kind'], string> = {
    summary: t('Summary of the compaction'),
    prompts: t('Your prompts'),
    replies: t('Replies & thinking'),
    calls: t('Tool calls'),
    results: t('Tool results'),
    images: t('Images & files'),
    reminders: t('Reminders & hooks'),
  }
  const tokens = (n: number) => (n >= 1_000_000 ? `${+(n / 1_000_000).toFixed(1)}M` : n >= 10_000 ? `${Math.round(n / 1000)}k` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : `${n}`)
  const total = $derived(detail?.size ?? detail?.used ?? 1)
  const pct = (n: number) => `${(n / total) * 100}%`
  const share = (n: number) => `${Math.round((n / Math.max(1, detail?.used ?? 1)) * 100)}%`
  let unfolded = $state(new Set<string>(['results']))
  const fold = (k: string) => {
    const next = new Set(unfolded)
    if (!next.delete(k)) next.add(k)
    unfolded = next
  }
  const since = $derived(detail?.compacted_at ? new Date(detail.compacted_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : null)

  const busy = $derived(agent.status === 'running' || agent.status === 'waiting_tool')
  const armed = armer()
  async function act(what: 'compact' | 'clear') {
    if (!armed.confirm(what)) return
    try {
      await (what === 'compact' ? compactAgent : clearAgent)(agent.id)
      open = false
    } catch (e) {
      toast(t(what === 'compact' ? 'Could not compact: {error}' : 'Could not clear: {error}', { error: e instanceof Error ? e.message : String(e) }))
    }
  }
</script>

{#if agent.context}
  {#if detailed}
    <span class="wrap" bind:this={wrap}>
      <button class="gauge" class:on={open} onclick={() => (open = !open)} aria-expanded={open} title={t('What the context is made of')}>
        <ContextGauge context={agent.context} />
      </button>
      {#if open}
        <div class="pop" role="dialog" aria-label={t('What the context is made of')} {@attach dismissable(() => (open = false), { trigger: () => wrap })}>
          {#if detail}
            <div class="head">
              <b>{detail.size ? `${Math.round((detail.used / detail.size) * 100)}%` : tokens(detail.used)}</b>
              <span>{detail.size ? t('{used} of {size} tokens', { used: tokens(detail.used), size: tokens(detail.size) }) : t('{n} tokens', { n: tokens(detail.used) })}</span>
              <span class="grow"></span>
              <small>{t('at the last reply')}{#if detail.size}{` · ${t('{n} left', { n: tokens(Math.max(0, detail.size - detail.used)) })}`}{/if}</small>
            </div>
            <div class="stack">
              {#if detail.base}<i class="k-base" style:width={pct(detail.base)}></i>{/if}
              {#each detail.parts as p (p.kind)}<i class="k-{p.kind}" style:width={pct(p.tokens)}></i>{/each}
            </div>
            <div class="rows">
              {#if detail.base}
                <div class="row"><span class="sw k-base"></span><span>{t('Base')} <span class="meas" title={t('The first request’s tokens, less your first prompt: system prompt, tools, MCP, skills, {file}', { file: agent.provider === 'codex' ? 'AGENTS.md' : 'CLAUDE.md' })}>{t('measured')}</span></span><span class="n">{tokens(detail.base)}</span><span class="p">{share(detail.base)}</span></div>
              {/if}
              {#each detail.parts as p (p.kind)}
                {@const foldable = p.tools.length > 0}
                <button class="row" class:foldable disabled={!foldable} onclick={() => fold(p.kind)}>
                  <span class="sw k-{p.kind}"></span><span>{LABELS[p.kind]}{#if foldable}<span class="car">{unfolded.has(p.kind) ? '▾' : '▸'}</span>{/if}</span><span class="n">{tokens(p.tokens)}</span><span class="p">{share(p.tokens)}</span>
                </button>
                {#if foldable && unfolded.has(p.kind)}
                  {#each p.tools as tool (tool.tool)}
                    <div class="row sub"><span>{tool.tool} · {tool.count}</span><span class="n">{tokens(tool.tokens)}</span><span class="p">{share(tool.tokens)}</span></div>
                  {/each}
                {/if}
              {/each}
              {#if detail.size}
                <div class="row"><span class="sw free"></span><span class="muted">{t('Free')}</span><span class="n muted">{tokens(Math.max(0, detail.size - detail.used))}</span><span class="p"></span></div>
              {/if}
            </div>
            {#if detail.heaviest.length}
              <p class="lbl">{t('Heaviest in the context')}</p>
              <div class="heavy">
                {#each detail.heaviest as h, i (i)}
                  <div class="h" title={h.what ?? h.tool}><span class="tool">{h.tool}{#if h.call}{' '}{t('call')}{/if}</span><span class="what">{h.what ?? '—'}</span><span class="n">{tokens(h.tokens)}</span><span class="turn">{t('turn {n}', { n: h.turn })}</span></div>
                {/each}
              </div>
            {/if}
            <p class="note">
              {#if since}{detail.compactions > 1 ? t('Since the compaction of {time} ({n} so far): what came before is now its summary.', { time: since, n: detail.compactions }) : t('Since the compaction of {time}: what came before is now its summary.', { time: since })} {/if}
              {#if detail.base}{t('Base measured at the first request; the rest ≈, by the size of each message, scaled to the last reply.')}{:else}{t('≈ by the size of each message, scaled to the last reply.')}{/if}
              {t('Names only, never a content.')}
            </p>
            <div class="actions">
              <button class="btn" class:armed={armed.is('clear')} onclick={() => act('clear')} disabled={busy} title={busy ? t('Once its turn is over') : t('Types /clear in the agent: a new conversation, nothing of this one in its context (its transcript stays)')}>{armed.is('clear') ? t('Clear? click again') : t('Clear')}</button>
              <button class="btn" class:armed={armed.is('compact')} onclick={() => act('compact')} disabled={busy} title={busy ? t('Once its turn is over') : t('Types /compact in the agent: the conversation gives way to its summary')}>{armed.is('compact') ? t('Compact? click again') : t('Compact')}</button>
            </div>
          {:else if error}
            <p class="note">{t('Could not read the context: {error}', { error })}</p>
          {:else if loaded}
            <p class="note">{t('No transcript to read yet: the detail comes with the agent’s first reply.')}</p>
          {:else}
            <p class="note">{t('Reading the transcript…')}</p>
          {/if}
        </div>
      {/if}
    </span>
  {:else}
    <ContextGauge context={agent.context} />
  {/if}
{/if}

<style>
  .wrap { position: relative; display: inline-flex; }
  .gauge { display: inline-flex; padding: 1px 6px; margin: -1px -6px; border: 1px solid transparent; border-radius: var(--r5); background: none; font: inherit; cursor: pointer; }
  .gauge:hover, .gauge.on { border-color: var(--border); background: var(--surface-muted); }
  .pop { position: absolute; left: 0; bottom: calc(100% + 8px); z-index: 30; width: 420px; max-width: calc(100vw - 32px); max-height: 70vh; overflow: auto; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); box-shadow: var(--shadow-pop); font-size: var(--fs-meta); }
  .head { display: flex; align-items: baseline; gap: 8px; margin-bottom: 8px; }
  .head b { font-size: 16px; }
  .head small, .muted { color: var(--text-muted); }
  .grow { flex: 1; }
  .stack { display: flex; height: 10px; border-radius: var(--r3); overflow: hidden; background: var(--surface-muted); }
  .stack i { display: block; height: 100%; }
  .rows { margin-top: 8px; display: flex; flex-direction: column; }
  .row { display: grid; grid-template-columns: 12px 1fr auto 36px; gap: 8px; align-items: center; padding: 2px 4px; margin: 0 -4px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: inherit; text-align: left; }
  .row.foldable { cursor: pointer; }
  .row.foldable:hover { background: var(--surface-muted); }
  .row:disabled { opacity: 1; cursor: default; }
  .row.sub { grid-template-columns: 1fr auto 36px; padding-left: 24px; color: var(--text-muted); }
  .sw { width: 9px; height: 9px; border-radius: var(--r2); }
  .sw.free { background: var(--surface-muted); border: 1px solid var(--border); }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  .p { text-align: right; color: var(--text-muted); font-size: var(--fs-tag); }
  .car { margin-left: 4px; color: var(--text-muted); font-size: 9px; }
  .meas { margin-left: 4px; padding: 0 5px; border-radius: var(--r9); background: var(--ok-bg); color: var(--ok); font-size: 9.5px; }
  .k-base { background: color-mix(in srgb, var(--text-muted) 70%, transparent); }
  .k-summary { background: color-mix(in srgb, var(--text-muted) 40%, transparent); }
  .k-prompts { background: var(--accent); }
  .k-replies { background: var(--violet); }
  .k-calls { background: var(--warn); }
  .k-results { background: var(--info); }
  .k-images { background: var(--ok); }
  .k-reminders { background: color-mix(in srgb, var(--violet) 40%, var(--text-muted)); }
  .lbl { margin: 10px 0 4px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); font-weight: 600; }
  .heavy { border: 1px solid var(--border); border-radius: var(--r6); }
  .h { display: grid; grid-template-columns: 64px minmax(0, 1fr) auto auto; gap: 8px; align-items: center; padding: 3px 8px; border-bottom: 1px solid var(--border); }
  .h:last-child { border-bottom: none; }
  .tool, .what { font: var(--fs-tag) var(--mono); }
  .tool, .turn { color: var(--text-muted); }
  .turn { font-size: var(--fs-tag); white-space: nowrap; }
  .what { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .note { margin: 8px 0 0; font-size: var(--fs-tag); color: var(--text-muted); line-height: 1.4; }
  .actions { display: flex; gap: 6px; margin-top: 8px; }
  .btn { padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.armed { color: var(--warn); border-color: var(--warn); }
</style>
