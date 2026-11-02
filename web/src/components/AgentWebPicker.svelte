<script lang="ts">
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'
  import { pref, setPref } from '../layout.svelte'
  import { ago } from '../time'
  import type { AgentArtifact, AgentPage } from '../agents.svelte'
  import { t, tc } from '../i18n.svelte'

  /**
   * The Web tab's picker: what it can show, from the agent's folder (its pages, documents and
   * images, as a tree) or from the scratchpad of its session (the artifacts, newest first). A
   * search narrows both to a flat list. ↑ ↓ move, → ← unfold or fold, Enter opens, Esc closes.
   * `onpick` gets a path of the folder, or `scratchpad: true` for an artifact. `ignored`: the
   * folder's list includes the files `.gitignore` leaves out (shown dimmed), `onignored` turns it.
   */
  let {
    pages, artifacts, ignored, onignored, cli, onpick, onclose,
  }: {
    pages: AgentPage[] | null
    artifacts: AgentArtifact[] | null
    ignored: boolean
    onignored: (on: boolean) => void
    /** The agent's CLI name, for the artifacts' wording. */
    cli: string
    onpick: (path: string, scratchpad: boolean) => void
    onclose: () => void
  } = $props()

  type Source = 'folder' | 'scratchpad'
  // An artifact newer than an hour: the scratchpad comes first, Claude has just written it.
  // svelte-ignore state_referenced_locally
  const fresh = (artifacts ?? []).some((a) => a.modified && Date.now() - new Date(a.modified).getTime() < 3_600_000)
  let source = $state<Source>(fresh ? 'scratchpad' : pref<Source>('agents.picker', 'folder'))
  const setSource = (s: Source) => {
    setPref('agents.picker', (source = s))
    cursor = 0
  }
  let query = $state('')

  const ICON: [RegExp, IconName][] = [[/\.(png|jpe?g|gif|webp|svg)$/i, 'image'], [/\.(md|markdown|txt)$/i, 'logs'], [/\.pdf$/i, 'files']]
  const iconOf = (p: string): IconName => ICON.find(([re]) => re.test(p))?.[1] ?? 'web'

  /** Every word of the search, in the path, in any order. */
  const terms = $derived(query.trim().toLowerCase().split(/\s+/).filter(Boolean))
  const hit = (p: string) => terms.every((term) => p.toLowerCase().includes(term))

  /* ---------- The folder, as a tree ---------- */

  interface Dir {
    name: string
    path: string
    dirs: Map<string, Dir>
    files: string[]
  }
  type Row = { key: string; path: string; name: string; depth: number; dir: boolean; when?: string | null; ignored?: boolean }
  const isIgnored = $derived(new Set((pages ?? []).filter((p) => p.ignored).map((p) => p.path)))
  const paths = $derived((pages ?? []).map((p) => p.path))
  const tree = $derived.by(() => {
    const root: Dir = { name: '', path: '', dirs: new Map(), files: [] }
    for (const f of paths) {
      let d = root
      for (const p of f.split('/').slice(0, -1)) {
        let c = d.dirs.get(p)
        if (!c) d.dirs.set(p, (c = { name: p, path: `${d.path}${p}/`, dirs: new Map(), files: [] }))
        d = c
      }
      d.files.push(f)
    }
    return root
  })
  /** Unfolded folders; the top level starts unfolded when it is small. */
  let open = $state(new Set<string>())
  $effect(() => {
    if (tree.dirs.size <= 3 && !open.size) open = new Set([...tree.dirs.values()].map((d) => d.path))
  })
  function fold(path: string, on?: boolean) {
    const next = new Set(open)
    if (on ?? !next.has(path)) next.add(path)
    else next.delete(path)
    open = next
  }

  const rows = $derived.by((): Row[] => {
    if (source === 'scratchpad') return (artifacts ?? []).filter((a) => hit(a.path)).map((a) => ({ key: `s:${a.path}`, path: a.path, name: a.path, depth: 0, dir: false, when: a.modified }))
    // Searching: the matches, flat, their folder shown.
    if (terms.length) return paths.filter(hit).slice(0, 400).map((p) => ({ key: `f:${p}`, path: p, name: p, depth: 0, dir: false, ignored: isIgnored.has(p) }))
    const out: Row[] = []
    const walk = (d: Dir, depth: number) => {
      for (const c of [...d.dirs.values()].sort((a, b) => a.name.localeCompare(b.name))) {
        out.push({ key: `d:${c.path}`, path: c.path, name: c.name, depth, dir: true })
        if (open.has(c.path)) walk(c, depth + 1)
      }
      for (const f of [...d.files].sort()) out.push({ key: `f:${f}`, path: f, name: f.slice(d.path.length), depth, dir: false, ignored: isIgnored.has(f) })
    }
    walk(tree, 0)
    return out
  })

  /* ---------- Keyboard and choice ---------- */

  let cursor = $state(0)
  $effect(() => {
    void terms
    cursor = Math.min(cursor, Math.max(0, rows.length - 1))
  })
  let list: HTMLDivElement | undefined = $state()
  function move(to: number) {
    cursor = Math.max(0, Math.min(rows.length - 1, to))
    list?.querySelector(`[data-i="${cursor}"]`)?.scrollIntoView({ block: 'nearest' })
  }
  function choose(r: Row) {
    if (r.dir) return fold(r.path)
    onpick(r.path, source === 'scratchpad')
    onclose()
  }
  function onkeydown(e: KeyboardEvent) {
    const r = rows[cursor]
    if (e.key === 'ArrowDown') move(cursor + 1)
    else if (e.key === 'ArrowUp') move(cursor - 1)
    else if (e.key === 'Enter' && r) choose(r)
    else if (e.key === 'ArrowRight' && r?.dir && !query) fold(r.path, true)
    else if (e.key === 'ArrowLeft' && r?.dir && !query) fold(r.path, false)
    else if (e.key === 'Tab') setSource(source === 'folder' ? 'scratchpad' : 'folder')
    else if (e.key === 'Escape') {
      e.stopPropagation()
      onclose()
    } else return
    e.preventDefault()
  }
  const focus = (el: HTMLInputElement) => el.focus()
  /** A click outside closes it. */
  let box: HTMLDivElement | undefined = $state()
  function outside(e: MouseEvent) {
    if (box && !box.contains(e.target as Node) && !(e.target as HTMLElement).closest?.('[data-picker-toggle]')) onclose()
  }
