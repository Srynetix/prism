<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { hasWindows, pace, peak, resets, usageLevel, week, weekTitle, type PlanSection } from '../usage'
  import { cliOf } from '../agent-cli'
  import Usage from './Usage.svelte'
  import Icon from './Icon.svelte'
  import { t, tn, tc } from '../i18n.svelte'

  /**
   * Plan usage of the agent CLI in the status bar: the 5 h session (bar and %), then the week in
   * 7 cells with the "now" marker, as in the Claude block. A click opens that full
   * view (windows, %/day remaining, resets). With agents on other CLIs too, a chip after it names
   * them (orange or red once one nears a limit), and the view has a section per CLI.
   */
  let { sections, now, working = false }: { sections: PlanSection[]; now: number; working?: boolean } = $props()

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  /** The gauge: `[agents] provider`'s, or failing a plan of its own, the first CLI with one. */
  const lead = $derived(sections.find(hasWindows) ?? null)
  const rest = $derived(sections.filter((p) => p !== lead))
  const restLevel = $derived(usageLevel({ utilization: Math.max(0, ...rest.map(peak)), resets_at: null }))
  const five = $derived(lead?.usage?.five_hour ?? null)
  const seven = $derived(lead?.usage?.seven_day ?? null)
  const wk = $derived(week(seven, now))
  const line = (p: PlanSection) => {
    const f = p.usage?.five_hour
    const w = p.usage?.seven_day
    return [f && `5h ${Math.round(f.utilization)}%`, w && `${t('week')} ${Math.round(w.utilization)}%`].filter(Boolean).join(' · ') || (!cliOf(p.provider).plan ? t('no plan') : (p.error ?? t('not read yet')))
  }
  const title = $derived(
    [
      five && `${t('Session (5h)')}: ${Math.round(five.utilization)}%${five.resets_at ? ` · ${resets(five)}` : ''}`,
      seven && wk && `${t('Week')}: ${weekTitle(seven, wk)}`,
      lead?.error && t('Last fetch failed: {error}', { error: lead.error }),
      ...rest.map((p) => `${cliOf(p.provider).name}: ${line(p)}`),
    ]
      .filter(Boolean)
      .join('\n'),
  )
  const agents = (n: number) => (n ? tn(n, '{n} agent', '{n} agents') : t('no agent'))
</script>

