<script lang="ts">
  import Markdown from '../../components/Markdown.svelte'
  import { openLink } from '../../links'
  import { pref, setPref } from '../../layout.svelte'
  import { t } from '../../i18n.svelte'
  import { runAction } from './actions'
  import { field, tone, type CustomBlockView, type CustomItem } from './types'
  import type { CustomStat } from '../../bindings/CustomStat'

  /**
   * What a custom block printed, by its kind. Every text comes from a program: shown as text,
   * markdown through the Chat's renderer (no HTML, no `prism:` link), links http(s) only and in
   * the browser. `short`: a few lines. `acts`: its action buttons (not in a preview).
   */
  let { block, short = false, acts = true }: { block: CustomBlockView; short?: boolean; acts?: boolean } = $props()

  const body = $derived(block.output?.body)
  const actionsOf = (i: CustomItem) => block.actions.filter((a) => i.actions.includes(a.id))
  const LIMIT = 5

  /** A table's sort, kept per block (shared prefs: the phone sorts the same). */
  type Sort = { key: string; desc: boolean } | null
  const sortKey = $derived(`customSort:${block.id}`)
  const sort = $derived(pref<Sort>(sortKey, null))
  let filter = $state('')
  function sortBy(key: string) {
    setPref(sortKey, sort?.key === key ? (sort.desc ? null : { key, desc: true }) : { key, desc: false })
  }
  const rows = $derived.by(() => {
    if (body?.kind !== 'table') return []
    const q = filter.trim().toLowerCase()
    const list = q ? body.rows.filter((r) => Object.values(r.fields).some((v) => String(v ?? '').toLowerCase().includes(q))) : [...body.rows]
    if (sort) {
      const k = sort.key
      list.sort((a, b) => {
        const x = a.fields[k], y = b.fields[k]
        const c = typeof x === 'number' && typeof y === 'number' ? x - y : String(x ?? '').localeCompare(String(y ?? ''), undefined, { numeric: true })
        return sort.desc ? -c : c
      })
    }
    return list
  })

  /** A stat's share of its range, for its bar. */
  const share = (s: CustomStat) => (s.max != null ? Math.max(0, Math.min(1, (s.value - (s.min ?? 0)) / (s.max - (s.min ?? 0) || 1))) : null)
  const num = (n: number) => (Number.isInteger(n) ? n.toLocaleString() : n.toLocaleString(undefined, { maximumFractionDigits: 2 }))
  /** The points of a sparkline 100×24. */
  function spark(values: number[]): string {
    if (values.length < 2) return ''
    const lo = Math.min(...values), hi = Math.max(...values)
    return values.map((v, i) => `${((i / (values.length - 1)) * 100).toFixed(1)},${(22 - ((v - lo) / (hi - lo || 1)) * 20).toFixed(1)}`).join(' ')
  }
</script>

