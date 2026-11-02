<script lang="ts">
  import { onDestroy } from 'svelte'
  import Icon from './Icon.svelte'
  import Gutter from './Gutter.svelte'
  import FilePane from './files/FilePane.svelte'
  import FileSearch from './files/FileSearch.svelte'
  import QuickOpen from './files/QuickOpen.svelte'
  import { agentFiles, downloadAgentFiles, type AgentFile } from '../agents.svelte'
  import { agentWorking, applySettings, checkStamps, closeTab, filesSettings, hasPending, keyOf, metaOf, openFile, paletteFiles, pinTab, saveBuffer, savePending, setSplit, tabsOf } from '../files-editor.svelte'
  import { armer } from '../armed.svelte'
  import { keyedAction } from '../app-menu'
  import { paletteKey, paletteOn } from '../palette.svelte'
  import { matches } from '../shortcut'
  import { pref, setPref } from '../layout.svelte'
  import { toast } from '../toast.svelte'
  import { t, tn } from '../i18n.svelte'
  import type { Agent } from '../types'

  /**
   * The Files tab of an agent's side panel: its folder as a tree (or the search in its files,
   * ⌘⇧F), and the files opened from it in tabs, each in an editor (`files/FilePane`). Folders
   * fold (remembered per agent); by default those holding changes are open, and all of them while
   * filtering. A folder shows how many changed files it holds. A click opens a file in a tab the
   * next one replaces; a double click, or an edit, keeps it. ↑ ↓ move, → ← unfold or fold, Enter
   * opens. ⌘P opens a file by its name. "Ignored" lists the files `.gitignore` leaves out too
   * (dimmed). ⬇ downloads the selected folder, or else the files listed, as a zip.
   * Narrow, the tree on top and the file in a card below; from `WIDE` pixels of panel, side by
   * side, and a second file can open beside the first (◫).
   */
  let { agent, onweb, ondiff }: { agent: Agent; onweb: (path: string) => void; ondiff: () => void } = $props()

  let files = $state<AgentFile[] | null>(null)
  let truncated = $state(false)
  let error = $state('')
  let query = $state('')
  let changedOnly = $state(pref<boolean>('agents.side.changed', false))
  const setChanged = (on: boolean) => setPref('agents.side.changed', (changedOnly = on))
  let withIgnored = $state(pref<boolean>('agents.side.ignored', false))
  function setIgnored(on: boolean) {
    setPref('agents.side.ignored', (withIgnored = on))
    void load()
  }
  let root = $state('')
  async function load() {
    error = ''
    try {
      const r = await agentFiles(agent.id, withIgnored)
      files = r.files
      root = r.root
      truncated = r.truncated
    } catch (e) {
      error = String(e)
    }
  }
  $effect(() => {
    if (!files && !error) void load()
  })

  const STATUS: Record<string, string> = { M: t('Modified'), A: t('Added'), D: t('Deleted'), R: t('Renamed'), '?': t('Untracked') }
  const changed = $derived((files ?? []).filter((f) => f.status).length)
  const filtering = $derived(!!query.trim() || changedOnly)
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return (files ?? []).filter((f) => (!changedOnly || f.status) && (!q || f.path.toLowerCase().includes(q)))
  })

  /* ---------- Tree ---------- */

  /** `kept`: files under it that `.gitignore` does not leave out (none: the folder shows dimmed). */
  interface Dir { name: string; path: string; dirs: Map<string, Dir>; files: AgentFile[]; changed: number; kept: number }
  type Row = { path: string; name: string; depth: number; dir: boolean; status: string | null; changed: number; ignored: boolean }

  const tree = $derived.by(() => {
    const root: Dir = { name: '', path: '', dirs: new Map(), files: [], changed: 0, kept: 0 }
    for (const f of filtered) {
      const parts = f.path.split('/')
      let d = root
      for (const p of parts.slice(0, -1)) {
        let c = d.dirs.get(p)
        if (!c) d.dirs.set(p, (c = { name: p, path: `${d.path}${p}/`, dirs: new Map(), files: [], changed: 0, kept: 0 }))
        if (f.status) c.changed++
        if (!f.ignored) c.kept++
        d = c
      }
      d.files.push(f)
    }
    return root
  })

  // Folded and unfolded folders, per agent: what was toggled by hand; the rest follows the default.
  // svelte-ignore state_referenced_locally
  let toggled = $state<Record<string, boolean>>(pref<Record<string, Record<string, boolean>>>('agents.side.tree', {})[agent.id] ?? {})
  const isOpen = (d: Dir) => toggled[d.path] ?? (filtering || d.changed > 0)
  function toggle(path: string, open?: boolean) {
    const d = find(path)
    if (!d) return
    toggled = { ...toggled, [path]: open ?? !isOpen(d) }
    setPref('agents.side.tree', { ...pref<Record<string, Record<string, boolean>>>('agents.side.tree', {}), [agent.id]: toggled })
  }
  function find(path: string): Dir | null {
    let d: Dir | undefined = tree
    for (const p of path.split('/').filter(Boolean)) d = d?.dirs.get(p)
    return d ?? null
  }

  /** At most this many rows: a whole repository does not fit in the DOM. */
  const SHOWN = 1500
  const rows = $derived.by(() => {
    const out: Row[] = []
    const walk = (d: Dir, depth: number) => {
      for (const c of [...d.dirs.values()].sort((a, b) => a.name.localeCompare(b.name))) {
        if (out.length >= SHOWN) return
        out.push({ path: c.path, name: c.name, depth, dir: true, status: null, changed: c.changed, ignored: !c.kept })
        if (isOpen(c)) walk(c, depth + 1)
      }
      for (const f of d.files) {
        if (out.length >= SHOWN) return
        out.push({ path: f.path, name: f.path.slice(d.path.length), depth, dir: false, status: f.status, changed: 0, ignored: f.ignored })
      }
    }
    walk(tree, 0)
    return out
  })

  let selected = $state<string | null>(null)
  let treeEl: HTMLDivElement | undefined = $state()
  function pick(r: Row, pin = false) {
    selected = r.path
    if (r.dir) toggle(r.path)
    else openFile(agent.id, r.path, { pin })
  }
  function onkeydown(e: KeyboardEvent) {
    const i = rows.findIndex((r) => r.path === selected)
    const r = rows[i]
    const go = (n: number) => {
      const next = rows[Math.max(0, Math.min(rows.length - 1, n))]
      if (!next) return
      selected = next.path
      treeEl?.querySelector(`[data-path="${CSS.escape(next.path)}"]`)?.scrollIntoView({ block: 'nearest' })
      if (!next.dir) openFile(agent.id, next.path)
    }
    if (e.key === 'ArrowDown') go(i + 1)
    else if (e.key === 'ArrowUp') go(i < 0 ? 0 : i - 1)
    else if (e.key === 'ArrowRight' && r?.dir) toggle(r.path, true)
    else if (e.key === 'ArrowLeft' && r?.dir && isOpen(find(r.path)!)) toggle(r.path, false)
    else if (e.key === 'ArrowLeft' && r) {
      // On a file or a folded folder: up to the parent folder.
      const parent = r.path.replace(/[^/]+\/?$/, '')
      if (parent) go(rows.findIndex((x) => x.path === parent))
    } else if (e.key === 'Enter' && r) pick(r)
    else return
    // The tree's keys stay here: not the dashboard's, nor the terminal's.
    e.preventDefault()
    e.stopPropagation()
  }

  /* ---------- Download ---------- */

  /** The selected folder, or else the files listed (filter and Changed applied). */
  const selectedDir = $derived(selected?.endsWith('/') ? selected : null)
  const folderName = (p: string) => p.replace(/\/+$/, '').split('/').pop() || 'files'
  const zipTitle = $derived(selectedDir ? t('Download {folder} as a zip', { folder: selectedDir }) : tn(filtered.length, 'Download the {n} file listed as a zip', 'Download the {n} files listed as a zip'))
  let downloading = $state(false)
  async function download(paths: string[], name: string) {
    downloading = true
    try {
      const saved = await downloadAgentFiles(agent.id, paths, name)
      if (saved) toast(t('{name} saved', { name: saved }))
    } catch (e) {
      error = String(e)
    } finally {
      downloading = false
    }
  }
  function downloadListed() {
    if (selectedDir) void download([selectedDir], folderName(selectedDir))
    else void download(filtered.map((f) => f.path), `${folderName(root)}${changedOnly ? '-changed' : ''}`)
  }

  /* ---------- Tabs ---------- */

  const tabs = $derived(tabsOf(agent.id))
  const statusOf = $derived(new Map((files ?? []).map((f) => [f.path, f.status])))
  const openPaths = $derived(new Set(tabs.tabs.map((x) => x.path)))
  const closer = armer()
  function close(path: string) {
    // Not saved: a second click closes it all the same.
    if (metaOf(agent.id, path)?.dirty && !closer.confirm(path)) return toast(t('Not saved: click again to close it all the same'), 2500)
    closeTab(agent.id, path)
  }
  /** The pane the keyboard is in: the second one opens beside the first. */
  let focusedPane = $state(0)
  let navTab = $state(pref<'files' | 'search'>('agents.side.nav', 'files'))
  const setNav = (v: 'files' | 'search') => setPref('agents.side.nav', (navTab = v))
  let quickOpen = $state(false)
  let seed = $state<{ text: string; at: number } | null>(null)
  function searchFor(text: string) {
    setNav('search')
    seed = { text, at: Date.now() }
  }

  /** ⌘P quick open (unless the palette's key), ⌘⇧F the search; Esc stays the editor's. */
  function onFilesKey(e: KeyboardEvent) {
    const chord = e.metaKey || e.ctrlKey
    if (chord && !e.altKey && e.key.toLowerCase() === 'p' && !e.shiftKey && !(paletteOn() && matches(e, paletteKey()))) {
      e.preventDefault()
      e.stopPropagation()
      quickOpen = true
    } else if (chord && e.shiftKey && !e.altKey && e.key.toLowerCase() === 'f') {
      // Not Zen mode here: the menu's echo of the key is ignored too.
      e.preventDefault()
      e.stopPropagation()
      keyedAction('zen')
      const sel = window.getSelection()?.toString() ?? ''
      setNav('search')
      seed = { text: sel && !sel.includes('\n') ? sel : '', at: Date.now() }
    } else if (chord && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 's' && tabs.active) {
      // Anywhere in the tab (a banner's button, the tree): the file shown.
      e.preventDefault()
      e.stopPropagation()
      void saveBuffer(keyOf(agent.id, focusedPane === 1 && tabs.split ? tabs.split : tabs.active))
    } else if (e.key === 'Escape' && (e.target as HTMLElement | null)?.closest?.('.cm-editor')) e.stopPropagation()
  }

  /* ---------- The agent writing ---------- */

  const working = $derived(agentWorking(agent.id))
  // Its turn over: what waited is saved.
  $effect(() => {
    if (!working && hasPending(agent.id)) savePending(agent.id)
  })
  // Files it changes are read again; the list too, its statuses with them.
  $effect(() => {
    if (!filesSettings().live_reload) return
    const timer = setInterval(() => {
      if (document.hidden) return
      void checkStamps(agent.id).then((changed) => {
        if (changed) void load()
      })
    }, 2500)
    return () => clearInterval(timer)
  })
  // A setting changed: the editors follow.
  $effect(() => {
    void JSON.stringify(filesSettings())
    void applySettings()
  })
  // The palette offers these files.
  $effect(() => {
    paletteFiles.agent = agent.id
    paletteFiles.title = agent.title
    paletteFiles.paths = (files ?? []).map((f) => f.path)
  })
  onDestroy(() => {
    if (paletteFiles.agent === agent.id) Object.assign(paletteFiles, { agent: null, title: '', paths: [] })
  })

  /* ---------- Split ---------- */

  /** The tree's part, filter bar included: its height when stacked, its width side by side. */
  const TREE_H = 254
  const TREE_W = 260
  const WIDE = 640
  let width = $state(0)
  const wide = $derived(width >= WIDE)
  let treeH = $state(pref<number>('agents.side.treeH', TREE_H))
  let treeW = $state(pref<number>('agents.side.treeW', TREE_W))
  const resize = (delta: number, [a, b]: [number, number]) => {
    const n = Math.round(Math.max(wide ? 160 : 80, Math.min(a + b - (wide ? 240 : 80), a + delta)))
    if (wide) setPref('agents.side.treeW', (treeW = n))
    else setPref('agents.side.treeH', (treeH = n))
  }
  const resetSplit = () => (wide ? setPref('agents.side.treeW', (treeW = TREE_W)) : setPref('agents.side.treeH', (treeH = TREE_H)))
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="files" class:wide bind:clientWidth={width} onkeydown={onFilesKey}>
<div class="nav" style:height={wide ? null : `${treeH}px`} style:width={wide ? `${treeW}px` : null}>
<div class="seg">
  <button class:on={navTab === 'files'} onclick={() => setNav('files')}>{t('Files')}</button>
  <button class:on={navTab === 'search'} onclick={() => setNav('search')} title={t('Search in every file (⌘⇧F)')}>{t('Search')}</button>
  <button onclick={() => (quickOpen = true)} title={t('Open a file by its name (⌘P)')}><Icon name="search" size={11} />⌘P</button>
