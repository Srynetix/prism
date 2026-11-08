<script lang="ts">
  import { baseName } from '../platform'
  import { t, tc, tn } from '../i18n.svelte'
  import { untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import { conventions, isDone } from '../conventions'
  import { pref, setPref } from '../layout.svelte'
  import { hours } from '../retro'
  import { actualRows, actualsCsv, agentCost, agentMinutes, agentsByTicket, pct, tierOf, tone, totals, trends, unlinked, type ActualRow, type Trend } from '../actuals'
  import { canPreview, openTicket } from '../ticket-view.svelte'
  import { invoke, isDesktop } from '../bridge'
  import { toast } from '../toast.svelte'
  import type { Snapshot } from '../types'
  import { useAgentCosts } from '../agent-costs.svelte'

  /**
   * My sprint's Actuals (`[tracker] actuals`, see actuals.ts): the sprint's done tickets with their
   * estimate, time logged, agents and gap; the totals; what the last three sprints say by type and by
   * estimate. Each sprint's rows are kept in the layout's prefs when shown, for the sprints after it
   * (the tracker reads the current sprint only). The table exports as CSV.
   */
  let { snap }: { snap: Snapshot } = $props()

  const HISTORY = 'actuals.history'
  /** Sprints kept: the last three, and a quarter's. */
  const KEEP = 8
  const QUARTER = 92 * 86_400_000
  /** Rows shown before "n more". */
  const SHOWN = 8
  interface Kept {
    name: string
    end: string | null
    rows: ActualRow[]
  }
  let history = $state<Kept[]>(pref<Kept[]>(HISTORY, []))
  type Range = 'sprint' | 'last3' | 'quarter'
  let range = $state<Range>(pref<Range>('actuals.range', 'sprint'))
  const setRange = (r: Range) => setPref('actuals.range', (range = r))
  let all = $state(false)

  const sprint = $derived(snap.sprint)
  const costs = useAgentCosts()
  const agents = $derived(agentsByTicket(costs.list, snap.merge_requests))
  const opts = $derived({ loggedOnly: conventions().actuals_logged_only, typeSource: conventions().ticket_type, sprint: sprint?.name ?? '' })
  const current = $derived(sprint ? actualRows(sprint.tickets.filter(isDone), agents, snap.time.entries, opts) : [])

  // This sprint's rows, kept for the next sprints to compare with (the last state seen).
  $effect(() => {
    if (!sprint) return
    const kept: Kept = { name: sprint.name, end: sprint.end, rows: current }
    const old = untrack(() => history).find((h) => h.name === kept.name)
    if (old && JSON.stringify(old) === JSON.stringify(kept)) return
    history = [...untrack(() => history).filter((h) => h.name !== kept.name), kept].slice(-KEEP)
    setPref(HISTORY, history)
  })

  /** The sprints of a range, the current one last: its rows as they are now, the others as kept. */
  function sprints(r: Range): Kept[] {
    const past = history.filter((h) => h.name !== sprint?.name)
    const now = sprint ? [{ name: sprint.name, end: sprint.end, rows: current }] : []
    if (r === 'sprint') return now
    if (r === 'last3') return [...past.slice(-2), ...now]
    const from = Date.now() - QUARTER
    return [...past.filter((h) => h.end && Date.parse(h.end) >= from), ...now]
  }
  const shownSprints = $derived(sprints(range))
  const rows = $derived(shownSprints.flatMap((h) => h.rows).sort((a, b) => (b.gap ?? -Infinity) - (a.gap ?? -Infinity)))
  const sum = $derived(totals(rows))
  const learn = $derived(trends(sprints('last3').flatMap((h) => h.rows)))
  /** The agents of no ticket over the range's dates: this sprint's, or as many sprints back as it lasts. */
  const loose = $derived.by(() => {
    const start = sprint?.start ? Date.parse(sprint.start) : Date.now()
    const length = Math.max(86_400_000, (sprint?.end ? Date.parse(sprint.end) : Date.now()) - start)
    const from = range === 'sprint' ? start : range === 'last3' ? start - 2 * length : Date.now() - QUARTER
    return unlinked(agents, from, Date.now())
  })
  const money = (n: number | null) => (n === null ? '—' : `$${n.toFixed(2)}`)
  const ratio = (r: ActualRow) => (r.gap === null ? (r.estimate ? '' : t('not estimated')) : pct(r.gap))

  /** What a trend says, in a sentence. */
  function says(x: Trend): string {
    const what = x.tier !== null ? t('{type} at {tier}', { type: x.type ?? t('untyped'), tier: hours(x.tier) }) : (x.type ?? t('untyped'))
    const base = { what, actual: hours(x.medianActual), gap: pct(x.medianGap), n: x.n }
    if (tone(x.medianGap) === 'ok') return tn(x.n, '{what}: well estimated ({gap}, {n} ticket).', '{what}: well estimated ({gap}, {n} tickets).', base)
    if (x.medianGap > 0 && x.tier !== null) return tn(x.n, '{what}: median actual {actual} ({gap}, {n} ticket). The tier above, {above}, would be closer.', '{what}: median actual {actual} ({gap}, {n} tickets). The tier above, {above}, would be closer.', { ...base, above: hours(tierOf(x.medianActual)) })
    return tn(x.n, '{what}: median actual {actual} ({gap}, {n} ticket).', '{what}: median actual {actual} ({gap}, {n} tickets).', base)
  }
  const lessons = $derived([...learn.byTier.slice(0, 4), ...learn.byType.filter((x) => !learn.byTier.some((y) => y.type === x.type && y.n === x.n)).slice(0, 2)])

  async function exportCsv() {
    const header = [t('Sprint'), t('Ticket'), t('Name'), t('Type'), t('Estimate (h)'), t('Logged (h)'), t('Agents (h)'), t('Agents ($)'), t('Actual (h)'), t('Gap (%)')]
    const text = actualsCsv(rows, header)
    const file = `${t('actuals')}-${(sprint?.name ?? 'sprint').replace(/[^\p{L}\p{N}-]+/gu, '-').replace(/-+$/, '')}.csv`
    try {
      if (isDesktop()) {
        const [{ save }, { downloadDir, join }] = await Promise.all([import('@tauri-apps/plugin-dialog'), import('@tauri-apps/api/path')])
        const dest = await save({ defaultPath: await join(await downloadDir(), file), filters: [{ name: 'CSV', extensions: ['csv'] }] })
        if (!dest) return
        await invoke('save_csv', { dest, text })
        toast(t('Saved: {name}', { name: baseName(dest) || file }))
        return
      }
      const url = URL.createObjectURL(new Blob([text], { type: 'text/csv' }))
      const link = Object.assign(document.createElement('a'), { href: url, download: file })
      document.body.append(link)
      link.click()
      link.remove()
      setTimeout(() => URL.revokeObjectURL(url), 10_000)
    } catch (e) {
      toast(t('Could not export: {error}', { error: String(e) }), 5000)
    }
  }
</script>

<div class="actuals">
  <div class="bar">
    <span class="muted">{tn(rows.length, '{n} done ticket', '{n} done tickets')}</span>
    <span class="seg" role="group" aria-label={t('Sprints shown')}>
      <button class:on={range === 'sprint'} aria-pressed={range === 'sprint'} onclick={() => setRange('sprint')}>{sprint?.name ?? t('This sprint')}</button>
      <button class:on={range === 'last3'} aria-pressed={range === 'last3'} onclick={() => setRange('last3')} title={t('This sprint and the two before, as kept when My sprint showed them')}>{t('Last 3')}</button>
      <button class:on={range === 'quarter'} aria-pressed={range === 'quarter'} onclick={() => setRange('quarter')} title={t('The sprints kept that ended in the last three months')}>{t('Quarter')}</button>
    </span>
    <button class="btn" onclick={exportCsv} disabled={!rows.length} title={t('The table as CSV: durations in hours, the gap in percent')}><Icon name="download" />{t('Export CSV')}</button>
  </div>
  {#if !rows.length}
    <p class="muted pad">{t('No done ticket yet in this sprint.')}</p>
  {:else}
    <div class="scroll">
      <table>
        <thead>
          <tr>
            <th>{t('Ticket')}</th><th>{t('Type')}</th><th class="r">{tc('noun', 'Estimate')}</th><th class="r">{t('Logged')}</th><th class="r">{t('Agents')}</th><th class="r">{tc('actuals', 'Gap')}</th>
          </tr>
        </thead>
        <tbody>
          {#each all ? rows : rows.slice(0, SHOWN) as r (r.sprint + r.id)}
            <tr>
              <td class="tk"><button class="id mono" onclick={() => canPreview() && openTicket(r.id)} title={range === 'sprint' ? r.name : `${r.sprint} · ${r.name}`}>{r.id}</button> <span class="nm" title={r.name}>{r.name}</span></td>
              <td>{#if r.type}<span class="tag">{r.type}</span>{/if}</td>
              <td class="r">{r.estimate ? hours(r.estimate) : '—'}</td>
              <td class="r">{r.logged ? hours(r.logged) : '—'}</td>
              <td class="r">{#if r.agentMinutes || r.agentCost !== null}{hours(r.agentMinutes)}{#if r.agentCost !== null}{' '}· {money(r.agentCost)}{/if}{:else}—{/if}</td>
              <td class="r"><span class="ratio {tone(r.gap)}">{ratio(r)}</span></td>
            </tr>
          {/each}
          {#if loose.length}
            <tr class="loose" title={t('Agents linked to no ticket: neither by hand, nor by their title, branch or MR')}>
              <td class="tk"><span class="nm">{t('Unlinked agents')}</span></td>
              <td></td><td class="r"></td><td class="r"></td>
              <td class="r">{hours(agentMinutes(loose))}{#if agentCost(loose) !== null}{' '}· {money(agentCost(loose))}{/if}</td>
              <td class="r muted">{tn(loose.length, '{n} session', '{n} sessions')}</td>
            </tr>
          {/if}
        </tbody>
      </table>
      {#if rows.length > SHOWN}<button class="link more" onclick={() => (all = !all)}>{all ? t('Show fewer') : tn(rows.length - SHOWN, '… {n} other', '… {n} others')}</button>{/if}
    </div>
    <div class="sum">
      <div class="card"><small>{t('Estimated (estimated tickets)')}</small><b>{hours(sum.estimated)}</b><small>{t('their actual: {actual}', { actual: hours(sum.estimatedActual) })}</small></div>
      <div class="card"><small>{t('Actual')}</small><b>{hours(sum.actual)}</b><small>{conventions().actuals_logged_only ? t('time logged only') : t('of which agents {time}', { time: hours(sum.agentMinutes) })}</small></div>
      <div class="card"><small>{t('Agent cost')}</small><b>{money(sum.agentCost)}</b><small>{sum.perTicket !== null ? t('{cost} a ticket on average', { cost: money(sum.perTicket) }) : t('no price known')}</small></div>
      <div class="card"><small>{t('Median gap')}</small><b class="ratio {tone(sum.medianGap)}">{sum.medianGap === null ? '—' : pct(sum.medianGap)}</b><small>{t('of the estimated tickets')}</small></div>
    </div>
  {/if}
  <div class="learn">
    <h3>{t('What the last 3 sprints say')}</h3>
    {#each lessons as x (`${x.type}|${x.tier}`)}
      <p class="obs"><span class={tone(x.medianGap)}>{tone(x.medianGap) === 'ok' ? '●' : x.medianGap > 0 ? '▲' : '▼'}</span>{says(x)}</p>
    {:else}
      <p class="muted">{t('Not enough estimated tickets of a same type yet: two or more make a trend.')}</p>
    {/each}
  </div>
</div>

<style>
  .actuals { display: flex; flex-direction: column; min-height: 0; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .bar .muted { margin-right: auto; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 1px 8px; border: none; background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: var(--surface-muted); color: var(--text); }
  .btn { display: inline-flex; align-items: center; gap: 4px; padding: 1px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .scroll { overflow: auto; }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-meta); }
  th { text-align: left; padding: 5px 12px; color: var(--text-muted); font-weight: 500; font-size: var(--fs-tag); border-bottom: 1px solid var(--border); white-space: nowrap; }
  td { padding: 4px 12px; border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .r { text-align: right; }
  td.r { font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .tk { display: flex; gap: 6px; align-items: baseline; max-width: 340px; min-width: 0; }
  .tk .nm { overflow: hidden; text-overflow: ellipsis; }
  .id { padding: 0; border: none; background: none; color: var(--info); cursor: pointer; font-size: var(--fs-tag); }
  .mono { font-family: var(--mono); }
  .tag { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r8); color: var(--text-muted); font-size: var(--fs-tag); }
  .ratio { display: inline-block; min-width: 46px; padding: 0 6px; border-radius: var(--r8); text-align: center; font-weight: 600; }
  .ratio.ok { color: var(--ok); background: color-mix(in srgb, var(--ok) 14%, transparent); }
  .ratio.warn { color: var(--warn); background: color-mix(in srgb, var(--warn) 14%, transparent); }
  .ratio.ko { color: var(--ko); background: color-mix(in srgb, var(--ko) 14%, transparent); }
  .ratio.none { color: var(--text-muted); font-weight: 400; }
  .loose td { color: var(--text-muted); font-style: italic; }
  .more { margin: 4px 12px; }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .sum { display: flex; gap: 8px; padding: 8px 12px; border-top: 1px solid var(--border); flex-wrap: wrap; }
  .card { flex: 1; min-width: 130px; padding: 6px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: color-mix(in srgb, var(--surface-muted) 50%, transparent); }
  .card small { display: block; color: var(--text-muted); font-size: var(--fs-tag); }
  .card b { display: inline-block; font-size: 16px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .card b.ratio { min-width: 0; padding: 0 6px; }
  .learn { padding: 8px 12px 10px; border-top: 1px solid var(--border); }
  h3 { margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .obs { display: grid; grid-template-columns: 14px minmax(0, 1fr); gap: 4px; margin: 0 0 5px; font-size: var(--fs-meta); }
  .obs .ok { color: var(--ok); } .obs .warn { color: var(--warn); } .obs .ko { color: var(--ko); }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); margin: 0; }
  .pad { padding: 8px 12px; }
</style>
