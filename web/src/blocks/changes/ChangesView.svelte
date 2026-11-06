<script lang="ts">
  import { tick, untrack } from 'svelte'
  import type { IconName } from '../../icons'
  import { t, tn } from '../../i18n.svelte'
  import { clock } from '../../clock.svelte'
  import { pref, setPref } from '../../layout.svelte'
  import { call } from '../block'
  import Icon from '../../components/Icon.svelte'
  import Logo from '../../components/Logo.svelte'
  import { openArgo } from '../../argo-view.svelte'
  import { openMr } from '../../mr-view.svelte'
  import { canShowPodLogs, openWorkload } from '../../pod-logs.svelte'
  import { openLink } from '../../links'
  import { copy } from '../../toast.svelte'
  import { keys } from '../../platform'
  import { claudeClick, diagnoseCrashAction } from '../../claude-actions'
  import type { ChangesFocus } from '../../changes-view.svelte'
  import { CORE_SOURCES, PERIODS, byHour, closedIn, counts, isCrash, markdown, periodLabel, rangeOf, rows, sentence, sourceLabel, tickLabel, ticksOf, visible, hm, when, type ChangeEvent, type ChangeLink, type ChangesState, type Period, type Range, type Row } from './changes'
  import { compare, defaultPair, sameApp, span, type Match } from './compare'
  import ChangesCompare from './ChangesCompare.svelte'

  /**
   * The Changes timeline of an environment: the compact band (a lane per source, a mark per
   * change, the ranges Prism was closed hatched; drag to zoom), then the list by hour, newest
   * first, repeats folded. A crash shows the change just before it on its application, "probably
   * linked"; Diagnose sends an agent on it in plan mode. `focus`: opened around a crash (drawer).
   * Compare: two environments on the same axis, their lists side by side (`ChangesCompare`).
   */
  let { state: journal, error, focus = null, compact = false }: { state: ChangesState | undefined; error?: string; focus?: ChangesFocus | null; compact?: boolean } = $props()

  const now = $derived(clock(60_000).now)
  const envs = $derived(journal?.envs ?? [])
  // The block's choices are kept; the drawer's start from its focus (the effect below).
  const first = untrack(() => ({ focus, period: journal?.default_period as Period | undefined }))
  let env = $state<string | null>(first.focus ? null : pref<string | null>('changes.env', null))
  let period = $state<Period>(pref<Period | null>('changes.period', null) ?? first.period ?? 'since_yesterday')
  let custom = $state<Range | null>(null)
  let hidden = $state(new Set<string>(first.focus ? [] : pref<string[]>('changes.hidden', [])))
  let query = $state('')
  let app = $state<string | null>(null)
  let highlight = $state<string | null>(null)
  // Compare: [A, B], A deployed first; null: one environment.
  let pair = $state<[string, string] | null>(first.focus ? null : pref<[string, string] | null>('changes.compare', null))
  let width = $state(1000)
  const narrow = $derived(width < 640)
  // Narrow: one list, the side picked.
  let side = $state<'a' | 'b'>('b')
  // A new focus (the drawer opened again elsewhere) resets the choices; the user's own stay until then.
  $effect(() => {
    const f = focus
    if (!f) return
    untrack(() => {
      env = f.env
      // Comparing two others: back to the focus' environment alone.
      if (pair && !(f.env && pair.includes(f.env))) pair = null
      app = f.app ?? null
      highlight = f.highlight ?? null
      custom = f.range ?? null
      period = f.range ? 'custom' : ((f.period as Period | undefined) ?? period)
    })
  })
  const pickEnv = (e: string | null) => {
    env = e
    if (!focus) setPref('changes.env', e)
  }
  const setPair = (p: [string, string] | null) => {
    pair = p
    if (!focus) setPref('changes.compare', p)
  }
  const toggleCompare = () => setPair(cmp ? null : defaultPair(envs, journal?.compare ?? [], env))
  /** Compare: an environment picked becomes B; B picked again swaps the two. */
  const pickSecond = (e: string) => {
    if (!pair) return
    if (e === pair[1] || e === pair[0]) setPair([pair[1], pair[0]])
    else setPair([pair[0], e])
  }
  const pickPeriod = (p: Period) => {
    period = p
    if (p !== 'custom') custom = null
    if (!focus && p !== 'custom') setPref('changes.period', p)
  }
  const toggle = (source: string) => {
    const next = new Set(hidden)
    if (next.has(source)) next.delete(source)
    else next.add(source)
    hidden = next
    if (!focus) setPref('changes.hidden', [...next])
  }

  const range = $derived(rangeOf(period, now, custom))
  const filter = $derived({ env, range, hidden, query, app })
  const events = $derived(visible(journal?.events ?? [], filter))
  const list = $derived(rows(events))
  // Comparing: the counts of the two environments.
  const sources = $derived.by(() => {
    const all = journal?.events ?? []
    const c = cmp
    return counts(c ? all.filter((e) => !e.env || c.includes(e.env)) : all, c ? { ...filter, env: null } : filter, journal?.sources ?? [...CORE_SOURCES])
  })
  const closed = $derived(closedIn(journal?.closed ?? [], range))
  const byId = $derived(new Map((journal?.events ?? []).map((e) => [e.id, e])))

  /* ---------- Compare ---------- */

  const cmp = $derived(pair && envs.includes(pair[0]) && envs.includes(pair[1]) ? pair : null)
  const comparison = $derived(cmp ? compare(journal?.events ?? [], cmp[0], cmp[1]) : null)
  const eventsA = $derived(cmp ? visible(journal?.events ?? [], { ...filter, env: cmp[0] }) : [])
  const eventsB = $derived(cmp ? visible(journal?.events ?? [], { ...filter, env: cmp[1] }) : [])
  /** Each change's match, by its id on either side. */
  const matchOf = $derived(new Map((comparison?.matches ?? []).flatMap((m) => [[m.a.id, m] as const, [m.b.id, m] as const])))
  const waitingIds = $derived(new Set((comparison?.waiting ?? []).flatMap((w) => [w.e.id, ...w.earlier.map((e) => e.id)])))
  /** B's crashes after a change that had crashed on A too (`related`, or the same app within correlate_minutes): their match, by the crash's id. */
  const seenBefore = $derived.by(() => {
    const out = new Map<string, Match>()
    const crashed = (comparison?.matches ?? []).filter((m) => m.crash)
    if (!crashed.length) return out
    const minutes = (journal?.correlate_minutes ?? 30) * 60_000
    for (const e of eventsB.filter(isCrash)) {
      const t0 = Date.parse(e.at)
      const m = crashed.find((m) => e.related === m.b.id || (t0 >= Date.parse(m.b.at) && t0 - Date.parse(m.b.at) <= minutes && sameApp(m.b, e)))
      if (m) out.set(e.id, m)
    }
    return out
  })
  const crashedBefore = (r: Row): Match | null => {
    for (const e of r.items) {
      const m = seenBefore.get(e.id)
      if (m) return m
    }
    return null
  }

  /** The list with the ranges Prism was closed, newest first, by hour. */
  type Item = { row: Row } | { closed: Range }
  type Group = { key: string; label: string; items: Item[] }
  const groups = $derived(groupsOf(list))
  const listA = $derived(rows(eventsA))
  const listB = $derived(rows(eventsB))
  function groupsOf(list: Row[]): Group[] {
    const items: (Item & { t: number })[] = [...list.map((row) => ({ row, t: row.t })), ...closed.map((c) => ({ closed: c, t: c.to }))]
    items.sort((a, b) => b.t - a.t)
    const hours = byHour(list)
    const hourOf = (ms: number) => {
      const d = new Date(ms)
      d.setMinutes(0, 0, 0)
      return String(d.getTime())
    }
    const out: Group[] = []
    for (const it of items) {
      const key = hourOf(it.t)
      let g = out[out.length - 1]
      if (!g || g.key !== key) {
        g = { key, label: hours.find((h) => h.key === key)?.label ?? hm(it.t), items: [] }
        out.push(g)
      }
      g.items.push(it)
    }
    return out
  }

  const ago = (ms: number) => {
    const m = Math.round(ms / 60_000)
    return m < 60 ? t('{n} min', { n: m }) : t('{h} h {m}', { h: Math.floor(m / 60), m: String(m % 60).padStart(2, '0') })
  }

  /* ---------- The band ---------- */

  const lanes = $derived(sources.map((s) => s.source).filter((s) => !hidden.has(s)))
  const x = (ms: number) => `${Math.min(100, Math.max(0, ((ms - range.from) / Math.max(1, range.to - range.from)) * 100))}%`
  const ticks = $derived(ticksOf(range))
  const tone = (e: ChangeEvent) => (e.severity === 'ko' ? 'ko' : e.severity === 'warn' ? 'warn' : e.severity === 'ok' ? 'ok' : 'info')

  // Drag on the band: the range picked, zoomed on.
  let band = $state<HTMLElement>()
  let drag = $state<{ from: number; to: number } | null>(null)
  const timeAt = (clientX: number) => {
    const r = band!.getBoundingClientRect()
    return range.from + ((clientX - r.left) / r.width) * (range.to - range.from)
  }
  function down(e: PointerEvent) {
    if (!band || e.button !== 0) return
    band.setPointerCapture(e.pointerId)
    const at = timeAt(e.clientX)
    drag = { from: at, to: at }
  }
  function move(e: PointerEvent) {
    if (drag) drag = { ...drag, to: timeAt(e.clientX) }
  }
  function up() {
    if (!drag) return
    const [from, to] = [Math.min(drag.from, drag.to), Math.max(drag.from, drag.to)]
    drag = null
    if (to - from < 2 * 60_000) return
    custom = { from, to }
    period = 'custom'
  }

  /* ---------- Rows ---------- */

  const ICON: Record<string, IconName> = { argocd: 'argocd', mrs: 'mr', pods: 'kube', alerts: 'warning', vulnerabilities: 'security' }
  const iconOf = (source: string): IconName => ICON[source] ?? 'category'

  function open(l: ChangeLink, e: ChangeEvent) {
    switch (l.kind) {
      case 'argo':
        return openArgo(l.cluster, l.app)
      case 'pod':
        return openWorkload({ context: l.context, namespace: l.namespace, deployment: l.deployment, label: `${l.deployment} · ${e.env ?? ''}` })
      case 'mr':
        return openMr({ iid: l.iid, repo: l.repo })
      case 'url':
        return openLink(l.url)
    }
  }
  const linkLabel = (l: ChangeLink) => ({ argo: 'Argo CD', pod: t('Logs'), mr: t('MR'), url: t('Open') })[l.kind]
  const linkable = (l: ChangeLink | undefined) => !!l && (l.kind !== 'pod' || canShowPodLogs())

  /** The row each change is shown in (a folded row holds several), by the change's id. */
  const rowOf = $derived(new Map((cmp ? [...listA, ...listB] : list).flatMap((r) => r.items.map((i) => [i.id, r.key] as const))))

  /** Scrolls to a change; one off the list (out of the range, its source hidden, another app) is brought in first. */
  async function show(id: string) {
    highlight = id
    let key = rowOf.get(id)
    const e = byId.get(id)
    if (!key && e) {
      const at = Date.parse(e.at)
      if (at < range.from || at > range.to) {
        custom = { from: Math.min(range.from, at - 15 * 60_000), to: Math.max(range.to, Math.min(now, at + 15 * 60_000)) }
        period = 'custom'
      }
      if (hidden.has(e.source)) hidden = new Set([...hidden].filter((x) => x !== e.source))
      if (!cmp && env && e.env && e.env !== env) env = e.env
      app = null
      query = ''
      await tick()
      key = rowOf.get(id)
    }
    // Narrow, comparing: the list of its side.
    if (cmp && narrow && e?.env && cmp.includes(e.env)) {
      side = e.env === cmp[0] ? 'a' : 'b'
      await tick()
    }
    if (key) document.querySelector(`[data-change="${CSS.escape(key)}"]`)?.scrollIntoView({ block: 'center', behavior: 'smooth' })
  }

  /** Diagnose a crash: its application's timeline, the change linked, where it runs. */
  function diagnose(r: Row) {
    const e = r.first
    const appName = e.app ?? e.title
    const linked = e.related ? byId.get(e.related) : undefined
    const linkedText = linked ? `${sentence({ key: linked.id, first: linked, items: [linked], t: Date.parse(linked.at) }).what}${linked.revision ? ` @ ${linked.revision}` : ''}${linked.mr ? `, !${linked.mr.iid} ${linked.mr.title}` : ''}` : ''
    const where = e.link?.kind === 'pod' ? `namespace ${e.link.namespace}, context ${e.link.context}, Deployment ${e.link.deployment}` : (e.namespace ? `namespace ${e.namespace}` : '')
    // Its application's timeline, from 3 h before: built at launch only.
    const span = { from: Date.parse(e.at) - 3 * 3600_000, to: now }
    const timeline = () => markdown(rows(visible(journal?.events ?? [], { env: e.env ?? env, range: span, hidden: new Set(), query: '', app: appName })), e.env ?? env, span)
    return diagnoseCrashAction(appName, e.env ?? env ?? '', where, linkedText, timeline)
  }

  let refreshing = $state(false)
  async function refresh() {
    refreshing = true
    try {
      await call('changes', 'refresh')
    } finally {
      setTimeout(() => (refreshing = false), 1500)
    }
  }

  // Opened around a crash: scrolled to it.
  $effect(() => {
    const id = focus?.crash
    if (!id) return
    requestAnimationFrame(() => {
      const key = untrack(() => rowOf.get(id))
      if (key) document.querySelector(`[data-change="${CSS.escape(key)}"]`)?.scrollIntoView({ block: 'center' })
    })
  })
