<script lang="ts">
  import { t } from '../i18n.svelte'
  import { untrack } from 'svelte'
  import BlockControls from './BlockControls.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { notes } from '../notes.svelte'
  import { hours, median, retro, retroMarkdown, type RetroSummary } from '../retro'
  import { addPoint, nextKind, POINT_KINDS, POINT_LABEL, pointsOf, pointsStorage, removePoint, sprintsWithPoints, updatePoint, type PointKind, type RetroPoint } from '../retro-points.svelte'
  import { claudeClick, retroAction } from '../claude-actions'
  import { canPreview, openTicket } from '../ticket-view.svelte'
  import { ticketKey, trackerLogo } from '../tracker'
  import { openSettings } from '../settings.svelte'
  import { copy } from '../toast.svelte'
  import { locale } from '../locale'
  import type { TrailState } from '../blocks/trail/trail'
  import type { Snapshot } from '../types'
  import SprintActuals from './SprintActuals.svelte'
  import { conventions } from '../conventions'

  /**
   * My sprint: the current sprint from your side, for the retro (see retro.ts). Your figures,
   * compared with your own previous sprint; each sprint's are kept in the layout's prefs when the
   * block shows them.
   */
  let { snap }: { snap: Snapshot } = $props()

  const HISTORY = 'retro.history'
  /** Sprints kept to compare with. */
  const KEEP = 8
  let history = $state<RetroSummary[]>(pref<RetroSummary[]>(HISTORY, []))
  const trail = $derived(snap.blocks?.trail as unknown as TrailState | undefined)
  // The history read untracked: keeping this sprint's figures in it (below) must not compute it all again.
  const r = $derived(retro(snap, trail, notes, untrack(() => history)))
  const previous = $derived(r ? history.filter((h) => h.name !== r.name).at(-1) : undefined)

  // The sprint's figures, kept for the next sprints to compare with (the last state seen).
  $effect(() => {
    const s = r?.summary
    if (!s) return
    const old = history.find((h) => h.name === s.name)
    if (old && JSON.stringify(old) === JSON.stringify(s)) return
    history = [...history.filter((h) => h.name !== s.name), s].slice(-KEEP)
    setPref(HISTORY, history)
  })

  const tracked = $derived(r ? r.time.sprint + r.time.other + r.time.none : 0)
  const all = $derived(r ? tracked + r.time.meetings : 0)
  const pct = (n: number) => (all ? Math.round((100 * n) / all) : 0)
  const focus = $derived(r ? median(r.daily.map((d) => d.focus)) : 0)
  const maxFocus = $derived(r ? Math.max(60, ...r.daily.map((d) => d.focus)) : 60)
  const delta = (now: number, before: number | undefined, unit = '') => (before === undefined || now === before ? '' : `${now > before ? '▲' : '▼'} ${Math.abs(now - before)}${unit}`)
  const dayLabel = (day: string) => new Date(`${day}T12:00:00`).toLocaleDateString(locale(), { weekday: 'narrow' })
  const dayTitle = (day: string) => new Date(`${day}T12:00:00`).toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'short' })
  /**
   * Points for the retro, written here: those of the sprint shown, the current one by default; any
   * sprint with points can be picked (the retro often comes once the next one has begun).
   */
  let picked = $state('')
  const pointSprints = $derived(r ? [...new Set([r.name, ...sprintsWithPoints()])] : [])
  const shown = $derived(picked && pointSprints.includes(picked) ? picked : (r?.name ?? ''))
  const points = $derived(pointsOf(shown))
  let pointKind = $state<PointKind>(pref<PointKind>('retro.kind', 'change'))
  const setKind = (k: PointKind) => setPref('retro.kind', (pointKind = k))
  let pointText = $state('')
  let editing = $state<string | null>(null)
  let editText = $state('')
  function submitPoint() {
    addPoint(shown, pointText, pointKind)
    pointText = ''
  }
  function endEdit(p: RetroPoint, keep: boolean) {
    if (keep && editText !== p.text) updatePoint(shown, p.id, { text: editText })
    editing = null
  }
  /** The retro, or the sprint's Actuals (`[tracker] actuals`): estimate against actual, by ticket. */
  let tab = $state<'retro' | 'actuals'>(pref<'retro' | 'actuals'>('retro.tab', 'retro'))
  const setTab = (v: 'retro' | 'actuals') => setPref('retro.tab', (tab = v))
  const actuals = $derived(conventions().actuals && tab === 'actuals')
  const md = $derived(r ? retroMarkdown(r, pointsOf(r.name)) : '')
  const draft = $derived(r ? retroAction(md, r.name) : null)
