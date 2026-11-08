<script lang="ts">
  import { untrack } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { armer } from '../../armed.svelte'
  import { hasCore } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import type { IconName } from '../../icons'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import { canPick, pickPath } from '../../pick'
  import { FILE_MANAGER, isMac } from '../../platform'
  import { ago } from '../../time'
  import { copy } from '../../toast.svelte'
  import { t, tc, tn } from '../../i18n.svelte'
  import { call, type BlockProps } from '../block'
  import Treemap from './Treemap.svelte'
  import { chain, count, deltaText, folderColor, kindColor, KINDS, RECLAIM, share, size, type DiskNode, type DiskReclaim, type DiskTrash, type DiskUsageState } from './types'

  /**
   * Disk usage: a folder scanned on demand, then where its space goes — the list of its folders
   * by size on the left (Δ since the scan before), the treemap on the right, the disk it is on at
   * the top, and Reclaimable below: what can be rebuilt, freed in two clicks. A click selects,
   * a double click enters a folder, a right click opens its actions. Narrow (a phone, a thin
   * column): the list only.
   */
  let { state: du, error }: BlockProps<DiskUsageState> = $props()
  const scan = $derived(du?.scan ?? null)
  const running = $derived(du?.running ?? null)

  /** The folder shown (its children three levels down), and the folders above it. */
  let view = $state<DiskNode | null>(null)
  let viewScan = 0
  let trail = $state<{ id: number; name: string }[]>([])
  const opened = new SvelteSet<number>()
  let selected = $state<DiskNode | null>(null)
  let hovered = $state<DiskNode | null>(null)
  let colorBy = $state<'kind' | 'folder'>('kind')
  let menu = $state<{ x: number; y: number; n: DiskNode } | null>(null)
  let busy = $state<string | null>(null)
  let failed = $state<string | null>(null)
  let done = $state<string | null>(null)
  /** The message after a move to the Trash: it offers to empty it. */
  let offerEmpty = $state(false)
  let root = $state('')
  const arming = armer()

  const args = (id: number, more: Record<string, unknown> = {}) => ({ scan: scan?.id, id, ...more })

  async function load(id: number): Promise<boolean> {
    if (!scan) return false
    try {
      view = await call<DiskNode>('disk_usage', 'tree', args(id, { depth: 3 }))
      failed = null
      return true
    } catch (e) {
      failed = String(e)
      return false
    }
  }
  /** The folder shown read again after something was freed; gone itself: the nearest one above still there. */
  async function reload() {
    if (await load(view?.id ?? 0)) return
    while (trail.length) {
      const up = trail[trail.length - 1]
      trail = trail.slice(0, -1)
      selected = null
      if (await load(up.id)) return
    }
    await load(0)
  }

  /** A new scan: back to its root; the same one changed (something freed): read again where it was. */
  const scanKey = $derived(scan ? `${scan.id}:${scan.bytes}` : '')
  $effect(() => {
    void scanKey
    const s = untrack(() => scan)
    untrack(() => {
      if (!s) {
        view = null
      } else if (s.id !== viewScan) {
        viewScan = s.id
        trail = []
        opened.clear()
        selected = null
        void load(0)
      } else {
        void reload()
      }
    })
  })

  /** The roots offered: `[disk_usage] roots`, then the one scanned last if it is another. */
  const roots = $derived([...new Set([...(du?.roots ?? []), ...(scan ? [scan.root] : [])])])
  $effect(() => {
    if (!root && roots.length) root = scan?.root ?? roots[0]
  })

  async function act(key: string, command: string, extra: Record<string, unknown>, after?: (out: unknown) => void) {
    busy = key
    failed = null
    done = null
    offerEmpty = false
    try {
      const out = await call<unknown>('disk_usage', command, extra)
      after?.(out)
    } catch (e) {
      failed = String(e)
    } finally {
      busy = null
    }
  }
  const startScan = (at = root) => act('scan', 'scan', { root: at })
  const stopScan = () => act('stop', 'stop', {})
  async function chooseRoot() {
    const p = await pickPath(root || '~')
    if (p) {
      root = p
      void startScan(p)
    }
  }

  /** Into folder `n` (in the view, or anywhere with its folders above). */
  async function enter(n: DiskNode) {
    if (!n.dir || !view) return
    const path = chain(view, n.id)
    if (path) trail = [...trail, ...path.slice(0, -1).map(({ id, name }) => ({ id, name }))]
    selected = null
    await load(n.id)
  }
  function up(i: number) {
    const to = trail[i]
    trail = trail.slice(0, i)
    selected = null
    void load(to.id)
  }
  /** Node `id`, anywhere in the scan: its folder shown, itself selected. */
  async function show(id: number) {
    try {
      const path = await call<[number, string][]>('disk_usage', 'ancestors', args(id))
      const parent = path.length > 1 ? path[path.length - 2] : path[0]
      trail = path.slice(0, -2).map(([pid, name]) => ({ id: pid, name }))
      await load(parent[0])
      selected = view?.children?.find((c) => c.id === id) ?? null
    } catch (e) {
      failed = String(e)
    }
  }

  /** A folder of the list opened: its children read when the view stops above them. */
  async function toggle(n: DiskNode) {
    if (opened.has(n.id)) return opened.delete(n.id)
    opened.add(n.id)
    if (n.dir && n.children === null) {
      try {
        const more = await call<DiskNode>('disk_usage', 'tree', args(n.id, { depth: 2 }))
        n.children = more.children
        n.rest_bytes = more.rest_bytes
        n.rest_count = more.rest_count
      } catch (e) {
        failed = String(e)
      }
    }
  }

  interface Row {
    n: DiskNode | null
    depth: number
    top: number
    /** A folder's leftovers: its small files and children. */
    rest?: { bytes: number; count: number }
  }
  const rows = $derived.by(() => {
    const out: Row[] = []
    const walk = (nodes: DiskNode[], depth: number, top: number | null, parent: DiskNode) => {
      nodes.forEach((n, i) => {
        const tp = top ?? i
        out.push({ n, depth, top: tp })
        if (n.dir && opened.has(n.id) && n.children) walk(n.children, depth + 1, tp, n)
      })
      if (parent.rest_bytes > 0) out.push({ n: null, depth, top: top ?? 0, rest: { bytes: parent.rest_bytes, count: parent.rest_count } })
    }
    if (view?.children) walk(view.children, 0, null, view)
    return out
  })

  const colorOf = (n: DiskNode, top: number, depth: number) => (colorBy === 'kind' ? kindColor(n.kind) : folderColor(top, depth))
  const hasDelta = $derived(!!scan?.previous_at && du?.delta)

  /** The disk: what was scanned, the rest used, and free. */
  const strip = $derived.by(() => {
    const d = du?.disk
    if (!d?.total || !scan) return null
    const used = Math.max(0, d.total - d.available)
    const scanned = Math.min(scan.bytes, used)
    return { d, scanned, other: used - scanned, free: d.available }
  })

  /** A running scan's share done, when this root was scanned before (its size known). */
  const progress = $derived(running && scan && scan.root === running.root && scan.bytes ? Math.min(0.99, running.bytes / scan.bytes) : null)

  interface Action {
    key: string
    label: string
    icon: IconName
    run: () => void
    danger?: boolean
  }
  function actions(n: DiskNode): Action[] {
    const out: Action[] = []
    if (n.dir && n.id !== view?.id) out.push({ key: 'enter', label: t('Open'), icon: 'chevronRight', run: () => void enter(n) })
    if (hasCore()) {
      out.push({ key: 'reveal', label: t('Show in {app}', { app: FILE_MANAGER }), icon: 'folder', run: () => void act('reveal', 'reveal', args(n.id)) })
      out.push({ key: 'terminal', label: t('Terminal here'), icon: 'terminal', run: () => void act('terminal', 'terminal', args(n.id)) })
    }
    out.push({ key: 'copy', label: t('Copy the path'), icon: 'copy', run: () => void copy(n.path, t('Path copied')) })
    if (n.dir) out.push({ key: 'scan', label: t('Scan this folder'), icon: 'refresh', run: () => void startScan((root = n.path)) })
    if (du?.cleanup && hasCore()) out.push({ key: 'trash', label: arming.is(`trash:${n.id}`) ? t('Sure? {name} to the Trash', { name: n.name }) : t('Move to the Trash'), icon: 'trash', danger: true, run: () => trash(n) })
    return out
  }
  function trash(n: DiskNode) {
    if (!arming.confirm(`trash:${n.id}`)) return
    menu = null
    void act(`trash:${n.id}`, 'trash', args(n.id), () => {
      done = t('{name} is in the Trash: emptying it frees {size}', { name: n.name, size: size(n.bytes) })
      offerEmpty = true
      if (selected?.id === n.id) selected = null
    })
  }
  function clean(kind: DiskReclaim, bytes: number) {
    if (!arming.confirm(`clean:${kind}`)) return
    void act(`clean:${kind}`, 'clean', { reclaim: kind }, (freed) => (done = t('{size} freed', { size: size(typeof freed === 'number' ? freed : bytes) })))
  }
  /** The Trash, measured apart from the scans (`[disk_usage] cleanup` on). */
  const bin = $derived<DiskTrash | null>(du?.cleanup ? (du.trash ?? null) : null)
  const binDenied = $derived(isMac ? t('Full Disk Access, in System Settings › Privacy & Security') : '')
  const binDetail = $derived(bin?.bins.map((b) => `${b.disk} — ${b.path}: ${size(b.bytes)}`).join('\n') ?? '')
  /** The Trash measured again when the block appears (it changes outside Prism too), and on a click. */
  const measureTrash = () => void call('disk_usage', 'trash_measure', {}).catch(() => {})
  let trashMeasured = false
  $effect(() => {
    if (trashMeasured || !hasCore() || !du?.cleanup) return
    trashMeasured = true
    measureTrash()
  })
  function emptyTrash() {
    if (!bin || !arming.confirm('empty-trash')) return
    void act('empty-trash', 'empty_trash', {}, (out) => {
      const r = (out ?? {}) as { freed?: number; left?: number }
      const freed = t('{size} freed', { size: size(r.freed ?? 0) })
      done = r.left ? `${freed} · ${tn(r.left, '{n} item left (locked or in use)', '{n} items left (locked or in use)')}` : freed
    })
  }
  function openMenu(e: MouseEvent, n: DiskNode) {
    e.preventDefault()
    selected = n
    menu = { x: e.clientX, y: e.clientY, n }
  }
  $effect(() => {
    if (!menu) return
    const close = (e: Event) => {
      if (e instanceof KeyboardEvent && e.key !== 'Escape') return
      if (e.target instanceof Element && e.target.closest('.du-menu')) return
      menu = null
    }
    document.addEventListener('mousedown', close, true)
    document.addEventListener('keydown', close, true)
    window.addEventListener('blur', close)
    return () => {
      document.removeEventListener('mousedown', close, true)
      document.removeEventListener('keydown', close, true)
      window.removeEventListener('blur', close)
    }
  })

  const focus = $derived(hovered ?? selected)
  const short = $derived(isShort('disk_usage'))