{#snippet buttons(i: CustomItem)}
  {#if acts && actionsOf(i).length}
    <span class="acts">
      {#each actionsOf(i) as a (a.id)}
        <button class="act" class:agent={a.kind === 'agent'} onclick={(e) => { e.stopPropagation(); void runAction(block.id, a.id, a.label, i) }} title={a.kind === 'agent' ? t('Opens the launcher with this line') : t('Runs {action} on the computer', { action: a.label })}>{a.label}</button>
      {/each}
    </span>
  {/if}
{/snippet}

{#if !body}
  <!-- Nothing read yet. -->
{:else if body.kind === 'list'}
  <ul class="list">
    {#each short ? body.items.slice(0, LIMIT) : body.items as i, n (`${n}:${i.key}`)}
      <li class="row">
        <span class="dot {tone(i.state)}"></span>
        <span class="txt">
          {#if i.url}<button class="title link" onclick={() => openLink(i.url!)} title={i.url}>{field(i, 'title') || i.key}</button>{:else}<span class="title">{field(i, 'title') || i.key}</span>{/if}
          {#if field(i, 'sub')}<span class="sub">{field(i, 'sub')}</span>{/if}
        </span>
        {#if field(i, 'badge')}<span class="badge {tone(i.state)}">{field(i, 'badge')}</span>{/if}
        {@render buttons(i)}
      </li>
    {:else}
      <li class="empty">{t('Nothing to show.')}</li>
    {/each}
    {#if short && body.items.length > LIMIT}<li class="more">{t('+{n} more', { n: body.items.length - LIMIT })}</li>{/if}
  </ul>
{:else if body.kind === 'stats'}
  <div class="stats" class:one={body.stats.length === 1}>
    {#each body.stats as s, n (`${n}:${s.key}`)}
      {@const p = share(s)}
      {@const h = block.history[s.key] ?? []}
      <div class="stat {tone(s.state)}">
        {#if s.label}<span class="label">{s.label}</span>{/if}
        <span class="value">{num(s.value)}{#if s.unit}<small>{s.unit}</small>{/if}{#if s.max != null && s.unit !== '%'}<small class="of">/ {num(s.max)}</small>{/if}</span>
        {#if p !== null}<span class="bar"><i style="width: {p * 100}%"></i></span>{/if}
        {#if h.length > 1}<svg class="spark" viewBox="0 0 100 24" preserveAspectRatio="none" aria-hidden="true"><polyline points={spark(h)} /></svg>{/if}
      </div>
    {/each}
  </div>
{:else if body.kind === 'markdown'}
  <div class="md"><Markdown text={body.text} mrs /></div>
{:else if body.kind === 'table'}
  {#if body.rows.length > 8 && !short}<input class="filter" placeholder={t('Filter')} bind:value={filter} aria-label={t('Filter the rows')} />{/if}
  <div class="tablewrap">
    <table>
      <thead>
        <tr>
          <th class="st"></th>
          {#each body.columns as c, n (`${n}:${c.key}`)}<th aria-sort={sort?.key === c.key ? (sort.desc ? 'descending' : 'ascending') : 'none'}><button onclick={() => sortBy(c.key)}>{c.label}{#if sort?.key === c.key}<span class="arrow">{sort.desc ? '▾' : '▴'}</span>{/if}</button></th>{/each}
          {#if acts && block.actions.length}<th></th>{/if}
        </tr>
      </thead>
      <tbody>
        {#each short ? rows.slice(0, LIMIT) : rows as r, n (`${n}:${r.key}`)}
          <tr>
            <td class="st"><span class="dot {tone(r.state)}"></span></td>
            {#each body.columns as c, ci (`${ci}:${c.key}`)}
              <td class:num={typeof r.fields[c.key] === 'number'}>{#if ci === 0 && r.url}<button class="link" onclick={() => openLink(r.url!)} title={r.url}>{field(r, c.key)}</button>{:else}{field(r, c.key)}{/if}</td>
            {/each}
            {#if acts && block.actions.length}<td>{@render buttons(r)}</td>{/if}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  <pre class="raw">{body.text || t('(nothing printed)')}</pre>
{/if}

<style>
  .list { margin: 0; padding: 0; list-style: none; }
  .row { display: flex; align-items: center; gap: 8px; padding: 5px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); }
  .row:first-child { border-top: none; }
  .dot { flex-shrink: 0; width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); opacity: 0.4; }
  .dot.ok { background: var(--ok); opacity: 1; }
  .dot.info { background: var(--info); opacity: 1; }
  .dot.warn { background: var(--warn); opacity: 1; }
  .dot.ko { background: var(--ko); opacity: 1; box-shadow: 0 0 0 3px color-mix(in srgb, var(--ko) 20%, transparent); }
  .txt { display: flex; flex-direction: column; flex: 1; min-width: 0; }
  .title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .link { padding: 0; border: none; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .link:hover { color: var(--info); text-decoration: underline; }
  .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: var(--fs-meta); }
  .badge { flex-shrink: 0; padding: 0 6px; border-radius: 9px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }
  .badge.warn { background: var(--warn-bg); color: var(--warn); }
  .badge.ko { background: var(--ko-bg); color: var(--ko); }
  .badge.ok { color: var(--ok); }
  .acts { display: inline-flex; gap: 4px; flex-shrink: 0; }
  .act { padding: 1px 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .act:hover { border-color: var(--info); color: var(--info); }
  .act.agent { border-color: color-mix(in srgb, var(--src-claude) 40%, var(--border)); }
  .empty, .more { padding: 8px 12px; color: var(--text-muted); font-size: var(--fs-meta); }
  .stats { display: grid; grid-template-columns: repeat(auto-fill, minmax(130px, 1fr)); gap: 8px; padding: 10px 12px; }
  .stat { display: flex; flex-direction: column; gap: 3px; padding: 8px 10px; border: 1px solid var(--border); border-left: 3px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); min-width: 0; }
  .stat.ok { border-left-color: var(--ok); }
  .stat.warn { border-left-color: var(--warn); }
  .stat.ko { border-left-color: var(--ko); }
  .stats.one .stat { align-items: flex-start; }
  .stats.one .value { font-size: 28px; }
  .label { color: var(--text-muted); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .value { font-size: 20px; font-weight: 600; font-variant-numeric: tabular-nums; line-height: 1.15; }
  .value small { margin-left: 2px; font-size: 11px; font-weight: 500; color: var(--text-muted); }
  .value .of { margin-left: 4px; }
  .stat.warn .value { color: var(--warn); }
  .stat.ko .value { color: var(--ko); }
  .bar { display: block; width: 100%; height: 4px; border-radius: 2px; background: var(--border); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--info); }
  .stat.ok .bar i { background: var(--ok); }
  .stat.warn .bar i { background: var(--warn); }
  .stat.ko .bar i { background: var(--ko); }
  .spark { width: 100%; height: 22px; }
  .spark polyline { fill: none; stroke: var(--text-muted); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .md { padding: 4px 12px 10px; }
  .filter { margin: 6px 12px 0; width: calc(100% - 24px); padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .tablewrap { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-size: var(--fs-body); }
  th { position: sticky; top: 0; background: var(--surface); text-align: left; font-weight: 500; color: var(--text-muted); font-size: var(--fs-meta); border-bottom: 1px solid var(--border); }
  th button { padding: 5px 6px; border: none; background: none; color: inherit; font: inherit; cursor: pointer; white-space: nowrap; }
  th button:hover { color: var(--text); }
  .arrow { margin-left: 3px; }
  td { padding: 4px 6px; border-top: 1px solid var(--border); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 260px; }
  td.num { text-align: right; font-variant-numeric: tabular-nums; }
  .st { width: 18px; padding-left: 12px; }
  .raw { margin: 0; padding: 8px 12px; font-family: var(--font-mono); font-size: var(--fs-meta); white-space: pre-wrap; word-break: break-word; color: var(--text); }
</style>