{#if lead}
  <button bind:this={button} class="sbu" class:open class:stale={!!lead.error} style:--fill={cliOf(lead.provider).color} onclick={(e) => { e.stopPropagation(); open = !open }} {title} aria-expanded={open}>
    <span class="g" class:fun-turn={working} aria-hidden="true"><Icon name={cliOf(lead.provider).icon} /></span>
    {#if five}
      <span class="w {usageLevel(five)}"><span class="k">5h</span><span class="bar"><i style:width="{Math.min(100, five.utilization)}%"></i></span><b>{Math.round(five.utilization)}%</b></span>
    {/if}
    {#if seven && wk}
      <span class="w week {usageLevel(seven)}">
        <span class="k">{t('wk')}</span>
        <span class="bar7">{#each wk.days as d (d.from.getTime())}<i class:current={d.current} style:background-image={d.bg}>{#if d.current}<span class="now" style:left="{d.nowAt}%"></span>{/if}</i>{/each}</span>
        <b class="pace-{pace(wk.ahead)}">{Math.round(seven.utilization)}%</b>
      </span>
    {/if}
    {#if rest.length}
      <span class="more {restLevel}" aria-label={t('Also {names}', { names: rest.map((p) => cliOf(p.provider).name).join(', ') })}>+{#each rest as p (p.provider)}<span class="mg" style:color={cliOf(p.provider).color}><Icon name={cliOf(p.provider).icon} /></span>{/each}</span>
    {/if}
  </button>
  {#if open}
    <div class="pop" style:--cli-fill={cliOf(lead.provider).color} {@attach anchored(() => button, { align: 'end', gap: 6 })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button })}>
      {#if sections.length === 1}
        <p>{t('{cli} usage', { cli: cliOf(lead.provider).name })}</p>
        <Usage usage={lead.usage} error={lead.error} />
      {:else}
        <p><span>{t('Usage')}</span><span>{t('{n} CLIs in use', { n: sections.length })}</span></p>
        {#each sections as p (p.provider)}
          <section class="cli" style:--cli-fill={cliOf(p.provider).color}>
            <header>
              <span class="cg"><Icon name={cliOf(p.provider).icon} /></span>
              <b>{cliOf(p.provider).name}</b>
              {#if p.configured}<span class="def">{tc('chat', 'default')}</span>{/if}
              <span class="n">{agents(p.agents)}</span>
            </header>
            {#if !cliOf(p.provider).plan}
              <div class="none">{t('No plan: it runs on your own providers’ keys.')}</div>
            {:else}
              <Usage usage={p.usage} error={p.error} />
            {/if}
          </section>
        {/each}
      {/if}
    </div>
  {/if}
{/if}

<style>
  .sbu { display: flex; align-items: center; gap: 8px; height: 20px; padding: 0 7px; border: none; border-radius: var(--r5); background: none; color: inherit; font: inherit; cursor: pointer; white-space: nowrap; }
  .sbu:hover, .sbu.open { background: var(--surface-muted); color: var(--text); }
  .g { display: inline-flex; color: var(--fill); }
  .stale { opacity: 0.7; }
  .w { display: inline-flex; align-items: center; gap: 4px; }
  .w.warn { --fill: var(--warn); }
  .w.ko { --fill: var(--ko); }
  .k { font-size: var(--fs-tag); color: var(--text-muted); }
  .w b { font-weight: 600; color: var(--text); font-variant-numeric: tabular-nums; }
  .w.warn b { color: var(--warn); }
  .w.ko b { color: var(--ko); }
  .bar { width: 34px; height: 5px; border-radius: var(--r3); overflow: hidden; background: var(--border); }
  .bar i { display: block; height: 100%; background: var(--fill); }
  /* The week in 7 cells: used within pace, overrun, "now" marker. */
  .week { --claude-fill: var(--fill); --over-fill: var(--warn); }
  .week.ko { --over-fill: var(--ko); }
  .bar7 { display: flex; gap: 1px; height: 7px; }
  .bar7 i { position: relative; width: 7px; border-radius: calc(1.5px * var(--round)); background-color: var(--border); }
  .bar7 i.current { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text-muted) 60%, transparent); }
  .bar7 .now { position: absolute; top: -2px; bottom: -2px; width: 2px; margin-left: -1px; border-radius: var(--r1); background: var(--text); }
  .pop { position: fixed; z-index: 45; width: 420px; overflow: hidden; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); }
  /* The other CLIs in use: their glyphs, tinted once one of them nears a limit. */
  .more { display: inline-flex; align-items: center; gap: 2px; height: 15px; padding: 0 5px; border-radius: var(--r8); background: var(--surface-muted); font-size: var(--fs-tag); color: var(--text-muted); }
  .more.warn { background: color-mix(in srgb, var(--warn) 16%, transparent); color: var(--warn); }
  .more.ko { background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); }
  .mg { display: inline-flex; }
  .mg :global(svg) { width: 10px; height: 10px; }
  .pop p { display: flex; justify-content: space-between; margin: 0; padding: 7px 12px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  /* One section per CLI, in its colour. */
  .cli { border-top: 1px solid var(--border); background: color-mix(in srgb, var(--cli-fill) 6%, transparent); }
  .cli :global(.usage) { border-top: none; background: none; padding-top: 2px; }
  .cli header { display: flex; align-items: center; gap: 7px; padding: 7px 12px 2px; font-size: var(--fs-body); }
  .cli header b { color: var(--text); }
  .cg { display: inline-flex; color: var(--cli-fill); }
  .def { font-size: var(--fs-tag); color: var(--text-muted); }
  .cli .n { margin-left: auto; font-size: var(--fs-tag); color: var(--text-muted); }
  .none { padding: 2px 12px 8px; font-size: var(--fs-meta); color: var(--text-muted); }
</style>