</script>

<section class="panel" style="--src: var(--src-clickup)" class:collapsed={isCollapsed('retro')}>
  <h2>
    <span class="title"><Logo name={trackerLogo()} />{t('My sprint')}</span>
    {#if r}<small>{r.name} · {t('day {day}/{days}', { day: r.day, days: r.days })}</small>{/if}
    {#if conventions().actuals && snap.sprint}
      <span class="tabs" role="group" aria-label={t('My sprint’s view')}>
        <button class:on={!actuals} aria-pressed={!actuals} onclick={() => setTab('retro')} title={t('Your sprint, for the retro')}>{t('Retro')}</button>
        <button class:on={actuals} aria-pressed={actuals} onclick={() => setTab('actuals')} title={t('The sprint’s done tickets: estimate against actual, and the trend by type')}>{t('Actuals')}</button>
      </span>
    {/if}
    <BlockControls id="retro" />
  </h2>
  {#if !snap.sprint}
    <div class="empty">{t('No current sprint read from the tracker.')}</div>
  {:else if actuals}
    {#if !isShort('retro')}<SprintActuals {snap} />{/if}
  {:else if !r}
    <div class="empty">{t('The sprint has no start date: Prism cannot tell its days.')}</div>
  {:else if !snap.tracker_me}
    <div class="empty">{t('Prism does not know you in the tracker yet: your tickets show once it does.')}</div>
  {:else}
    <div class="kpis">
      <div class="kpi" title={t('Your tickets of the sprint done, of those assigned to you')}>
        <span class="l">{t('Delivered')}</span><span class="v">{r.delivered}<small> / {r.mine.length}</small></span><span class="d">{delta(r.delivered, previous?.delivered) || t('tickets')}</span>
      </div>
      <div class="kpi" title={t('Time traced on the sprint’s tickets, of all your traced time and meetings (Day trail)')}>
        <span class="l">{t('On sprint tickets')}</span><span class="v">{r.traced ? `${pct(r.time.sprint)}` : '—'}<small>{r.traced ? ' %' : ''}</small></span><span class="d">{r.traced ? t('{part} of {all}', { part: hours(r.time.sprint), all: hours(all) }) : t('Day trail off')}</span>
      </div>
      <div class="kpi" title={t('Median of each day’s longest stretch without a meeting, between your first and last trace')}>
        <span class="l">{t('Longest focus')}</span><span class="v">{r.daily.length ? hours(focus) : '—'}</span><span class="d">{delta(focus, previous?.focusMedian, ' min') || t('a day, median')}</span>
      </div>
      <div class="kpi" title={t('Your approvals and comments on others’ MRs')}>
        <span class="l">{t('Reviews given')}</span><span class="v">{r.traced ? r.reviews : '—'}</span><span class="d">{delta(r.reviews, previous?.reviews) || t('approvals, comments')}</span>
      </div>
      <div class="kpi" title={t('Agents launched since the sprint started, their cost at the API prices')}>
        <span class="l">{t('Agents')}</span><span class="v">{r.agents}</span><span class="d">${r.cost.toFixed(0)}{r.merged ? ` · ${t('{by}/{merged} MRs', { by: r.byAgents, merged: r.merged })}` : ''}</span>
      </div>
    </div>
    {#if !isShort('retro')}
      <div class="scroll">
        <div class="grid">
          <div class="cell">
            <h3>{t('Where my time went')} <span>{r.traced ? t('{time} traced', { time: hours(all) }) : ''}</span></h3>
            {#if !r.traced}
              <p class="muted">{t('The Day trail is off: turn it on to see your time.')} <button class="link" onclick={() => openSettings('trail')}>{t('Settings › Day trail')}</button></p>
            {:else if !all}
              <p class="muted">{t('No trace yet in this sprint.')}</p>
            {:else}
              <div class="split">
                <i style:width="{pct(r.time.sprint)}%" style:background="var(--info)"></i>
                <i style:width="{pct(r.time.meetings)}%" style:background="var(--violet)"></i>
                <i style:width="{pct(r.time.other)}%" style:background="var(--warn)"></i>
                <i style:width="{pct(r.time.none)}%" style:background="var(--text-muted)"></i>
              </div>
              <div class="legend">
                <i style:background="var(--info)"></i><span>{t('Sprint tickets')}</span><span class="n">{hours(r.time.sprint)}</span><span class="p">{pct(r.time.sprint)} %</span>
                <i style:background="var(--violet)"></i><span>{t('Meetings')}</span><span class="n">{hours(r.time.meetings)}</span><span class="p">{pct(r.time.meetings)} %</span>
                <i style:background="var(--warn)"></i><span>{t('Other tickets')}</span><span class="n">{hours(r.time.other)}</span><span class="p">{pct(r.time.other)} %</span>
                <i style:background="var(--text-muted)"></i><span>{t('No ticket')}</span><span class="n">{hours(r.time.none)}</span><span class="p">{pct(r.time.none)} %</span>
              </div>
              <h3 class="sub">{t('Focus by day')} <span>{t('longest stretch · switches')}</span></h3>
              <div class="days">
                {#each r.daily as d (d.day)}
                  <div class="day" title={t('{day}: longest focus {focus}, {n} switches, {meetings} of meetings', { day: dayTitle(d.day), focus: hours(d.focus), n: d.switches, meetings: hours(d.meetings) })}>
                    <span class="bar"><i style:height="{Math.max(6, (100 * d.focus) / maxFocus)}%"></i></span>
                    <span class="sw" class:many={d.switches >= 8}>{d.switches}</span>
                    <span class="dl">{dayLabel(d.day)}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
          <div class="cell">
            <h3>{t('My tickets')} <span>{t('estimate · time · cycle')}</span></h3>
            {#each r.mine as m (m.t.task_id)}
              {@const spent = Math.max(m.logged, m.traced)}
              <div class="tk" class:carried={!m.done}>
                <button class="id mono" onclick={() => canPreview() && openTicket(ticketKey(m.t))} title={m.t.name}>{m.t.id}</button>
                <span class="nm" title={m.done ? m.t.name : `${m.t.name} · ${t('carried: {why}', { why: m.why ?? '' })}`}>{m.t.name}{#if !m.done}<em> · {m.why}</em>{/if}</span>
                <span class="n">{m.estimate ? hours(m.estimate) : '—'}</span>
                <span class="n" class:over={!!m.estimate && spent > m.estimate * 1.5} title={t('Logged {logged} · traced {traced}', { logged: hours(m.logged), traced: hours(m.traced) })}>{spent ? hours(spent) : '—'}</span>
                <span class="n">{m.cycleDays !== null ? t('{n}d', { n: m.cycleDays }) : m.done ? '—' : ''}</span>
              </div>
            {:else}
              <p class="muted">{t('No ticket of the sprint assigned to you.')}</p>
            {/each}
          </div>
          <div class="cell">
            <h3>{t('Seen by Prism')} <span>{previous ? t('vs {name}', { name: previous.name }) : t('first sprint kept')}</span></h3>
            {#each r.observations as o (o.text)}
              <p class="obs"><span class:good={o.good} class:bad={!o.good}>{o.good ? '▲' : '▼'}</span>{o.text}</p>
            {:else}
              <p class="muted">{t('Nothing stands out yet.')}</p>
            {/each}
          </div>
          <div class="cell">
            <h3>{t('For the retro')}
              <span>
                {#if pointSprints.length > 1}
                  <select class="sprint" value={shown} onchange={(e) => (picked = e.currentTarget.value)} aria-label={t('Sprint of the points')}>{#each pointSprints as s (s)}<option value={s}>{s}{s === r.name ? ` ${t('(current)')}` : ''}</option>{/each}</select>
                {:else}{points.length || ''}{/if}
              </span>
            </h3>
            <form class="add" onsubmit={(e) => { e.preventDefault(); submitPoint() }}>
              <span class="kinds" role="group" aria-label={t('Kind of point')}>
                {#each POINT_KINDS as [k, mark, label] (k)}<button type="button" class="k {k}" class:on={pointKind === k} onclick={() => setKind(k)} title={label} aria-pressed={pointKind === k}>{mark}</button>{/each}
              </span>
              <input bind:value={pointText} placeholder={t('A point for the retro… ↩')} title={t('Kept with {sprint}', { sprint: shown })} spellcheck="true" aria-label={t('New point')} />
            </form>
            {#each points as p (p.id)}
              <div class="pt">
                <button class="mark {p.kind}" onclick={() => updatePoint(shown, p.id, { kind: nextKind(p.kind) })} title="{POINT_LABEL[p.kind]} · {t('click:')} {POINT_LABEL[nextKind(p.kind)].toLowerCase()}">{POINT_KINDS.find(([k]) => k === p.kind)?.[1]}</button>
                {#if editing === p.id}
                  <!-- svelte-ignore a11y_autofocus -->
                  <input class="edit" bind:value={editText} autofocus onblur={() => endEdit(p, true)} onkeydown={(e) => { if (e.key === 'Enter') endEdit(p, true); else if (e.key === 'Escape') { e.stopPropagation(); endEdit(p, false) } }} />
                {:else}
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <span class="tx" ondblclick={() => { editing = p.id; editText = p.text }} title={t('Double-click to edit')}>{p.text}</span>
                {/if}
                <button class="rm" onclick={() => removePoint(shown, p.id)} title={t('Remove this point')} aria-label={t('Remove')}>×</button>
              </div>
            {:else}
              <p class="muted">{t('What to keep, change or try, as it comes during the sprint: it is kept with {sprint}.', { sprint: shown })}</p>
            {/each}
            {#if pointsStorage.failed}<p class="err">{t('Could not save the points.')}</p>{/if}
            {#if r.notes.length}
              <h3 class="sub">{t('From my notes')} <span>#retro</span></h3>
              {#each r.notes as n (n.id)}
                <p class="obs"><span class="muted">·</span>{n.text.replace(/\s*#retro\b/gi, '').trim()}</p>
              {/each}
            {/if}
            <h3 class="sub">{t('Team')} <span>{t('one line, no ranking')}</span></h3>
            <p class="muted">{t('{done} of {total} tickets done.', { done: r.team.done, total: r.team.total })}</p>
          </div>
        </div>
      </div>
      <div class="foot">
        <span class="muted">{t('From the tracker, the Day trail, your time, agents and notes. Stays in Prism.')}</span>
        <button class="btn" onclick={() => copy(md, t('My sprint copied as Markdown'))}><Icon name="copy" />{t('Copy as Markdown')}</button>
        {#if draft && (draft.launch || draft.href)}<a class="btn claude" href={draft.href} onclick={claudeClick(draft)} title={draft.prompt}><Logo name="agent" size={10} />{t('Draft my points')}</a>{/if}
      </div>
    {/if}
  {/if}
</section>

<style>
  .tabs { display: inline-flex; margin-left: 8px; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; text-transform: none; letter-spacing: 0; }
  .tabs button { padding: 0 8px; border: none; background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); font-weight: 400; cursor: pointer; }
  .tabs button + button { border-left: 1px solid var(--border); }
  .tabs button.on { background: var(--surface-muted); color: var(--text); }
  .kpis { display: grid; grid-template-columns: repeat(auto-fit, minmax(110px, 1fr)); border-bottom: 1px solid var(--border); }
  .kpi { display: flex; flex-direction: column; padding: 7px 12px; border-right: 1px solid var(--border); min-width: 0; }
  .kpi:last-child { border-right: none; }
  .kpi .l { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .kpi .v { font-size: 18px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .kpi .v small { font-size: var(--fs-meta); font-weight: 400; color: var(--text-muted); }
  .kpi .d { font: var(--fs-tag) var(--mono); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); }
  .cell { padding: 8px 12px 10px; border-right: 1px solid var(--border); border-bottom: 1px solid var(--border); min-width: 0; }
  h3 { display: flex; gap: 6px; margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 span { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; }
  h3.sub { margin-top: 12px; }
  .split { display: flex; height: 12px; border-radius: var(--r4); overflow: hidden; background: var(--surface-muted); margin-bottom: 6px; }
  .split i { display: block; height: 100%; }
  .legend { display: grid; grid-template-columns: 10px minmax(0, 1fr) auto 40px; gap: 2px 8px; align-items: center; font-size: var(--fs-meta); }
  .legend i { width: 10px; height: 10px; border-radius: 2px; }
  .legend .n { font-family: var(--mono); text-align: right; }
  .legend .p { font: var(--fs-tag) var(--mono); color: var(--text-muted); text-align: right; }
  .days { display: flex; gap: 4px; align-items: flex-end; height: 70px; }
  .day { flex: 1; display: flex; flex-direction: column; align-items: center; gap: 2px; height: 100%; min-width: 12px; }
  .day .bar { flex: 1; width: 100%; display: flex; align-items: flex-end; border-radius: 3px; background: var(--surface-muted); overflow: hidden; }
  .day .bar i { display: block; width: 100%; background: var(--info); }
  .day .sw { font: 10px var(--mono); color: var(--text-muted); }
  .day .sw.many { color: var(--warn); }
  .day .dl { font-size: 10px; color: var(--text-muted); text-transform: uppercase; }
  .tk { display: grid; grid-template-columns: auto minmax(0, 1fr) 48px 52px 40px; gap: 6px; align-items: center; padding: 2px 0; font-size: var(--fs-meta); }
  .tk .id { padding: 0; border: none; background: none; color: var(--info); cursor: pointer; font-size: var(--fs-tag); }
  .tk .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tk.carried .nm { color: var(--text-muted); }
  .tk em { font-style: normal; color: var(--warn); }
  .tk .n { font: var(--fs-tag) var(--mono); text-align: right; color: var(--text-muted); }
  .tk .n.over { color: var(--warn); }
  .obs { display: grid; grid-template-columns: 14px minmax(0, 1fr); gap: 4px; margin: 0 0 5px; font-size: var(--fs-meta); }
  .good { color: var(--ok); } .bad { color: var(--warn); }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); margin: 0; }
  .add { display: flex; gap: 6px; align-items: center; margin-bottom: 6px; }
  .add input { flex: 1; min-width: 0; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .kinds { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .kinds button { padding: 1px 6px; border: none; background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .kinds button + button { border-left: 1px solid var(--border); }
  .kinds button.on { background: var(--surface-muted); }
  .keep { --c: var(--ok); } .change { --c: var(--warn); } .idea { --c: var(--info); }
  .kinds button.on, .mark { color: var(--c); }
  .pt { display: grid; grid-template-columns: 14px minmax(0, 1fr) auto; gap: 4px; align-items: start; margin: 0 0 4px; font-size: var(--fs-meta); }
  .pt .mark, .pt .rm { padding: 0; border: none; background: none; font: inherit; cursor: pointer; }
  .pt .rm { color: var(--text-muted); opacity: 0; }
  .pt:hover .rm, .pt .rm:focus-visible { opacity: 1; }
  .pt .tx { overflow-wrap: anywhere; }
  .pt .edit { padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); font: inherit; background: var(--surface); color: var(--text); }
  .sprint { max-width: 150px; padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); }
  .err { margin: 4px 0 0; color: var(--ko); font-size: var(--fs-tag); }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .foot { display: flex; align-items: center; gap: 6px; padding: 6px 12px; flex-wrap: wrap; }
  .foot .muted { margin-right: auto; font-size: var(--fs-tag); }
  .btn { display: inline-flex; align-items: center; gap: 4px; padding: 1px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; text-decoration: none; }
  .btn.claude { border-color: color-mix(in srgb, var(--src-claude) 60%, var(--border)); }
</style>