</script>

<section class="panel" style="--src: var(--accent)" class:collapsed={isCollapsed('disk_usage')}>
  <h2>
    <span class="title"><Icon name="disk" />{t('Disk usage')}</span>
    {#if du?.disk && !short}<small>{du.disk.name} · {size(du.disk.total)}</small>{/if}
    {#if du?.disk}<span class="pill" class:warn={du.disk.available < du.disk.total * 0.1} title={t('Free on {disk}', { disk: du.disk.name })}>{t('{size} free', { size: size(du.disk.available) })}</span>{/if}
    <span class="head">
      {#if du && hasCore()}
        {#if !short}
          <select class="root" bind:value={root} disabled={!!running} aria-label={t('Folder to scan')}>
            {#each roots as r (r)}<option value={r}>{r === '~' ? t('~ (home folder)') : r === '/' ? t('/ (the whole disk)') : /^[A-Za-z]:\\?$/.test(r) ? t('{drive} (the whole disk)', { drive: r }) : r}</option>{/each}
            {#if root && !roots.includes(root)}<option value={root}>{root}</option>{/if}
          </select>
          {#if canPick()}<button class="act" disabled={!!running} onclick={chooseRoot} title={t('Any folder')}>{t('Choose…')}</button>{/if}
        {/if}
        {#if running}
          <button class="act" disabled={busy === 'stop'} onclick={stopScan}><Icon name="stop" />{t('Stop')}</button>
        {:else}
          <button class="act pri" disabled={busy === 'scan' || !root} onclick={() => startScan()}><Icon name="refresh" />{scan ? t('Rescan') : t('Scan')}</button>
        {/if}
      {/if}
    </span>
    <BlockControls id="disk_usage" />
  </h2>

  <div class="du">
  {#if bin}
    {@const unreadable = bin.denied && !bin.bins.some((b) => b.items > 0)}
    <div class="trash" class:short>
      <Icon name="trash" /><button class="tlabel" onclick={measureTrash} disabled={!hasCore()} title={t('Measure the Trash again')}>{t('Trash')}</button>
      {#if !unreadable}
        {#if bin.items === 0 && !bin.emptying}
          <span class="muted">{tc('trash', 'empty')}</span>
        {:else}
          <span class="mono">{size(bin.bytes)}</span>
          <span class="muted">· {tn(bin.items, '{n} item', '{n} items')}{#if bin.emptying}{' '}{tc('trash', 'left')}{/if}</span>
        {/if}
        {#if bin.bins.length > 1 && !short}<span class="muted disks" title={binDetail}>· {tn(bin.bins.length, '{n} disk', '{n} disks')}</span>{/if}
      {/if}
      {#if bin.denied}
        <span class="warn" title={binDenied}>{unreadable ? '' : '· '}{isMac ? t('the home folder’s is not readable: Full Disk Access needed (System Settings › Privacy & Security)') : t('the home folder’s is not readable: check its permissions')}</span>
      {/if}
      <span class="sp"></span>
      {#if hasCore()}
        <button class="act" class:armed={arming.is('empty-trash')} disabled={busy !== null || bin.emptying || unreadable || bin.items === 0} onclick={emptyTrash} title={binDetail}>
          {bin.emptying || busy === 'empty-trash' ? t('Emptying…') : arming.is('empty-trash') ? t('Sure? {size} deleted', { size: size(bin.bytes) }) : tc('trash', 'Empty')}
        </button>
      {/if}
    </div>
  {/if}
  {#if !du}
    <div class="empty">{t('Disk usage is off. Turn on Settings › Infra › Disk usage.')}</div>
  {:else}
    {#if running}
      <div class="scanning">
        <div class="prog" class:indeterminate={progress === null}><i style:width={progress === null ? null : `${progress * 100}%`}></i></div>
        <div class="kv">
          <span><b>{count(running.files)}</b> {t('files')}</span>
          <span><b>{size(running.bytes)}</b> {t('seen')}</span>
          <span><b>{Math.round((Date.now() - Date.parse(running.started_at)) / 1000)} s</b></span>
          <span class="live mono" title={running.current}>{running.current}</span>
        </div>
        {#if isMac && !scan}<div class="hint">{t('macOS may ask whether Prism can read Documents, Desktop, Downloads or Photos: the scan waits for the answer. Full Disk Access (System Settings › Privacy & Security) avoids it.')}</div>{/if}
      </div>
    {/if}
    {#if failed || error}<p class="err" title={failed ?? error}>{failed ?? error}</p>{/if}
    {#if done}
      <p class="ok-msg">
        {done}
        {#if offerEmpty && bin && hasCore()}
          <button class="act" class:armed={arming.is('empty-trash')} disabled={busy !== null || bin.emptying || (bin.denied && !bin.bins.some((b) => b.items > 0))} onclick={emptyTrash}>{arming.is('empty-trash') ? t('Sure? Everything in the Trash deleted') : t('Empty the Trash')}</button>
        {/if}
      </p>
    {/if}

    {#if !scan}
      {#if !running}
        <div class="empty start">
          <span>{t('No scan yet. A scan reads the disk once, then nothing is watched until the next one.')}</span>
          {#if hasCore()}
            <span class="roots">
              {#each roots as r (r)}<button class="act" onclick={() => startScan((root = r))}>{t('Scan {root}', { root: r })}</button>{/each}
              {#if canPick()}<button class="act" onclick={chooseRoot}>{t('Choose…')}</button>{/if}
            </span>
          {/if}
        </div>
      {/if}
    {:else}
      <div class="result" class:stale={!!running}>
        {#if strip && !short}
          <div class="strip" title={t('{disk}: {scanned} in {root}, {other} elsewhere, {free} free', { disk: strip.d.name, scanned: size(strip.scanned), root: scan.root, other: size(strip.other), free: size(strip.free) })}>
            <i style:width="{(100 * strip.scanned) / strip.d.total}%" style:background="var(--accent)"></i>
            <i style:width="{(100 * strip.other) / strip.d.total}%" style:background="var(--text-muted)"></i>
          </div>
          <div class="legend">
            <span><i class="dot" style:background="var(--accent)"></i>{scan.root} <b>{size(strip.scanned)}</b></span>
            <span><i class="dot" style:background="var(--text-muted)"></i>{t('rest of the disk')} <b>{size(strip.other)}</b></span>
            <span><i class="dot free"></i>{t('free')} <b>{size(strip.free)}</b></span>
          </div>
        {/if}

        <div class="bar">
          <span class="crumbs">
            {#each trail as c, i (c.id)}<button onclick={() => up(i)}>{c.name}</button><i>›</i>{/each}
            <b>{view?.name ?? scan.root}</b>
            {#if view}<span class="muted">· {size(view.bytes)}</span>{/if}
          </span>
          {#if !short}
            <span class="sp"></span>
            <span class="seg wide-only" role="radiogroup" aria-label={t('Color')}>
              <button role="radio" aria-checked={colorBy === 'kind'} class:on={colorBy === 'kind'} onclick={() => (colorBy = 'kind')}>{t('by type')}</button>
              <button role="radio" aria-checked={colorBy === 'folder'} class:on={colorBy === 'folder'} onclick={() => (colorBy = 'folder')}>{t('by folder')}</button>
            </span>
          {/if}
        </div>

        {#if view}
          <div class="split" class:short>
            <div class="list" role="tree">
              <div class="tr head" class:delta={hasDelta}>
                <span>{t('Name')}</span><span></span><span class="n">{t('Size')}</span><span class="n">%</span>
                {#if hasDelta}<span class="n" title={t('Since the scan of {date}', { date: ago(scan.previous_at ?? '') })}>Δ</span>{:else}<span class="n">{t('Files')}</span>{/if}
              </div>
              {#each short ? rows.filter((r) => r.depth === 0).slice(0, 6) : rows as r, i (r.n?.id ?? `rest-${i}`)}
                {#if r.n}
                  {@const n = r.n}
                  <div
                    class="tr"
                    class:delta={hasDelta}
                    class:sel={selected?.id === n.id}
                    role="treeitem"
                    aria-selected={selected?.id === n.id}
                    aria-expanded={n.dir ? opened.has(n.id) : undefined}
                    tabindex="-1"
                    onclick={() => (selected = n)}
                    ondblclick={() => enter(n)}
                    oncontextmenu={(e) => openMenu(e, n)}
                    onkeydown={(e) => e.key === 'Enter' && enter(n)}
                  >
                    <span class="nm" style:padding-left="{4 + r.depth * 13}px">
                      {#if n.dir && !short}<button class="chev" onclick={(e) => { e.stopPropagation(); void toggle(n) }} aria-label={opened.has(n.id) ? t('Fold') : t('Unfold')}>{opened.has(n.id) ? '▾' : '▸'}</button>{:else}<span class="chev"></span>{/if}
                      <i class="dot" style:background={colorOf(n, r.top, r.depth)}></i>
                      <span class="t" title={n.path}>{n.name}</span>
                      {#if n.badge}<span class="badge">{n.badge === 'repo' ? t('repository') : t('worktree')}</span>{/if}
                    </span>
                    <span class="b"><i style:width="{view.bytes ? (100 * n.bytes) / view.bytes : 0}%" style:background={colorOf(n, r.top, r.depth)}></i></span>
                    <span class="n mono">{size(n.bytes)}</span>
                    <span class="n mono muted">{share(n.bytes, view.bytes)}</span>
                    {#if hasDelta}<span class="n mono" class:up={(n.delta ?? 0) > 0} class:down={(n.delta ?? 0) < 0}>{deltaText(n.delta)}</span>{:else}<span class="n mono muted">{n.dir ? count(n.files) : ''}</span>{/if}
                  </div>
                {:else if r.rest && !short}
                  <div class="tr rest" class:delta={hasDelta}>
                    <span class="nm muted" style:padding-left="{4 + r.depth * 13 + 16}px">{tn(r.rest.count, '{n} smaller item', '{n} smaller items')}</span>
                    <span></span><span class="n mono muted">{size(r.rest.bytes)}</span><span></span><span></span>
                  </div>
                {/if}
              {/each}
            </div>
            {#if !short}
              <div class="map">
                <Treemap root={view} color={colorOf} selected={selected?.id ?? null} onpick={(n) => (selected = n)} onenter={enter} onhover={(n) => (hovered = n)} oncontext={openMenu} />
              </div>
            {/if}
          </div>
        {/if}

        {#if !short}
          <div class="status">
            {#if focus}
              <span class="path mono" title={focus.path}>{focus.path}{focus.dir ? '/' : ''}</span>
              <span class="mono">{size(focus.bytes)} · {share(focus.bytes, view?.bytes ?? scan.bytes)}{focus.kind !== 'other' ? ` · ${KINDS[focus.kind].label}` : ''}{deltaText(focus.delta) ? ` · ${deltaText(focus.delta)}` : ''}</span>
              {#if selected && !hovered}
                <span class="acts">
                  {#each actions(selected) as a (a.key)}
                    <button class="icon" class:armed={a.key === 'trash' && arming.is(`trash:${selected.id}`)} disabled={busy !== null} onclick={a.run} title={a.label} aria-label={a.label}><Icon name={a.icon} /></button>
                  {/each}
                </span>
              {/if}
            {:else}
              <span class="path muted">{t('Click to select, double-click to open a folder, right-click for its actions.')}</span>
            {/if}
          </div>
          <div class="info muted">
            {t('Scanned {time}', { time: ago(scan.at) })} · {tn(scan.files, '{n} file', '{n} files')} · {(scan.duration_ms / 1000).toFixed(0)} s{#if scan.saved}{' '}· {t('read back from the saved scan: rescan for the details')}{/if}
            {#if scan.denied}
              {' '}· <span class="warn" title={scan.denied_paths.join('\n')}>{tn(scan.denied, '{n} folder not readable', '{n} folders not readable')}</span>{#if isMac}{' '}({t('Full Disk Access, in System Settings › Privacy & Security')}){/if}
            {/if}
          </div>
        {/if}

        {#if scan.reclaim.length && !short}
          <div class="reclaim">
            <h3>{t('Reclaimable')} <span class="muted">{t('{size} in all', { size: size(scan.reclaim.reduce((s, g) => s + g.bytes, 0)) })}</span></h3>
            <div class="cards">
              {#each scan.reclaim as g (g.kind)}
                <div class="card">
                  <b>{RECLAIM[g.kind].label}{#if g.count > 1}{' '}<span class="pill">×{g.count}</span>{/if}</b>
                  <span class="sz mono">{size(g.bytes)}</span>
                  <span class="how">{RECLAIM[g.kind].how}</span>
                  <span class="acts">
                    <button class="act" onclick={() => show(g.first)}>{t('Show')}</button>
                    {#if du.cleanup && hasCore()}
                      <button class="act" class:armed={arming.is(`clean:${g.kind}`)} disabled={busy !== null || !g.available || !!running} title={g.available ? RECLAIM[g.kind].how : t('Its tool is not installed')} onclick={() => clean(g.kind, g.bytes)}>
                        {busy === `clean:${g.kind}` ? '…' : arming.is(`clean:${g.kind}`) ? t('Sure? {size}', { size: size(g.bytes) }) : RECLAIM[g.kind].act}
                      </button>
                    {/if}
                  </span>
                </div>
              {/each}
            </div>
          </div>
        {:else if short && scan.reclaim.length}
          <div class="srow muted">{t('{size} reclaimable', { size: size(scan.reclaim.reduce((s, g) => s + g.bytes, 0)) })}</div>
        {/if}
      </div>
    {/if}
  {/if}
  </div>

  {#if menu}
    <div class="du-menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
      <div class="mh mono">{menu.n.name} · {size(menu.n.bytes)}</div>
      {#each actions(menu.n) as a (a.key)}
        <button role="menuitem" class:danger={a.danger} class:armed={a.key === 'trash' && arming.is(`trash:${menu.n.id}`)} onclick={() => { a.run(); if (a.key !== 'trash') menu = null }}><Icon name={a.icon} />{a.label}</button>
      {/each}
    </div>
  {/if}
</section>

<style>
  .du { container-type: inline-size; }
  .head { display: flex; gap: 6px; align-items: center; margin-left: auto; }
  .root { max-width: 190px; height: 22px; padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  .act { display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  .act :global(svg) { width: 12px; height: 12px; }
  .act.pri { border-color: var(--accent); color: var(--accent); }
  .act:hover:not(:disabled) { border-color: var(--accent); }
  .act.armed, .icon.armed { border-color: var(--ko); color: var(--ko); }
  .act:disabled, .icon:disabled { opacity: 0.5; cursor: default; }
  .err { margin: 0; padding: 4px 12px; color: var(--ko); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ok-msg { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 0; padding: 4px 12px; color: var(--ok); font-size: var(--fs-meta); }
  .trash { display: flex; align-items: center; gap: 6px; min-width: 0; padding: 5px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); }
  .trash :global(svg) { width: 13px; height: 13px; color: var(--text-muted); flex: none; }
  .tlabel { padding: 0; border: none; background: none; color: var(--text); font: inherit; font-weight: 550; cursor: pointer; }
  .tlabel:disabled { cursor: default; }
  .trash .disks { cursor: help; text-decoration: underline dotted; text-underline-offset: 2px; }
  .trash > span { white-space: nowrap; }
  .trash .warn { overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .warn { color: var(--warn); }
  .muted { color: var(--text-muted); }

  .scanning { display: grid; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .prog { height: 5px; border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .prog i { display: block; height: 100%; background: var(--accent); border-radius: var(--r3); transition: width 0.4s; }
  .prog.indeterminate i { width: 30%; animation: du-slide 1.6s ease-in-out infinite alternate; }
  @keyframes du-slide { from { margin-left: 0 } to { margin-left: 70% } }
  .kv { display: flex; gap: 14px; align-items: baseline; font-size: var(--fs-meta); color: var(--text-muted); min-width: 0; }
  .kv b { color: var(--text); font-weight: 600; font-variant-numeric: tabular-nums; }
  .live { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-tag); }
  .hint { font-size: var(--fs-tag); color: var(--text-muted); }
  .start { display: grid; gap: 10px; justify-items: center; }
  .roots { display: flex; gap: 6px; flex-wrap: wrap; justify-content: center; }
  .result.stale { opacity: 0.55; }

  .strip { display: flex; height: 7px; margin: 8px 12px 3px; border-radius: var(--r3); overflow: hidden; background: var(--surface-muted); }
  .strip i { display: block; height: 100%; }
  .legend { display: flex; gap: 12px; flex-wrap: wrap; padding: 0 12px 6px; font-size: var(--fs-tag); color: var(--text-muted); }
  .legend b { color: var(--text); font-weight: 600; }
  .dot { display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 2px; margin-right: 4px; }
  .dot.free { border: 1px solid var(--border); }

  .bar { display: flex; align-items: center; gap: 8px; padding: 4px 12px; border-top: 1px solid var(--border); border-bottom: 1px solid var(--border); font-size: var(--fs-meta); }
  .crumbs { display: flex; align-items: center; gap: 2px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .crumbs button { padding: 1px 5px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: inherit; cursor: pointer; }
  .crumbs button:hover { background: var(--surface-muted); }
  .crumbs i { font-style: normal; color: var(--text-muted); }
  .crumbs b { padding: 1px 5px; }
  .sp { flex: 1; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 0 8px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .seg button.on { background: color-mix(in srgb, var(--src) 18%, transparent); color: var(--text); }

  .split { display: grid; grid-template-columns: minmax(300px, 42%) 1fr; height: 420px; }
  .split.short { grid-template-columns: 1fr; height: auto; }
  .list { overflow: auto; border-right: 1px solid var(--border); }
  .split.short .list { border-right: none; }
  .map { padding: 6px; min-width: 0; }
  .tr { display: grid; grid-template-columns: minmax(0, 1fr) 56px 58px 40px 52px; gap: 6px; align-items: center; height: 22px; padding-right: 8px; font-size: var(--fs-meta); cursor: default; }
  .tr:hover { background: var(--surface-muted); }
  .tr.sel { background: color-mix(in srgb, var(--accent) 16%, transparent); }
  .tr.head { position: sticky; top: 0; z-index: 1; background: var(--surface); color: var(--text-muted); font-size: var(--fs-tag); border-bottom: 1px solid var(--border); }
  .tr.head > span:first-child { padding-left: 8px; }
  .tr.rest:hover { background: none; }
  .nm { display: flex; align-items: center; gap: 3px; min-width: 0; white-space: nowrap; overflow: hidden; }
  .chev { flex: none; width: 13px; padding: 0; border: none; background: none; color: var(--text-muted); font-size: 9px; cursor: pointer; }
  .t { overflow: hidden; text-overflow: ellipsis; }
  .badge { flex: none; font-size: 9.5px; padding: 0 5px; border-radius: var(--r-chip, 7px); background: var(--surface-muted); color: var(--text-muted); }
  .b { height: 6px; border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .b i { display: block; height: 100%; }
  .n { text-align: right; font-size: var(--fs-tag); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .up { color: var(--ko); }
  .down { color: var(--ok); }

  .status { display: flex; align-items: center; gap: 10px; min-height: 28px; padding: 3px 12px; border-top: 1px solid var(--border); font-size: var(--fs-meta); }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-tag); }
  .acts { display: flex; gap: 2px; justify-content: flex-end; }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; padding: 0; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { color: var(--text); border-color: var(--border); }
  .info { padding: 0 12px 6px; font-size: var(--fs-tag); }

  .reclaim { padding: 8px 12px 10px; border-top: 1px solid var(--border); }
  .reclaim h3 { display: flex; gap: 8px; margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .reclaim h3 .muted { margin-left: auto; text-transform: none; letter-spacing: 0; font-weight: 400; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 6px; }
  .card { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 8px; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r7, 7px); }
  .card b { font-size: var(--fs-meta); }
  .sz { font-size: var(--fs-meta); font-weight: 600; text-align: right; }
  .how { font-size: var(--fs-tag); color: var(--text-muted); }
  .card .acts { gap: 4px; align-self: end; }
  .srow { padding: 4px 12px 6px; font-size: var(--fs-meta); }

  .du-menu { position: fixed; z-index: 50; min-width: 220px; padding: 4px; border: 1px solid var(--border); border-radius: var(--r8, 8px); background: var(--surface); box-shadow: 0 8px 28px rgb(0 0 0 / 0.25); font-size: var(--fs-meta); }
  .du-menu .mh { padding: 3px 9px 5px; color: var(--text-muted); font-size: var(--fs-tag); border-bottom: 1px solid var(--border); margin-bottom: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 320px; }
  .du-menu button { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 9px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .du-menu button :global(svg) { width: 13px; height: 13px; color: var(--text-muted); }
  .du-menu button:hover { background: var(--surface-muted); }
  .du-menu button.danger { color: var(--ko); }
  .du-menu button.armed { background: color-mix(in srgb, var(--ko) 14%, transparent); }

  /* Narrow (a phone, a thin column): the list only, full width. */
  @container (max-width: 620px) {
    .split { grid-template-columns: 1fr; height: auto; max-height: 460px; }
    .list { border-right: none; }
    .map, .wide-only { display: none; }
    .tr { grid-template-columns: minmax(0, 1fr) 40px 56px 0 48px; }
    .tr > :nth-child(4) { display: none; }
  }
</style>