</div>
{#if navTab === 'search'}
  <FileSearch {agent} files={files ?? []} {seed} />
{:else}
<div class="bar">
  <input bind:value={query} placeholder={t('Filter files')} spellcheck="false" />
  <button class:on={changedOnly} onclick={() => setChanged(!changedOnly)} title={t('Only the files git sees as changed')}>{t('Changed')} {changed}</button>
  <button class:on={withIgnored} onclick={() => setIgnored(!withIgnored)} title={t('Also the files .gitignore leaves out (.env, builds…), dimmed; dependencies and caches (node_modules, target…) never listed')}>{t('Ignored')}</button>
  <button onclick={downloadListed} disabled={downloading || (!selectedDir && !filtered.length)} title={zipTitle}><Icon name="download" spin={downloading} /></button>
  <button onclick={() => { toggled = {}; setPref('agents.side.tree', { ...pref<Record<string, Record<string, boolean>>>('agents.side.tree', {}), [agent.id]: {} }) }} title={t('Back to the default: only the folders holding changes open')}><Icon name="chevronDown" /></button>
  <button onclick={load} title={t('Read the folder again')}><Icon name="refresh" /></button>
</div>
{#if error}<p class="err">{error}</p>{/if}
<!-- svelte-ignore a11y_no_noninteractive_tabindex : the tree takes the arrow keys -->
<div class="tree" bind:this={treeEl} tabindex="0" role="tree" aria-label={t('Files of {name}', { name: agent.title })} {onkeydown}>
  {#each rows as r (r.path)}
    <button
      class="n"
      class:dir={r.dir}
      class:sel={r.path === selected}
      class:ign={r.ignored}
      data-path={r.path}
      style:padding-left="{6 + r.depth * 14}px"
      role="treeitem"
      aria-selected={r.path === selected}
      aria-expanded={r.dir ? isOpen(find(r.path)!) : undefined}
      tabindex="-1"
      class:open={openPaths.has(r.path)}
      onclick={() => pick(r)}
      ondblclick={() => !r.dir && pinTab(agent.id, r.path)}
      title={r.dir ? r.path : `${r.path}${r.status ? ` · ${STATUS[r.status] ?? r.status}` : ''}${r.ignored ? ` · ${t('ignored by .gitignore')}` : ''}`}
    >
      <span class="car">{#if r.dir}<Icon name={isOpen(find(r.path)!) ? 'chevronDown' : 'chevronRight'} size={10} />{/if}</span>
      <span class="ic"><Icon name={r.dir ? 'folder' : 'files'} size={12} /></span>
      <span class="nm">{r.name}</span>
      {#if r.dir && r.changed}<span class="cnt" title={tn(r.changed, '{n} changed file', '{n} changed files')}>{r.changed}</span>{/if}
      {#if r.status}<span class="s s{r.status === '?' ? 'u' : r.status}">{r.status}</span>{/if}
    </button>
  {:else}
    {#if files}<p class="muted empty">{filtering ? t('No file matches.') : t('No file.')}</p>{/if}
  {/each}
  {#if rows.length >= SHOWN || truncated}<p class="muted empty">{truncated ? t('List cut (folder too large): filter to narrow.') : t('List cut: filter to narrow.')}</p>{/if}
</div>
{/if}
</div>
<Gutter dir={wide ? 'x' : 'y'} onresize={resize} onreset={resetSplit} />
<div class="well">
<div class="prev">
  {#if tabs.tabs.length}
    <div class="ftabs" role="tablist">
      {#each tabs.tabs as tab (tab.path)}
        {@const m = metaOf(agent.id, tab.path)}
        {@const st = statusOf.get(tab.path)}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div class="ft" class:on={tab.path === tabs.active} class:side={tab.path === tabs.split} class:loose={!tab.pinned} role="tab" tabindex="-1" aria-selected={tab.path === tabs.active} title={tab.path}
          onclick={(e) => openFile(agent.id, tab.path, { side: wide && (e.altKey || focusedPane === 1) })}
          ondblclick={() => pinTab(agent.id, tab.path)}
          onauxclick={(e) => e.button === 1 && close(tab.path)}>
          <span class="nm">{tab.path.slice(tab.path.lastIndexOf('/') + 1)}</span>
          {#if st}<span class="s s{st === '?' ? 'u' : st}">{st}</span>{/if}
          {#if m?.dirty}<span class="dot" class:armed={closer.is(tab.path)} title={t('Not saved')}></span>{/if}
          <button class="x" class:armed={closer.is(tab.path)} onclick={(e) => { e.stopPropagation(); close(tab.path) }} title={t('Close')}><Icon name="close" size={10} /></button>
        </div>
      {/each}
      <span class="grow"></span>
      {#if wide && tabs.active}<button class="tb" class:on={!!tabs.split} onclick={() => setSplit(agent.id, tabs.split ? null : (tabs.tabs.find((x) => x.path !== tabs.active)?.path ?? null))} disabled={!tabs.split && tabs.tabs.length < 2} title={tabs.split ? t('One file') : t('Two files side by side: ⌘↵ in quick open, or ⌥-click a tab, opens one beside')}><Icon name="split" /></button>{/if}
    </div>
  {/if}
  {#if tabs.active}
    <div class="panes">
      <FilePane {agent} path={tabs.active} status={statusOf.get(tabs.active) ?? null} {onweb} {ondiff} ondownload={(p) => void download([p], folderName(p))} onfocus={() => (focusedPane = 0)} />
      {#if wide && tabs.split}
        <div class="sep"></div>
        <FilePane {agent} path={tabs.split} status={statusOf.get(tabs.split) ?? null} {onweb} {ondiff} ondownload={(p) => void download([p], folderName(p))} onfocus={() => (focusedPane = 1)} />
      {/if}
    </div>
  {:else}
    <p class="muted empty">{t('Click a file to open it, or ⌘P to find it by its name.')}</p>
  {/if}
</div>
</div>
{#if quickOpen}<QuickOpen {agent} files={files ?? []} onclose={() => (quickOpen = false)} onsearch={searchFor} />{/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .bar input { flex: 1; min-width: 0; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: var(--fs-meta) var(--mono); outline: none; }
  .bar input:focus { border-color: var(--src-claude); }
  .bar button { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .bar button.on { border-color: var(--src-claude); color: var(--src-claude); }
  .err { margin: 0; padding: 6px 10px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .empty { padding: 12px; font-size: var(--fs-meta); }
  .grow { flex: 1; }

  /* Stacked: the tree's part on a muted ground, the file in a card on it. Side by side: the
     tree's part a muted column, the file beside it. */
  .files { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .files.wide { flex-direction: row; }
  .nav { flex-shrink: 0; min-height: 0; min-width: 0; display: flex; flex-direction: column; background: var(--surface-muted); }
  .well { flex: 1; min-height: 0; min-width: 0; display: flex; padding: 0 8px 8px; background: var(--surface-muted); }
  .wide .well { padding: 0; background: var(--surface); }
  .files > :global(.gutter) { flex-basis: 8px; background: var(--surface-muted); }
  .files > :global(.gutter)::before { content: ''; position: absolute; border-radius: var(--r2); background: color-mix(in srgb, var(--text-muted) 45%, transparent); }
  .files > :global(.gutter.y)::before { left: 50%; top: 3px; width: 28px; height: 2px; margin-left: -14px; }
  .files > :global(.gutter.x)::before { top: 50%; left: 3px; width: 2px; height: 28px; margin-top: -14px; }
  .files > :global(.gutter.x) { border-left: 1px solid var(--border); }
  .files > :global(.gutter.y)::after { top: 3px; }
  .files > :global(.gutter.x)::after { left: 3px; }
  .tree { flex: 1; min-height: 0; overflow: auto; padding: 3px 4px; outline: none; }
  .tree:focus-visible { box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--src-claude) 50%, transparent); }
  .n { display: flex; align-items: center; gap: 4px; width: 100%; padding: 1px 6px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: var(--fs-meta) var(--mono); text-align: left; white-space: nowrap; cursor: pointer; }
  .n:hover { background: var(--surface-muted); }
  .n.sel { background: color-mix(in srgb, var(--src-claude) 14%, transparent); }
  .n .car { display: inline-flex; width: 10px; color: var(--text-muted); }
  .n .ic { display: inline-flex; color: var(--text-muted); }
  .n.dir .ic { color: var(--src-claude); }
  .n .nm { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .n.dir .nm { font-weight: 600; font-family: var(--sans); }
  .n.ign { opacity: 0.55; }
  .bar button:disabled { opacity: 0.5; cursor: default; }
  .n .cnt { margin-left: auto; color: var(--warn); font-size: var(--fs-tag); font-variant-numeric: tabular-nums; }
  .n .s { margin-left: auto; }
  .n .cnt + .s { margin-left: 4px; }
  .s { width: 14px; text-align: center; font: 700 var(--fs-tag) var(--mono); }
  .sM, .sR { color: var(--warn); }
  .sA, .su { color: var(--ok); }
  .sD { color: var(--ko); }

  .prev { position: relative; flex: 1; min-height: 0; min-width: 0; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); overflow: hidden; box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06); }
  .wide .prev { border: none; border-radius: 0; box-shadow: none; }
  .seg { display: flex; gap: 2px; padding: 6px 8px 0; }
  .seg button { flex: 1; display: inline-flex; align-items: center; justify-content: center; gap: 3px; padding: 2px 0; border: none; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .seg button:last-child { flex: 0 0 auto; padding: 2px 6px; font-size: var(--fs-tag); }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08); }
  .n.open .nm { text-decoration: underline dotted color-mix(in srgb, var(--text-muted) 70%, transparent); text-underline-offset: 2px; }
  .ftabs { display: flex; align-items: stretch; flex-shrink: 0; background: var(--surface-muted); border-bottom: 1px solid var(--border); font-size: var(--fs-meta); overflow-x: auto; scrollbar-width: none; }
  .ft { display: flex; align-items: center; gap: 5px; padding: 4px 4px 4px 10px; border-right: 1px solid var(--border); color: var(--text-muted); white-space: nowrap; cursor: pointer; }
  .ft.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: inset 0 2px 0 var(--src-claude); }
  .ft.side { background: var(--surface); color: var(--text); box-shadow: inset 0 2px 0 var(--border); }
  .ft.loose .nm { font-style: italic; }
  .ft .x { display: inline-flex; padding: 2px; border: none; border-radius: var(--r4); background: none; color: inherit; opacity: 0.5; cursor: pointer; }
  .ft .x:hover { opacity: 1; background: var(--surface-muted); }
  .ft .x.armed { opacity: 1; color: var(--ko); }
  .ft .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text); }
  .ft .dot.armed { background: var(--ko); }
  .ft .s { width: auto; }
  .tb { display: inline-flex; align-items: center; padding: 0 8px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .tb.on { color: var(--src-claude); }
  .tb:disabled { opacity: 0.4; cursor: default; }
  .panes { flex: 1; min-height: 0; display: flex; }
  .panes .sep { flex: 0 0 1px; background: var(--border); }
  /* On the phone: rows a thumb can hit, a filter iOS does not zoom in on. */
  :global(html.remote) .n { padding-block: 6px; }
  :global(html.remote) .bar input { font-size: 16px; }
</style>
