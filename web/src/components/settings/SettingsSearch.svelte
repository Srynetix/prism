<script module lang="ts">
  import { changes, isChanged, originOf, showSettings, valueOf } from '../../settings.svelte'
  import { buildIndex, pageNode, pagesFor, search, shown as showValue, type Entry } from '../../settings-search'
  import { TREE, type TreeNode } from '../../settings-tree'
  import type { Section } from '../../settings-sections'
  import { t, tc, tn } from '../../i18n.svelte'

  export type Filter = 'all' | 'on' | 'changed' | 'file' | 'pack' | 'attention'
  /** What the search asks of the Settings view: a page's switch and state, and showing a page. */
  export interface SearchDeps {
    isOn: (n: TreeNode) => boolean | undefined
    nodeAttention: (n: TreeNode) => boolean
    attention: (s: Section) => boolean
    go: (id: string) => void
  }
  /** Result rows drawn at first, each mounting a control: « Show N more » draws the rest. */
  const FIRST = 50

  /**
   * The search's state, shared by its two parts (the box and filters in the tree's column, the
   * results in the body): the query and the filter, the counts of each filter, the keys listed.
   */
  export class SearchState {
    #deps: SearchDeps
    #query = $state('')
    #filter = $state<Filter>('all')
    /** The row ↑ ↓ chose, and how many rows are drawn: both start again as the query or the filter change. */
    selected = $state(0)
    limit = $state(FIRST)
    input = $state<HTMLInputElement>()
    readonly index = buildIndex()
    readonly pages = TREE.flatMap((r) => r.children ?? [])
    readonly filters: { id: Filter; label: string; title: string }[] = [
      { id: 'all', label: t('All'), title: t('Every page') },
      { id: 'on', label: tc('filter', 'On'), title: t('The features switched on') },
      { id: 'changed', label: t('Not saved'), title: t('Unsaved changes') },
      { id: 'file', label: t('Not default'), title: t('Written in config.toml: your own values') },
      { id: 'pack', label: t('From a pack'), title: t('Given by a pack, not changed in config.toml') },
      { id: 'attention', label: t('Attention'), title: t('A check failed, or a source could not be read') },
    ]

    constructor(deps: SearchDeps) {
      this.#deps = deps
    }

    get query() {
      return this.#query
    }
    set query(v: string) {
      if (v === this.#query) return
      this.#query = v
      this.selected = 0
      this.limit = FIRST
    }
    get filter() {
      return this.#filter
    }
    set filter(v: Filter) {
      if (v === this.#filter) return
      this.#filter = v
      this.selected = 0
      this.limit = FIRST
    }

    /** The filters on keys; `on` and `attention` keep pages. */
    #keyFilter = (f: Filter, e: Entry) => (f === 'changed' ? isChanged(e.path) : f === 'file' ? originOf(e.path) === 'file' : f === 'pack' ? originOf(e.path) === 'pack' : f === 'on' ? this.#deps.isOn(e.page) === true : f === 'attention' ? this.#deps.attention(e.section) : true)
    #pageFilter = (f: Filter, n: TreeNode) => (f === 'on' ? this.#deps.isOn(n) === true : f === 'attention' ? this.#deps.nodeAttention(n) : true)

    /** Keys written in config.toml, and given by a pack: one pass, following the file and the edits alone. */
    #origins = $derived.by(() => {
      let file = 0
      let pack = 0
      for (const e of this.index) {
        const o = originOf(e.path)
        if (o === 'file') file++
        else if (o === 'pack') pack++
      }
      return { file, pack }
    })
    #on = $derived(this.pages.filter((n) => this.#deps.isOn(n) === true).length)
    /** The only count that follows the snapshot (a source in error): the others are not computed again with it. */
    #attention = $derived(this.pages.filter((n) => this.#deps.nodeAttention(n)).length)
    counts = $derived<Record<Filter, number>>({ all: 0, on: this.#on, changed: changes().length, file: this.#origins.file, pack: this.#origins.pack, attention: this.#attention })

    searching = $derived(!!this.#query.trim() || this.#filter !== 'all')
    /** The keys listed: those answering the query, kept by the filter (a page filter alone lists pages). */
    results = $derived.by(() => {
      const q = this.#query.trim()
      const f = this.#filter
      if (!this.searching) return []
      if (!q && (f === 'on' || f === 'attention')) return []
      return (q ? search(this.index, this.#query, (e) => showValue(valueOf(e.path))) : this.index).filter((e) => this.#keyFilter(f, e)).slice(0, 200)
    })
    /** The rows drawn: the first ones, until « Show N more ». */
    shown = $derived(this.results.slice(0, this.limit))
    /** The pages answering the query, or kept by a page filter: chips above the keys. */
    pageChips = $derived(this.searching ? (this.#query.trim() ? pagesFor(this.#query) : this.pages).filter((n) => this.#pageFilter(this.#filter, pageNode(n))) : [])
    /** In the tree while searching: the pages with a key listed, or listed themselves. */
    listedPages = $derived(new Set([...this.results.map((e) => e.page.id), ...this.pageChips.map((n) => pageNode(n).id)]))

    isOn = (n: TreeNode) => this.#deps.isOn(n)
    go = (id: string) => this.#deps.go(id)

    /** Out of the search: the query and the filter cleared. */
    reset() {
      this.query = ''
      this.filter = 'all'
    }
    focus() {
      this.input?.focus()
      this.input?.select()
    }
    open(e: Entry) {
      showSettings((e.provider ?? e.page).id, e.key, e.section.id)
      this.reset()
    }
    onKey(e: KeyboardEvent) {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        const n = this.shown.length
        if (n) this.selected = (this.selected + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
        document.getElementById(`res-${this.selected}`)?.scrollIntoView({ block: 'nearest' })
      } else if (e.key === 'Enter') {
        e.preventDefault()
        const r = this.shown[this.selected]
        if (r) this.open(r)
        else if (this.pageChips[0]) this.#deps.go(this.pageChips[0].id)
      } else if (e.key === 'Escape') {
        // The box's own: it clears, then leaves the field; Settings does not close on it.
        e.preventDefault()
        e.stopPropagation()
        if (this.#query || this.#filter !== 'all') this.reset()
        else this.input?.blur()
      }
    }
  }
</script>

<script lang="ts">
  import Icon from '../Icon.svelte'
  import Control from './Control.svelte'
  import { chainOf } from '../../settings-tree'
  import { packLabel, setValue } from '../../settings.svelte'

  /**
   * The search of the Settings view (settings-search.ts), in two parts sharing one `SearchState`:
   * `box`, the field and the filters, each saying how many, at the top of the tree; `results`, the
   * keys listed, each changed in place (↵ opens its page on it), and the pages answering as chips.
   */
  let {
    search: s,
    part,
    showToml = false,
    ontoml,
  }: {
    search: SearchState
    part: 'box' | 'results'
    showToml?: boolean
    ontoml?: () => void
  } = $props()
</script>

{#if part === 'box'}
  <label class="search"><Icon name="search" /><input bind:this={s.input} bind:value={s.query} onkeydown={(e) => s.onKey(e)} placeholder={t('Search a setting, a key, a value…')} spellcheck="false" aria-label={t('Search the settings')} />{#if s.query}<button class="clear" onclick={() => (s.query = '')} aria-label={t('Clear the search')}><Icon name="close" /></button>{:else}<kbd>/</kbd>{/if}</label>
  <div class="filters">
    {#each s.filters as f (f.id)}<button class:on={s.filter === f.id} onclick={() => (s.filter = s.filter === f.id && f.id !== 'all' ? 'all' : f.id)} title={f.title}>{f.label}{#if f.id !== 'all'}<span class="n" class:hot={s.counts[f.id] > 0}>{s.counts[f.id]}</span>{/if}</button>{/each}
  </div>
{:else}
  <!-- The search: keys, each changed in place; ↵ opens its page on it. -->
  <div class="rhead">
    <b>{s.query.trim() || s.filter === 'changed' || s.filter === 'file' || s.filter === 'pack' ? tn(s.results.length, '{n} setting', '{n} settings') : s.filters.find((f) => f.id === s.filter)?.title}</b>
    {#if s.filter !== 'all'}<span class="path">{s.filters.find((f) => f.id === s.filter)?.label}</span>{/if}
    <span class="grow"></span>
    <span class="hint">{t('↑ ↓ to choose, ↵ to open its page, Esc to clear')}</span>
    <button class="btn sm ghost" onclick={ontoml} aria-pressed={showToml} title={t('Show config.toml next to the settings')}>TOML</button>
  </div>
  {#if s.pageChips.length}
    <div class="chips">
      {#each s.pageChips.slice(0, 16) as n (n.id)}
        {@const at = pageNode(n)}
        {@const on = s.isOn(at)}
        <button class="chip" class:off={on === false} onclick={() => s.go(n.id)}><span class="sw" style:background={chainOf(at)[0].color}></span>{chainOf(at)[0].label} › {n === at ? n.label : `${at.label} › ${n.label}`}{#if on !== undefined}<span class="dot" class:ok={on}></span>{/if}</button>
      {/each}
    </div>
  {/if}
  <div class="results">
    {#each s.shown as e, i (e.id)}
      {@const o = originOf(e.path)}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="res" class:sel={i === s.selected} id="res-{i}" onmouseenter={() => (s.selected = i)}>
        <button class="rl" onclick={() => s.open(e)} title={t('Open its page on it (↵)')}>
          <span class="rc">{e.crumb.join(' › ')}</span>
          <b>{e.field.label}</b>
          <span class="key">{e.key}</span>
          {#if e.field.hint}<small>{e.field.hint}</small>{/if}
        </button>
        <div class="rv">
          {#if e.field.kind === 'toggle' || e.field.kind === 'select' || e.field.kind === 'number' || e.field.kind === 'text'}
            <Control kind={e.field.kind} value={valueOf(e.path)} onchange={(v) => setValue(e.path, v)} options={e.field.options} labels={e.field.labels} optional={e.field.optional} placeholder={e.field.placeholder} unit={e.field.unit} step={e.field.step} compact />
          {:else}<span class="val mono" title={showValue(valueOf(e.path))}>{showValue(valueOf(e.path))}</span>{/if}
          <span class="origin {o}">{o === 'changed' ? t('not saved') : o === 'file' ? 'config.toml' : o === 'pack' ? packLabel(e.path) : tc('settings', 'default')}</span>
        </div>
      </div>
    {:else}
      {#if !s.pageChips.length}<p class="hint none">{t('No setting matches. Try another word, its TOML key, or a table: [forge].')}</p>{/if}
    {/each}
  </div>
  {#if s.results.length > s.shown.length}
    <button class="btn sm ghost more" onclick={() => (s.limit = s.results.length)}>{t('Show {n} more', { n: s.results.length - s.shown.length })}</button>
  {/if}
{/if}

<style>
  /* The box and its filters, at the top of the tree. */
  .search { display: flex; align-items: center; gap: 6px; height: 28px; margin: 0 2px 8px; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text-muted); }
  .search input { flex: 1; min-width: 0; border: none; outline: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); }
  .search kbd { padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r4); color: var(--text-muted); font: var(--fs-tag) var(--mono); }
  .search .clear { display: inline-flex; padding: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .filters { display: flex; flex-wrap: wrap; gap: 4px; margin: 0 2px 6px; }
  .filters button { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px; border: 1px solid var(--border); border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .filters button.on { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); border-color: var(--accent); font-weight: 600; }
  .filters .n { font-family: var(--mono); opacity: 0.6; }
  .filters .n.hot { opacity: 1; color: var(--text); }
  /* Search results: a key per row, its value changed in place. */
  .rhead { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  .rhead b { font-size: var(--fs-title); }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 12px; }
  .chip { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .chip:hover { border-color: var(--accent); }
  .chip.off { color: var(--text-muted); }
  .results { display: flex; flex-direction: column; margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); overflow: hidden; flex-shrink: 0; }
  .results:empty { display: none; }
  .res { display: grid; grid-template-columns: minmax(0, 1fr) minmax(200px, 40%); gap: 14px; align-items: center; padding: 8px 14px; border-bottom: 1px solid var(--border); }
  .res:last-child { border-bottom: none; }
  .res.sel { background: color-mix(in srgb, var(--accent) 9%, transparent); box-shadow: inset 3px 0 0 var(--accent); }
  .rl { display: flex; flex-direction: column; gap: 1px; min-width: 0; padding: 0; border: none; background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .rl b { font-weight: 550; font-size: var(--fs-body); }
  .rl small { color: var(--text-muted); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rc { color: var(--text-muted); font-size: var(--fs-tag); }
  .key { font: var(--fs-tag) var(--mono); color: var(--text-muted); opacity: 0.8; }
  .rv { display: flex; align-items: center; justify-content: flex-end; gap: 8px; min-width: 0; }
  .val { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
  .origin { flex-shrink: 0; padding: 0 6px; border-radius: var(--r9); font-size: var(--fs-tag); line-height: 16px; background: var(--surface-muted); color: var(--text-muted); }
  .origin.file { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); font-family: var(--mono); }
  .origin.pack { background: var(--info-bg); color: var(--info); }
  .origin.changed { background: color-mix(in srgb, var(--warn) 16%, transparent); color: var(--warn); }
  .more { align-self: flex-start; margin: -6px 0 14px; flex-shrink: 0; }
  @container (max-width: 700px) { .res { grid-template-columns: minmax(0, 1fr); } .rv { justify-content: flex-start; } }
  .sw { width: 8px; height: 8px; flex-shrink: 0; border-radius: var(--r2); }
  .path { padding: 1px 7px; border-radius: var(--r5); background: var(--surface-muted); color: var(--text-muted); font: var(--fs-tag) var(--mono); }
  .grow { flex: 1; }
  .hint { font-size: var(--fs-meta); color: var(--text-muted); }
  .none { margin: 8px; }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .btn[aria-pressed='true'] { color: var(--accent); border-color: var(--accent); }
</style>
