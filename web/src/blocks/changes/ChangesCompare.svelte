<script lang="ts">
  import { clock } from '../../clock.svelte'
  import { t } from '../../i18n.svelte'
  import Icon from '../../components/Icon.svelte'
  import { hm, isCrash, sentence, tickLabel, ticksOf, when, type ChangeEvent, type Range } from './changes'
  import { appOf, span, type Match, type Waiting } from './compare'
  import type { IconName } from '../../icons'

  /**
   * Two environments on the same axis (T-008): a lane each, the same change on both joined by a
   * line with its delay (red: it had crashed on the first), the first change still waiting on the
   * second marked; below, the box of what waits.
   */
  let { a, b, eventsA, eventsB, range, matches, waiting, highlight, onshow }: {
    a: string
    b: string
    eventsA: ChangeEvent[]
    eventsB: ChangeEvent[]
    range: Range
    matches: Match[]
    waiting: Waiting[]
    highlight: string | null
    onshow: (id: string) => void
  } = $props()

  const at = (e: ChangeEvent) => Date.parse(e.at)
  const pos = (ms: number) => Math.min(100, Math.max(0, ((ms - range.from) / Math.max(1, range.to - range.from)) * 100))
  const inRange = (e: ChangeEvent) => at(e) >= range.from && at(e) <= range.to
  const ticks = $derived(ticksOf(range))
  const shown = $derived(matches.filter((m) => inRange(m.a) || inRange(m.b)))
  // Labels only while they fit; each line keeps its tooltip.
  const labelled = $derived(shown.length <= 8)
  /** Each label's row (0–2), so that close ones do not overlap; by the match's index in `shown`. */
  const rowOfLabel = $derived.by(() => {
    const mids = shown.map((m, i) => ({ i, mid: (pos(at(m.a)) + pos(at(m.b))) / 2 })).sort((p, q) => p.mid - q.mid)
    const last = [-Infinity, -Infinity, -Infinity]
    const out: number[] = []
    for (const { i, mid } of mids) {
      const r = last.findIndex((x) => mid - x >= 16)
      out[i] = r < 0 ? last.indexOf(Math.min(...last)) : r
      last[out[i]] = mid
    }
    return out
  })
  /** The oldest change waiting, in the range: where the dashed line goes. */
  const firstWaiting = $derived(waiting.flatMap((w) => [w.e, ...w.earlier]).filter(inRange).sort((p, q) => at(p) - at(q))[0] ?? null)
  const waitingCount = $derived(waiting.reduce((n, w) => n + 1 + w.earlier.length, 0))
  let all = $state(false)
  const WAIT_ROWS = 5

  const now = $derived(clock(60_000).now)
  const what = (e: ChangeEvent) => sentence({ key: e.id, first: e, items: [e], t: at(e) }).what
  const ICON: Record<string, IconName> = { argocd: 'argocd', mrs: 'mr', pods: 'kube', alerts: 'warning', vulnerabilities: 'security' }

  function title(m: Match): string {
    const dir = m.delay >= 0 ? t('on {b} {time} after {a}', { a, b, time: span(m.delay) }) : t('on {b} {time} before {a}', { a, b, time: span(m.delay) })
    const basis = m.basis === 'image' ? t('same image {image}', { image: m.label }) : m.basis === 'mr' ? t('same MR {mr}', { mr: m.label }) : t('same revision {revision}', { revision: m.label })
    const crash = m.crash ? `\n↯ ${t('had crashed on {env} at {time}', { env: a, time: when(at(m.crash)) })}` : ''
    return `${appOf(m.b)} · ${basis}\n${dir}${crash}`
  }
</script>