</script>

<svelte:window onmousedown={outside} />

<div class="picker" bind:this={box} role="dialog" aria-label={t('Open a page')}>
  <div class="top">
    <span class="seg" role="tablist">
      <button role="tab" class:on={source === 'folder'} aria-selected={source === 'folder'} onclick={() => setSource('folder')} title={t('Pages, documents and images of the agent’s folder')}>{t('Folder')} <b>{pages?.length ?? '…'}</b></button>
      <button role="tab" class:on={source === 'scratchpad'} aria-selected={source === 'scratchpad'} onclick={() => setSource('scratchpad')} title={t('Pages {cli} wrote in the scratchpad of this session (its artifacts)', { cli })}>{t('Artifacts')} <b>{artifacts?.length ?? '…'}</b></button>
    </span>
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={query} use:focus {onkeydown} placeholder={source === 'folder' ? t('Search the folder…') : t('Search the artifacts…')} spellcheck="false" aria-label={t('Search')} />
  </div>
  <div class="list" bind:this={list} role="listbox">
    {#each rows as r, i (r.key)}
      <button class="row" class:on={i === cursor} class:dir={r.dir} class:ign={r.ignored} data-i={i} role="option" aria-selected={i === cursor} style:padding-left="{8 + r.depth * 14}px" onmouseenter={() => (cursor = i)} onclick={() => choose(r)} title={r.path}>
        {#if r.dir}
          <Icon name={open.has(r.path) ? 'chevronDown' : 'chevronRight'} /><Icon name="folder" /><span class="name">{r.name}</span>
        {:else}
          <span class="sp"></span><Icon name={iconOf(r.path)} />
          {#if terms.length && source === 'folder' && r.name.includes('/')}<span class="name">{r.name.split('/').pop()}</span><span class="dirname">{r.name.split('/').slice(0, -1).join('/')}</span>
          {:else}<span class="name">{r.name}</span>{/if}
          {#if r.ignored}<span class="when">{t('ignored')}</span>{/if}
          {#if r.when !== undefined}<span class="when">{ago(r.when)}</span>{/if}
        {/if}
      </button>
    {:else}
      <p class="none">
        {#if source === 'folder'}{pages === null ? t('Reading the folder…') : terms.length ? t('No file matches.') : t('No page, document or image in this folder.')}
        {:else}{artifacts === null ? t('Reading the scratchpad…') : terms.length ? t('No artifact matches.') : t('No artifact yet: {cli} writes them in the scratchpad of its session.', { cli })}{/if}
      </p>
    {/each}
  </div>
  <div class="foot">
    <span><kbd>↑</kbd><kbd>↓</kbd> {tc('key', 'move')} · <kbd>↵</kbd> {tc('key', 'open')} · <kbd>tab</kbd> {t('folder / artifacts')} · <kbd>esc</kbd> {tc('key', 'close')}</span>
    {#if source === 'folder'}<label class="ign-check" title={t('Also the files .gitignore leaves out (a mockup, a build report); dependencies and caches are never listed')}><input type="checkbox" checked={ignored} onchange={(e) => onignored(e.currentTarget.checked)} />{t('.gitignored files')}</label>{/if}
  </div>
</div>

<style>
  .picker { position: absolute; top: 36px; left: 8px; right: 8px; z-index: 20; display: flex; flex-direction: column; max-height: min(60vh, 520px); border: 1px solid var(--border); border-radius: var(--r9); background: var(--surface); box-shadow: var(--shadow-pop); overflow: hidden; }
  .top { display: flex; align-items: center; gap: 6px; padding: 7px; border-bottom: 1px solid var(--border); }
  .top input { flex: 1; min-width: 0; height: 26px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); outline: none; }
  .top input:focus { border-color: var(--src-claude); }
  .seg { display: inline-flex; flex-shrink: 0; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { display: inline-flex; align-items: center; gap: 5px; padding: 3px 9px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .seg button + button { border-left: 1px solid var(--border); }
  .seg button.on { background: color-mix(in srgb, var(--src-claude) 14%, var(--surface)); color: var(--src-claude); font-weight: 600; }
  .seg b { font-weight: 500; opacity: 0.8; }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 4px; }
  .row { display: flex; align-items: center; gap: 6px; width: 100%; height: 24px; padding-right: 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: var(--fs-meta) var(--mono); text-align: left; cursor: pointer; }
  .row.on { background: color-mix(in srgb, var(--src-claude) 14%, transparent); }
  .row :global(svg) { flex-shrink: 0; color: var(--text-muted); }
  .row.dir :global(svg:nth-of-type(2)) { color: var(--src-claude); }
  .sp { width: 1em; flex-shrink: 0; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dirname { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); direction: rtl; text-align: left; }
  .when { margin-left: auto; flex-shrink: 0; color: var(--text-muted); font-family: var(--sans); }
  .none { margin: 0; padding: 12px; color: var(--text-muted); font-size: var(--fs-meta); }
  .row.ign .name { opacity: 0.6; }
  .ign-check { display: inline-flex; align-items: center; gap: 4px; margin-left: auto; color: var(--text-muted); cursor: pointer; white-space: nowrap; }
  .ign-check input { margin: 0; --tick: var(--src-claude); }
  .foot { display: flex; align-items: center; gap: 10px; padding: 5px 9px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-tag); }
  kbd { font: 10px var(--mono); padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); }
</style>
