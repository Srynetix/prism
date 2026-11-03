<script lang="ts">
  import { hasCore } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../../layout.svelte'
  import { locale } from '../../locale'
  import { t } from '../../i18n.svelte'
  import { call, type BlockProps } from '../block'
  import { bounds, change, dollars, ofTool, period, tokens, toolsOf, type Span } from './period'
  import { TOOL_LABEL, type TokenDay, type TokensState } from './types'

  /**
   * Tokens: what the coding tools (Claude Code, Codex, Command Code, OpenCode) consumed over a week
   * or a month, every session of this computer. The period's total against the one before, a bar a
   * day, then where they went: the kinds of tokens, the tools (when there are several), the models,
   * the projects; with several tools, one of them alone. ‹ › go back in time; the cost is at the API prices, a comparison
   * rather than a bill on a subscription. Short: the total and the bars.
   */
  let { state: usage, error }: BlockProps<TokensState> = $props()

  let span = $state<Span>(pref<Span>('tokens.span', 'week'))
  let offset = $state(0)
  function setSpan(s: Span) {
    setPref('tokens.span', (span = s))
    offset = 0
  }

  /**
   * The snapshot's recent days; the older ones asked of the computer the first time ‹ needs them (the
   * period shown, or the one it is compared with), then kept: they no longer change.
   */
  const recent = $derived(usage?.days ?? [])
  const first = $derived(recent[0]?.date ?? '')
  let older = $state.raw<TokenDay[]>([])
  let olderTo = ''
  $effect(() => {
    const need = iso(bounds(span, offset + 1).from)
    if (!usage?.oldest || !first || need >= first || olderTo === first) return
    olderTo = first
    call<TokenDay[]>('tokens', 'days', { to: first }).then(
      (d) => (older = d),
      () => (olderTo = ''),
    )
  })
  const days = $derived(older.length ? [...older.filter((d) => d.date < first), ...recent] : recent)
  /** The tools of the history; one picked ('' all of them), when there are several. */
  const tools = $derived(toolsOf(days))
  let picked = $state(pref<string>('tokens.tool', ''))
  const tool = $derived(tools.length > 1 && tools.includes(picked) ? picked : '')
  function setTool(name: string) {
    setPref('tokens.tool', (picked = name))
  }
  /** A phrase of the week or of the month: each its own, French gives them a gender. */
  const bySpan = (week: string, month: string, vars?: Record<string, string>) => (span === 'week' ? t(week, vars) : t(month, vars))
  const p = $derived(period(tool ? ofTool(days, tool) : days, span, offset))
  const peak = $derived(Math.max(1, ...p.days.map((d) => d.tokens)))
  const vs = $derived(change(p.totals.tokens, p.before.tokens))
  /** The oldest day kept: ‹ stops there. */
  const oldest = $derived(usage?.oldest ?? days[0]?.date ?? '')
  function iso(d: Date) {
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }
  const canBack = $derived(!!oldest && iso(p.from) > oldest)
  const today = $derived(iso(new Date()))

  const range = $derived(
    span === 'month'
      ? p.from.toLocaleDateString(locale(), { month: 'long', year: 'numeric' })
      : `${p.from.toLocaleDateString(locale(), { day: 'numeric', month: 'short' })} – ${p.to.toLocaleDateString(locale(), { day: 'numeric', month: 'short' })}`,
  )
  const label = (d: Date) => (span === 'week' ? d.toLocaleDateString(locale(), { weekday: 'narrow' }) : [1, 8, 15, 22, 29].includes(d.getDate()) ? String(d.getDate()) : '')
  const dayTitle = (d: { day: Date; tokens: number; cost: number; future: boolean }) =>
    t('{day}: {what}', { day: d.day.toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long' }), what: d.future ? t('to come') : d.tokens ? t('{n} tokens · ≈ {cost}', { n: tokens(d.tokens), cost: dollars(d.cost) }) : t('nothing') })

  const KINDS = [
    ['output', t('Output'), t('Written by the model')],
    ['input', t('Input'), t('Sent fresh (not from the cache)')],
    ['cache_write', t('Cache writes'), t('Context put in the cache')],
    ['cache_read', t('Cache reads'), t('Context read back from the cache: cheap, but most of the volume')],
  ] as const
  const share = (n: number) => (p.totals.tokens ? (n / p.totals.tokens) * 100 : 0)
  const model = (m: string) => m.replace(/^claude-/, '')

  let refreshing = $state(false)
  async function refresh() {
    refreshing = true
    try {
      await call('tokens', 'refresh')
    } finally {
      setTimeout(() => (refreshing = false), 1500)
    }
  }
</script>

<section class="panel" style="--src: var(--src-claude)" class:collapsed={isCollapsed('tokens')}>
  <h2>
    <span class="title"><Icon name="activity" />{t('Tokens')}</span>
    {#if usage}
      <small class="range">{range}</small>
      {#if hasCore()}<button class="icon" class:spin={refreshing} onclick={refresh} title={t('Read the sessions again now')} aria-label={t('Refresh')}><Icon name="refresh" /></button>{/if}
    {/if}
    <BlockControls id="tokens" />
  </h2>
  {#if !usage}
    <div class="empty">{t('Token use is not read. Turn on Settings › Agents › Tokens, then restart Prism.')}</div>
  {:else if !days.length}
    <div class="empty">{error ?? (usage.read_at ? t('No coding tool reply on this computer yet.') : t('Reading the coding tools’ sessions…'))}</div>
  {:else}
    <div class="body">
      <div class="tools">
        <span class="seg" role="group" aria-label={t('Period')}>
          <button class:on={span === 'week'} onclick={() => setSpan('week')}>{t('Week')}</button>
          <button class:on={span === 'month'} onclick={() => setSpan('month')}>{t('Month')}</button>
        </span>
        {#if tools.length > 1}
          <span class="seg" role="group" aria-label={t('Tool')}>
            <button class:on={!tool} onclick={() => setTool('')} title={t('Every tool')}>{t('All')}</button>
            {#each tools as tl (tl)}<button class:on={tool === tl} onclick={() => setTool(tl)} title={t('{tool} alone', { tool: TOOL_LABEL[tl] ?? tl })}>{TOOL_LABEL[tl] ?? tl}</button>{/each}
          </span>
        {/if}
        <span class="grow"></span>
        <span class="nav">
          <button class="icon" onclick={() => offset++} disabled={!canBack} title={bySpan('The week before', 'The month before')} aria-label={bySpan('Previous week', 'Previous month')}><Icon name="chevronLeft" /></button>
          {#if offset}<button class="now" onclick={() => (offset = 0)} title={bySpan('Back to this week', 'Back to this month')}>{bySpan('This week', 'This month')}</button>{/if}
          <button class="icon" onclick={() => offset--} disabled={offset === 0} title={bySpan('The week after', 'The month after')} aria-label={bySpan('Next week', 'Next month')}><Icon name="chevronRight" /></button>
        </span>
      </div>
      <div class="hero">
        <b>{tokens(p.totals.tokens)}</b><span class="unit">{t('tokens')}</span>
        {#if vs}<span class="vs" title="{tokens(p.before.tokens)} {p.partial ? bySpan('over the same days of the week before', 'over the same days of the month before') : bySpan('the week before', 'the month before')}">{vs} vs {p.partial ? bySpan('same days last week', 'same days last month') : bySpan('last week', 'last month')}</span>{/if}
        <span class="grow"></span>
        <span class="cost" title="{t('Claude Code’s and Codex’s at the models’ public API prices (on a subscription, a figure to compare rather than a bill), Command Code’s and OpenCode’s as they count it')}{p.totals.unpriced ? ` · ${t('{n} tokens without a cost left out (a model without a known price)', { n: tokens(p.totals.unpriced) })}` : ''}">{#if p.totals.cost || !p.totals.unpriced}≈ {dollars(p.totals.cost)}{p.totals.unpriced ? '+' : ''}{:else}{t('no cost')}{/if}</span>
      </div>
      <div class="bars" class:month={span === 'month'} role="img" aria-label={t('Tokens per day, {range}', { range })}>
        {#each p.days as d (d.date)}
          <div class="col" class:today={d.date === today} title={dayTitle(d)}>
            <span class="track">{#if d.future}<i class="future"></i>{:else if d.tokens}<i style:height="{Math.max(3, (d.tokens / peak) * 100)}%"></i>{/if}</span>
            <span class="lbl">{label(d.day)}</span>
          </div>
        {/each}
      </div>
      {#if !isShort('tokens') && p.totals.tokens}
        <div class="split">
          <div class="list">
            <span class="cap">{t('Kind')}</span>
            {#each KINDS as [key, name, hint] (key)}
              <div class="kv" title={hint}><span class="nm">{name}</span><span class="tr"><i style:width="{share(p.totals[key])}%"></i></span><span class="n">{tokens(p.totals[key])}</span></div>
            {/each}
          </div>
          <div class="list">
            {#if p.totals.tools.length > 1}
              <span class="cap">{t('Tool')}</span>
              {#each p.totals.tools as [tl, n] (tl)}
                <button class="kv pick" onclick={() => setTool(tl)} title={t('{tool} alone', { tool: TOOL_LABEL[tl] ?? tl })}><span class="nm">{TOOL_LABEL[tl] ?? tl}</span><span class="tr"><i style:width="{share(n)}%"></i></span><span class="n">{tokens(n)}</span></button>
              {/each}
            {/if}
            <span class="cap">{t('Model')}</span>
            {#each p.totals.models.slice(0, 4) as [m, n] (m)}
              <div class="kv" title={m}><span class="nm mono">{model(m)}</span><span class="tr"><i style:width="{share(n)}%"></i></span><span class="n">{tokens(n)}</span></div>
            {/each}
            <span class="cap">{t('Project')}</span>
            {#each p.totals.projects.slice(0, 5) as [name, n] (name)}
              <div class="kv" title={name}><span class="nm">{name}</span><span class="tr"><i style:width="{share(n)}%"></i></span><span class="n">{tokens(n)}</span></div>
            {/each}
          </div>
        </div>
        <p class="foot muted">{t('{n} replies', { n: p.totals.replies.toLocaleString(locale()) })} · {tool ? t('{tool}’s sessions of this computer', { tool: TOOL_LABEL[tool] ?? tool }) : t('every session of this computer')}{#if p.totals.unpriced}{' '}· {t('{n} without a cost (a model without a known price)', { n: tokens(p.totals.unpriced) })}{/if}</p>
      {/if}
    </div>
  {/if}
</section>

<style>
  .empty { padding: 14px 16px; color: var(--text-muted); font-size: var(--fs-body); }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); }
  .grow { flex: 1; }
  .icon { display: inline-grid; place-items: center; width: 22px; height: 22px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  .icon:disabled { opacity: 0.35; cursor: default; }
  .icon :global(svg) { width: 13px; height: 13px; }
  .spin :global(svg) { animation: spin 0.8s linear infinite; }
  .range { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .tools, .hero, .bars { flex-shrink: 0; }
  .tools { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .nav { display: inline-flex; align-items: center; gap: 2px; }
  .now { padding: 0 6px; border: none; border-radius: var(--r5); background: none; color: var(--info); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; font-size: var(--fs-tag); }
  .seg button { padding: 0 8px; border: none; background: none; color: var(--text-muted); font: inherit; line-height: 18px; cursor: pointer; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: var(--surface-muted); color: var(--text); font-weight: 500; }
  .body { display: flex; flex-direction: column; gap: 8px; padding: 6px 12px 10px; overflow: auto; }
  .hero { display: flex; align-items: baseline; gap: 6px; }
  .hero b { font-size: 22px; font-weight: 600; font-variant-numeric: tabular-nums; letter-spacing: -0.3px; }
  .unit { color: var(--text-muted); font-size: var(--fs-meta); }
  .vs { margin-left: 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  .cost { color: var(--text); font-size: var(--fs-body); font-variant-numeric: tabular-nums; }
  /* A bar a day, anchored to the baseline, a 2px gap between them. */
  .bars { display: flex; align-items: stretch; gap: 2px; height: 86px; }
  .col { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .track { flex: 1; display: flex; align-items: flex-end; border-bottom: 1px solid var(--border); }
  .track i { display: block; width: 100%; max-width: 28px; margin: 0 auto; border-radius: 4px 4px 0 0; background: var(--src-claude); }
  .col:hover .track i:not(.future) { background: color-mix(in srgb, var(--src-claude) 75%, var(--text)); }
  .track i.future { height: 3px; border-radius: 2px; background: none; border-top: 1px dashed var(--border); }
  .lbl { height: 12px; text-align: center; color: var(--text-muted); font-size: 10px; line-height: 12px; font-variant-numeric: tabular-nums; }
  .today .lbl { color: var(--text); font-weight: 600; }
  .split { display: grid; grid-template-columns: repeat(auto-fit, minmax(190px, 1fr)); gap: 6px 18px; }
  .list { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .cap { margin-top: 4px; color: var(--text-muted); font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; }
  .cap:first-child { margin-top: 0; }
  .kv { display: grid; grid-template-columns: minmax(0, 1fr) 70px 46px; align-items: center; gap: 8px; font-size: var(--fs-meta); }
  .pick { width: 100%; padding: 0; border: none; background: none; color: inherit; font-family: inherit; text-align: left; cursor: pointer; }
  .pick:hover .nm { color: var(--info); }
  .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tr { height: 4px; border-radius: 2px; background: var(--surface-muted); overflow: hidden; }
  .tr i { display: block; height: 100%; border-radius: 2px; background: color-mix(in srgb, var(--src-claude) 70%, transparent); }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  .foot { margin: 0; font-size: var(--fs-tag); }
</style>