<div class="cmp">
  <div class="names"><span class="na">{a}</span><span class="nb">{b}</span></div>
  <div class="track">
    <svg class="links" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
      {#each shown as m (m.a.id + m.b.id)}
        <line x1={pos(at(m.a))} y1="17" x2={pos(at(m.b))} y2="83" class:ko={m.crash} class:image={m.basis === 'image'} />
      {/each}
    </svg>
    <div class="lane la">
      {#each eventsA as e (e.id)}
        <button class="m {e.source}" class:crash={isCrash(e)} class:hl={highlight === e.id} style:left="{pos(at(e))}%" title="{hm(at(e))} · {what(e)}" aria-label={what(e)} onclick={() => onshow(e.id)}></button>
      {/each}
    </div>
    {#each shown as m, i (m.a.id + m.b.id)}
      {@const mid = (pos(at(m.a)) + pos(at(m.b))) / 2}
      <button class="delay" class:ko={m.crash} class:lone={!labelled} style:left="{mid}%" style:top="{24 + (labelled ? rowOfLabel[i] : 1) * 14}px"title={title(m)} onclick={() => onshow(m.b.id)}>
        {#if labelled}{#if m.crash}↯ {/if}<b>{span(m.delay)}</b> · {appOf(m.b)}{#if m.basis === 'image'}<i>{t('image')}</i>{/if}{:else}•{/if}
      </button>
    {/each}
    {#if firstWaiting}
      <span class="pend" style:left="{pos(at(firstWaiting))}%"><span>{t('waiting on {env} · {n}', { env: b, n: waitingCount })}</span></span>
    {/if}
    <div class="lane lb">
      {#each eventsB as e (e.id)}
        <button class="m {e.source}" class:crash={isCrash(e)} class:hl={highlight === e.id} style:left="{pos(at(e))}%" title="{hm(at(e))} · {what(e)}" aria-label={what(e)} onclick={() => onshow(e.id)}></button>
      {/each}
    </div>
    <div class="axis">
      {#each ticks as tk (tk)}<span style:left="{pos(tk)}%">{tickLabel(tk)}</span>{/each}
    </div>
  </div>
</div>

{#if waiting.length}
  <div class="wait">
    <div class="wh"><Icon name="pending" />{t('Waiting on {env} · {n}', { env: b, n: waitingCount })}<span class="grow"></span><small>{t('on {a}, not yet on {b}', { a, b })}</small></div>
    {#each all ? waiting : waiting.slice(0, WAIT_ROWS) as w (w.e.id)}
      {@const e = w.e}
      <button class="wr" onclick={() => onshow(e.id)}>
        <span class="src {e.source}" aria-hidden="true"><Icon name={ICON[e.source] ?? 'category'} /></span>
        <span class="app">{appOf(e)}</span>
        <span class="wt"><b class="mono">{e.revision ?? e.images[0] ?? ''}</b>{#if e.mr}{' '}· !{e.mr.iid} {e.mr.title}{/if}{#if w.earlier.length}{' '}<small class="earlier" title={w.earlier.map((x) => `${when(at(x))} · ${x.revision ?? x.images[0] ?? ''}`).join('\n')}>{t('+{n} before', { n: w.earlier.length })}</small>{/if} <small>— {t('on {env} for {time}', { env: a, time: span(now - at(e)) })}</small></span>
        {#if w.crash}<span class="chip ko">↯ {t('crashed on {env} at {time}', { env: a, time: when(at(w.crash)) })}</span>{:else}<span class="chip ok">{t('no crash on {env}', { env: a })}</span>{/if}
      </button>
    {/each}
    {#if waiting.length > WAIT_ROWS}
      <button class="more" onclick={() => (all = !all)}>{all ? t('Fewer') : t('+{n} more', { n: waiting.length - WAIT_ROWS })}</button>
    {/if}
  </div>
{/if}

<style>
  .cmp { display: grid; grid-template-columns: 72px minmax(0, 1fr); padding: 8px 14px 0 4px; border-bottom: 1px solid var(--border); }
  .names { position: relative; }
  .names span { position: absolute; right: 8px; overflow: hidden; max-width: 64px; font-size: var(--fs-meta); font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
  .na { top: 3px; color: var(--accent); }
  .nb { top: 73px; color: var(--info); }
  .track { position: relative; height: 112px; }
  .lane { position: absolute; left: 0; right: 0; height: 20px; border-radius: var(--r6); background: var(--surface-muted); }
  .la { top: 0; box-shadow: inset 2px 0 var(--accent); }
  .lb { top: 70px; box-shadow: inset 2px 0 var(--info); }
  .links { position: absolute; left: 0; right: 0; top: 0; height: 90px; width: 100%; overflow: visible; pointer-events: none; }
  .links line { stroke: var(--info); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .links line.image { stroke-dasharray: 4 3; }
  .links line.ko { stroke: var(--ko); }
  .m { position: absolute; top: 5px; width: 10px; height: 10px; padding: 0; border: 1.5px solid var(--surface); border-radius: 50%; background: var(--c, var(--info)); transform: translateX(-50%); cursor: pointer; }
  .m.argocd { --c: var(--c-argocd); }
  .m.mrs { --c: var(--c-mrs); }
  .m.pods { --c: var(--c-pods); }
  .m.alerts { --c: var(--c-alerts); }
  .m.vulnerabilities { --c: var(--c-vulnerabilities); }
  .m.crash { border-radius: 2px; background: var(--ko); transform: translateX(-50%) rotate(45deg); }
  .m.hl { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 55%, transparent); z-index: 1; }
  .delay { position: absolute; z-index: 1; padding: 0 6px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); white-space: nowrap; transform: translateX(-50%); cursor: pointer; }
  .delay b { color: var(--text); font-variant-numeric: tabular-nums; }
  .delay i { margin-left: 4px; font-style: normal; opacity: 0.75; }
  .delay.ko { border-color: var(--ko); color: var(--ko); }
  .delay.lone { padding: 0 3px; }
  .pend { position: absolute; top: 22px; height: 46px; border-left: 1.5px dashed var(--warn); pointer-events: none; }
  .pend span { position: absolute; bottom: 0; right: 4px; color: var(--warn); font-size: var(--fs-tag); white-space: nowrap; }
  .axis { position: absolute; left: 0; right: 0; top: 92px; height: 18px; }
  .axis span { position: absolute; top: 2px; transform: translateX(-50%); color: var(--text-muted); font-size: var(--fs-tag); font-variant-numeric: tabular-nums; white-space: nowrap; }

  .wait { margin: 8px 12px; overflow: hidden; border: 1px solid color-mix(in srgb, var(--warn) 60%, var(--border)); border-radius: var(--r6); background: color-mix(in srgb, var(--warn) 10%, var(--surface)); }
  .wh { display: flex; align-items: center; gap: 6px; padding: 5px 10px; color: var(--warn); font-weight: 600; }
  .wh :global(svg) { width: 12px; height: 12px; }
  .wh small { color: var(--text-muted); font-size: var(--fs-meta); font-weight: 400; }
  .grow { flex: 1; }
  .wr { display: grid; grid-template-columns: 18px 110px minmax(0, 1fr) auto; gap: 8px; align-items: center; width: 100%; padding: 4px 10px; border: none; border-top: 1px solid var(--border); background: var(--surface); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .wr:hover { background: var(--surface-muted); }
  .src { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: var(--r5); background: color-mix(in srgb, var(--c, var(--text-muted)) 20%, transparent); color: var(--c, var(--text-muted)); }
  .src :global(svg) { width: 11px; height: 11px; }
  .src.argocd { --c: var(--c-argocd); }
  .src.pods { --c: var(--c-pods); }
  .app { overflow: hidden; font-family: var(--mono); font-size: var(--fs-meta); text-overflow: ellipsis; white-space: nowrap; }
  .wt { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wt small { color: var(--text-muted); font-size: var(--fs-meta); }
  .wt .earlier { padding: 0 5px; border-radius: 8px; background: var(--surface-muted); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .chip { padding: 0 6px; border-radius: 8px; font-size: var(--fs-tag); font-weight: 600; white-space: nowrap; }
  .chip.ko { background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); }
  .chip.ok { background: color-mix(in srgb, var(--ok) 16%, transparent); color: var(--ok); }
  .more { width: 100%; padding: 3px 10px; border: none; border-top: 1px solid var(--border); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }

  /* Narrow: the lanes stay, the waiting rows on two lines. */
  @container changes (max-width: 640px) {
    .wr { grid-template-columns: 18px minmax(0, 1fr); }
    .wr .app { font-weight: 600; }
    .wr .wt, .wr .chip { grid-column: 2; justify-self: start; }
    .delay i { display: none; }
  }
</style>