</script>

<div class="changes" class:compact bind:clientWidth={width}>
  <div class="bar">
    {#if cmp}
      <span class="pills" role="group" aria-label={t('Environments compared')}>
        {#each envs as e (e)}
          <button class="pill" class:on={e === cmp[0]} class:b={e === cmp[1]} aria-pressed={cmp.includes(e)} onclick={() => pickSecond(e)} title={cmp.includes(e) ? t('Swap the two') : t('Compare {a} with {b}', { a: cmp[0], b: e })}>{e}</button>
        {/each}
        <button class="swap" onclick={() => setPair([cmp[1], cmp[0]])} title={t('Swap the two')} aria-label={t('Swap the two')}>⇄</button>
      </span>
    {:else}
      <span class="pills" role="radiogroup" aria-label={t('Environment')}>
        <button role="radio" aria-checked={env === null} class="pill" class:on={env === null} onclick={() => pickEnv(null)}>{t('All')}</button>
        {#each envs as e (e)}
          <button role="radio" aria-checked={env === e} class="pill" class:on={env === e} onclick={() => pickEnv(e)}>{e}</button>
        {/each}
      </span>
    {/if}
    <select class="sel" value={period} onchange={(e) => pickPeriod(e.currentTarget.value as Period)} aria-label={t('Period')}>
      {#each PERIODS as p (p)}<option value={p}>{periodLabel(p)}</option>{/each}
      {#if period === 'custom'}<option value="custom">{hm(range.from)} → {hm(range.to)}</option>{/if}
    </select>
    {#if app}<button class="pill on" onclick={() => (app = null)} title={t('Every application')}>{app} ✕</button>{/if}
    <span class="grow"></span>
    <input class="search" type="search" placeholder={t('App, author, revision…')} bind:value={query} aria-label={t('Search')} />
    {#if envs.length > 1}<button class="cbtn" class:on={!!cmp} aria-pressed={!!cmp} onclick={toggleCompare} title={t('Two environments on the same axis: what went on one and not yet on the other')}><Icon name="split" />{t('Compare')}</button>{/if}
    <button class="icon" onclick={() => copy(cmp ? `${markdown(listA, cmp[0], range)}\n\n${markdown(listB, cmp[1], range)}` : markdown(list, env, range), t('Changes copied'))} title={t('Copy as Markdown (post-mortem, message)')} aria-label={t('Copy')} disabled={cmp ? !listA.length && !listB.length : !list.length}><Icon name="copy" /></button>
    <button class="icon" class:spin={refreshing} onclick={refresh} title={t('Read the sources again now')} aria-label={t('Refresh')}><Icon name="refresh" /></button>
  </div>

  <div class="filters">
    {#each sources as s (s.source)}
      <button class="chip" class:off={hidden.has(s.source)} onclick={() => toggle(s.source)} aria-pressed={!hidden.has(s.source)} title={hidden.has(s.source) ? t('Show') : t('Hide')}>
        <Icon name={iconOf(s.source)} />{sourceLabel(s.source)} <b>{s.n}</b>
      </button>
    {/each}
  </div>

  {#if cmp && comparison}
    <ChangesCompare a={cmp[0]} b={cmp[1]} {eventsA} {eventsB} {range} matches={comparison.matches} waiting={comparison.waiting} {highlight} onshow={show} />
  {:else}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="band" title={t('Drag to zoom on a range')}>
    <div class="track" bind:this={band} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={() => (drag = null)}>
      {#each closed as c (c.from)}
        <div class="gap" style:left={x(c.from)} style:width={`calc(${x(c.to)} - ${x(c.from)})`} title={t('Prism closed: {from} → {to}', { from: hm(c.from), to: hm(c.to) })}></div>
      {/each}
      {#each lanes as lane (lane)}
        <div class="lane">
          <span class="lbl">{sourceLabel(lane)}</span>
          {#each events.filter((e) => e.source === lane) as e (e.id)}
            {@const what = sentence({ key: e.id, first: e, items: [e], t: Date.parse(e.at) }).what}
            <button class="m {tone(e)}" class:hl={highlight === e.id} class:crash={isCrash(e)} style:left={x(Date.parse(e.at))} title={`${hm(Date.parse(e.at))} · ${what}`} onpointerdown={(ev) => ev.stopPropagation()} onclick={() => show(e.id)} aria-label={what}></button>
          {/each}
        </div>
      {/each}
      <div class="axis">
        {#each ticks as tk (tk)}<span style:left={x(tk)}>{tickLabel(tk)}</span>{/each}
      </div>
      {#if drag}<div class="brush" style:left={x(Math.min(drag.from, drag.to))} style:width={`calc(${x(Math.max(drag.from, drag.to))} - ${x(Math.min(drag.from, drag.to))})`}></div>{/if}
    </div>
  </div>
  {/if}

  {#if cmp}
    {#if narrow}
      <div class="sides" role="radiogroup" aria-label={t('List')}>
        <button role="radio" aria-checked={side === 'a'} class="pill" class:on={side === 'a'} onclick={() => (side = 'a')}>{cmp[0]}</button>
        <button role="radio" aria-checked={side === 'b'} class="pill b" class:on={side === 'b'} onclick={() => (side = 'b')}>{cmp[1]}</button>
      </div>
      <div class="list">{@render listOf(groupsOf(side === 'a' ? listA : listB), cmp[side === 'a' ? 0 : 1])}</div>
    {:else}
      <div class="cols">
        <div class="list"><div class="colh"><span class="pill on">{cmp[0]}</span></div>{@render listOf(groupsOf(listA), cmp[0])}</div>
        <div class="list"><div class="colh"><span class="pill b">{cmp[1]}</span></div>{@render listOf(groupsOf(listB), cmp[1])}</div>
      </div>
    {/if}
  {:else}
    <div class="list">{@render listOf(groups, env)}</div>
  {/if}
  {#if !cmp && list.length}<p class="note">{tn(events.length, '{n} change recorded', '{n} changes recorded')} · {t('Times are the source’s; “seen”: when Prism saw it.')}</p>{/if}
  {#if cmp && comparison}<p class="note">{t('Linked: the same revision or MR on both; otherwise the same application with the same image (dashed).')}</p>{/if}
</div>

{#snippet listOf(groups: Group[], env: string | null)}
    {#each groups as g (g.key)}
      <div class="hour">{g.label}</div>
      {#each g.items as it ('row' in it ? it.row.key : `closed-${it.closed.from}`)}
        {#if 'closed' in it}
          <div class="closed">{t('Prism closed {from} → {to}: alerts and vulnerabilities not read over that range', { from: hm(it.closed.from), to: hm(it.closed.to) })}</div>
        {:else}
          {@const r = it.row}
          {@const e = r.first}
          {@const s = sentence(r)}
          {@const linked = e.related ? byId.get(e.related) : undefined}
          <div class="ev {tone(e)}" class:hl={highlight !== null && r.items.some((i) => i.id === highlight)} data-change={r.key}>
            <span class="t mono" title={e.seen ? t('Seen by Prism at this time: the source gives no date') : undefined}>{hm(r.t)}{#if e.seen}<i>{t('seen')}</i>{/if}</span>
            <span class="src {e.source}" aria-hidden="true"><Icon name={iconOf(e.source)} /></span>
            <span class="what">
              <span class="line">{#if !env && e.env}<span class="env">{e.env}</span>{/if}{s.what}{#if e.mr && e.kind !== 'merged'}{' '}<button class="mr" onclick={() => openMr({ iid: e.mr!.iid, repo: e.mr!.repo })} title={e.mr.title}>!{e.mr.iid}</button>{/if}</span>
              {#if s.detail}<small>{s.detail}</small>{/if}
              {#if cmp}
                {@const m = matchOf.get(e.id)}
                {@const seen = crashedBefore(r)}
                {#if m && m.a.id === e.id}
                  <button class="cmpnote" onclick={() => show(m.b.id)} title={m.basis === 'image' ? t('same image {image}', { image: m.label }) : undefined}>→ {m.delay >= 0 ? t('on {env} {time} after', { env: cmp[1], time: span(m.delay) }) : t('on {env} {time} before', { env: cmp[1], time: span(m.delay) })}{#if m.basis === 'image'} · {t('image')}{/if}</button>
                {:else if m && m.b.id === e.id}
                  <button class="cmpnote" onclick={() => show(m.a.id)} title={m.basis === 'image' ? t('same image {image}', { image: m.label }) : undefined}>← {cmp[0]} {when(Date.parse(m.a.at))}{#if m.basis === 'image'} · {t('image')}{/if}</button>
                  {#if m.crash}<button class="warnchip" onclick={() => show(m.crash!.id)}>↯ {t('had crashed on {env} at {time}', { env: cmp[0], time: when(Date.parse(m.crash.at)) })}</button>{/if}
                {:else if waitingIds.has(e.id)}
                  <span class="cmpnote wait">{t('not on {env} yet', { env: cmp[1] })}</span>
                {/if}
                {#if seen?.crash}<button class="warnchip" onclick={() => show(seen.crash!.id)}>↯ {t('already seen on {env} at {time}', { env: cmp[0], time: when(Date.parse(seen.crash.at)) })}</button>{/if}
              {/if}
              {#if linked}
                <button class="linked" onclick={() => show(linked.id)} title={t('Same application, {time} before: a guess on time and name, not a certain cause', { time: ago(Date.parse(e.at) - Date.parse(linked.at)) })}>
                  ↖ {t('probably linked: {what}', { what: sentence({ key: linked.id, first: linked, items: [linked], t: Date.parse(linked.at) }).what })} · {t('{time} before', { time: ago(Date.parse(e.at) - Date.parse(linked.at)) })}
                </button>
              {/if}
            </span>
            <span class="acts">
              {#if isCrash(e)}
                {@const a = diagnose(r)}
                {#if a.href}<a class="act" href={a.href} onclick={claudeClick(a)} title={`${t('An agent in plan mode: the logs, the events and the diff of the change linked')}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{a.label}</a>{/if}
              {/if}
              {#if linkable(e.link)}<button class="act" onclick={() => open(e.link!, e)}>{linkLabel(e.link!)}</button>{/if}
            </span>
          </div>
        {/if}
      {/each}
    {:else}
      <div class="empty">
        {#if !journal?.events.length}{t('Nothing recorded yet: Prism notes what changes as it reads Argo CD, the clusters, the forge, the alerts and the scans.')}{:else}{t('Nothing changed on this range.')}{/if}
        {#if error}<br /><small>{error}</small>{/if}
      </div>
    {/each}
{/snippet}

<style>
  .changes { flex: 1; min-height: 0; display: flex; flex-direction: column; container: changes / inline-size; font-size: var(--fs-body); --c-argocd: #e96d36; --c-mrs: var(--violet); --c-pods: var(--src-k8s); --c-alerts: var(--ko); --c-vulnerabilities: var(--warn); }
  .bar { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .pills { display: inline-flex; flex-wrap: wrap; gap: 4px; }
  .pill { padding: 1px 9px; border: 1px solid var(--border); border-radius: 10px; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .pill.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); font-weight: 600; }
  .sel { padding: 1px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .grow { flex: 1; }
  .pill.b { border-color: var(--info); background: color-mix(in srgb, var(--info) 14%, transparent); color: var(--info); font-weight: 600; }
  .swap { padding: 0 2px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: 13px; cursor: pointer; }
  .swap:hover { color: var(--text); }
  .cbtn { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .cbtn :global(svg) { width: 11px; height: 11px; }
  .cbtn.on { border-color: var(--accent); color: var(--accent); font-weight: 600; }
  .cols { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .cols > .list + .list { border-left: 1px solid var(--border); }
  .colh { position: sticky; top: 0; z-index: 2; padding: 4px 12px; background: var(--surface); border-bottom: 1px solid var(--border); }
  .cols .hour { top: 27px; }
  .sides { display: flex; gap: 4px; padding: 5px 12px; border-bottom: 1px solid var(--border); }
  .cmpnote { display: inline-block; margin-top: 2px; padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .cmpnote.wait { color: var(--warn); cursor: default; }
  .warnchip { display: inline-block; margin: 2px 0 0 6px; padding: 0 6px; border: none; border-radius: 8px; background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); font: inherit; font-size: var(--fs-tag); font-weight: 600; cursor: pointer; }
  .search { width: 170px; max-width: 40%; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .icon { display: inline-grid; place-items: center; width: 22px; height: 22px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  .icon:disabled { opacity: 0.4; cursor: default; }
  .icon :global(svg) { width: 13px; height: 13px; }
  .spin :global(svg) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .filters { display: flex; flex-wrap: wrap; gap: 5px; padding: 5px 12px; border-bottom: 1px solid var(--border); }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: none; border-radius: 10px; background: var(--surface-muted); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .chip :global(svg) { width: 11px; height: 11px; }
  .chip b { font-variant-numeric: tabular-nums; }
  .chip.off { opacity: 0.45; text-decoration: line-through; }

  .band { padding: 6px 12px 0; border-bottom: 1px solid var(--border); }
  .track { position: relative; margin-left: 80px; cursor: crosshair; user-select: none; touch-action: pan-y; }
  .lane { position: relative; height: 18px; border-bottom: 1px dashed var(--border); }
  .lbl { position: absolute; left: -82px; top: 2px; width: 76px; overflow: hidden; color: var(--text-muted); font-size: var(--fs-tag); text-align: right; text-overflow: ellipsis; white-space: nowrap; }
  .m { position: absolute; top: 4px; width: 9px; height: 9px; padding: 0; border: 1.5px solid var(--surface); border-radius: 50%; background: var(--info); transform: translateX(-50%); cursor: pointer; }
  .m.ok { background: var(--ok); }
  .m.warn { background: var(--warn); }
  .m.ko { background: var(--ko); }
  .m.crash { width: 11px; height: 11px; top: 3px; }
  .m.hl { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 55%, transparent); z-index: 1; }
  .axis { position: relative; height: 18px; }
  .axis span { position: absolute; top: 2px; transform: translateX(-50%); color: var(--text-muted); font-size: var(--fs-tag); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .gap { position: absolute; top: 0; bottom: 18px; background: repeating-linear-gradient(135deg, transparent 0 4px, var(--border) 4px 6px); }
  .brush { position: absolute; top: 0; bottom: 18px; border-left: 2px solid var(--accent); border-right: 2px solid var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); pointer-events: none; }

  .list { flex: 1; min-height: 0; overflow: auto; }
  .hour { position: sticky; top: 0; z-index: 1; padding: 3px 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; }
  .closed { margin: 3px 12px 3px 88px; padding: 2px 8px; border: 1px dashed var(--border); border-radius: var(--r6); background: repeating-linear-gradient(135deg, transparent 0 6px, color-mix(in srgb, var(--border) 50%, transparent) 6px 8px); color: var(--text-muted); font-size: var(--fs-meta); }
  .ev { display: grid; grid-template-columns: 58px 20px minmax(0, 1fr) auto; gap: 0 8px; align-items: start; padding: 5px 12px; border-top: 1px solid var(--border); }
  .ev:hover { background: var(--surface-muted); }
  .ev.ko { box-shadow: inset 2px 0 var(--ko); }
  .ev.hl { background: color-mix(in srgb, var(--accent) 12%, transparent); box-shadow: inset 3px 0 var(--accent); }
  .t { padding-top: 1px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .t i { display: block; font-size: 9.5px; font-style: normal; }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .src { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: var(--r5); background: color-mix(in srgb, var(--c, var(--text-muted)) 20%, transparent); color: var(--c, var(--text-muted)); }
  .src :global(svg) { width: 11px; height: 11px; }
  .src.argocd { --c: var(--c-argocd); }
  .src.mrs { --c: var(--c-mrs); }
  .src.pods { --c: var(--c-pods); }
  .src.alerts { --c: var(--c-alerts); }
  .src.vulnerabilities { --c: var(--c-vulnerabilities); }
  .what { min-width: 0; line-height: 1.4; }
  .line { overflow-wrap: anywhere; }
  .env { margin-right: 6px; padding: 0 5px; border-radius: var(--r5); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }
  .what small { display: block; overflow: hidden; color: var(--text-muted); font-size: var(--fs-meta); text-overflow: ellipsis; white-space: nowrap; }
  .mr { padding: 0; border: none; background: none; color: var(--info); font: inherit; font-family: var(--mono); font-size: var(--fs-meta); cursor: pointer; }
  .linked { display: block; margin-top: 2px; padding: 0 6px; border: 1px dashed color-mix(in srgb, var(--warn) 60%, var(--border)); border-radius: var(--r5); background: none; color: var(--warn); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .acts { display: inline-flex; gap: 4px; }
  .act { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); text-decoration: none; white-space: nowrap; cursor: pointer; }
  .act:hover { border-color: var(--accent); }
  .empty { padding: 14px 16px; color: var(--text-muted); }
  .note { margin: 0; padding: 4px 12px 6px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-tag); }

  .compact .band { display: none; }
  /* The phone: the band scrolls sideways, the list takes the width. */
  :global(html.remote) .band { overflow-x: auto; }
  :global(html.remote) .track { min-width: 560px; }
  :global(html.remote) .search { width: 100%; max-width: none; order: 10; }
  :global(html.remote) .ev { grid-template-columns: 54px 20px minmax(0, 1fr); }
  :global(html.remote) .acts { grid-column: 3; flex-wrap: wrap; margin-top: 3px; }
</style>
