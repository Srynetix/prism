<script lang="ts">
  /**
   * A container's files (`container-files.svelte.ts`): the folder's path as crumbs (a path typed and
   * Enter goes there), its entries, folders first; a text file shown below them. ⬇ downloads a file,
   * or a folder as a `.tar.gz`; Send… and files dropped on it send them into the open folder, when
   * its block allows it. Into prod, or replacing files, a send waits for its Send button. A text
   * file shown is edited there too (Edit, `ContainerFileEditor.svelte`): into prod, its changes are
   * shown before they are written; changed in the container meanwhile, compared or overwritten.
   */
  import Drawer from './ui/Drawer.svelte'
  import State from './ui/State.svelte'
  import Icon from './Icon.svelte'
  import ContainerFileEditor from './ContainerFileEditor.svelte'
  import { armer } from '../armed.svelte'
  import { t, tn } from '../i18n.svelte'
  import { ago } from '../time'
  import { cancelEdit, cancelSend, closeContainerFiles, compareConflict, confirmSend, containerFiles as cf, crumbs, download, dropFiles, editable, editDirty, go as goNow, joinPath, overwriteConflict, parentPath, pickAndSend, reloadFolder as reloadNow, saveEdit, show as showNow, startEdit, takeTheirs } from '../container-files.svelte'
  import { highlightText, type Token } from '../highlight'
  import type { ContainerEntry } from '../bindings/ContainerEntry'
  import type { ContainerFileText } from '../bindings/ContainerFileText'

  const phone = document.documentElement.classList.contains('remote')
  const armed = armer()

  /** Unsaved changes are left only at a second click (Tauri's webview has no `confirm()`). */
  function leave(key: string): boolean {
    if (!editDirty()) return true
    if (armed.confirm(key)) return true
    cf.note = { ok: false, text: t('Unsaved changes: click again to leave them') }
    return false
  }
  const go = (p: string) => leave(`go:${p}`) && goNow(p)
  const show = (p: string) => leave(`show:${p}`) && showNow(p)
  const reloadFolder = () => leave('reload') && reloadNow()
  const close = () => leave('close') && closeContainerFiles()
  const closeFile = () => leave('close-file') && ((cf.edit = null), (cf.file = null))
  const cancel = () => leave('cancel') && cancelEdit()
  const listing = $derived(cf.listing)
  let query = $state('')
  let typed = $state<string | null>(null)
  let dragging = $state(false)

  const entries = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return (listing?.entries ?? []).filter((e) => !q || e.name.toLowerCase().includes(q))
  })
  const path = $derived(listing?.path ?? cf.path)

  /** The file shown, highlighted (highlight.ts) once its grammar is read: its text plain until then. */
  let colored = $state<{ of: ContainerFileText; lines: Token[][] } | null>(null)
  $effect(() => {
    const f = cf.file
    const of = f?.text
    if (!f || !of?.text || of.binary) return
    highlightText(f.path, of.text).then(
      (lines) => { if (lines && cf.file?.text === of) colored = { of, lines } },
      (e) => console.warn('highlight', f.path, e),
    )
  })

  const size = (b: number | null | undefined) => (b === null || b === undefined ? '' : b >= 1e9 ? t('{n} GB', { n: (b / 1e9).toFixed(1) }) : b >= 1e6 ? t('{n} MB', { n: (b / 1e6).toFixed(1) }) : b >= 1e3 ? t('{n} kB', { n: Math.round(b / 1e3) }) : t('{n} B', { n: b }))

  function open(e: ContainerEntry) {
    const at = joinPath(path, e.name)
    if (e.dir) {
      query = ''
      void go(at)
    } else void show(at)
  }

  function submitPath(e: KeyboardEvent) {
    if (e.key === 'Enter' && typed !== null) {
      e.preventDefault()
      const p = typed.trim()
      typed = null
      if (p) void go(p.startsWith('/') ? p : joinPath(path, p))
    } else if (e.key === 'Escape' && typed !== null) {
      e.preventDefault()
      e.stopPropagation()
      typed = null
    }
  }

  /** The path typed: downloaded as it is (a minimal image, nothing listed). */
  const typedPath = $derived(typed?.trim().startsWith('/') ? typed.trim() : '')

  function onDrop(e: DragEvent) {
    dragging = false
    const files = [...(e.dataTransfer?.files ?? [])]
    if (!cf.write || !files.length) return
    e.preventDefault()
    dropFiles(files)
  }
  function onDragOver(e: DragEvent) {
    if (!cf.write || !e.dataTransfer?.types.includes('Files')) return
    e.preventDefault()
    dragging = true
  }
</script>

<Drawer open={cf.open && !!cf.target} key="container-files" identity={cf.label} crumb={cf.label} label={t('Container files')} size={phone ? 'full' : 'l'} source={cf.target?.kind === 'pod' ? 'var(--src-k8s)' : 'var(--info)'} class="cfiles" onclose={close}>
  {#snippet head()}
    <Icon name="folder" />
    <span class="what" title={cf.label}>{cf.label}</span>
    {#if cf.prod}<span class="prod" title={t('A production environment: sending files asks again')}>prod</span>{/if}
    <span class="grow"></span>
    <input class="filter" bind:value={query} onkeydown={(e) => { if (e.key === 'Escape' && query) { e.preventDefault(); e.stopPropagation(); query = '' } }} placeholder={t('Filter…')} spellcheck="false" />
    {#if cf.write && listing}<button class="btn sm" onclick={() => void pickAndSend()} disabled={cf.busy !== null} title={t('Send files of this computer into {dir}', { dir: path })}><Icon name="upload" /> {t('Send…')}</button>{/if}
    {#if listing}<button class="btn sm icon ghost" onclick={() => void download(path, true)} disabled={cf.busy !== null} title={t('Download this folder (.tar.gz)')} aria-label={t('Download this folder')}><Icon name="download" /></button>{/if}
    <button class="btn sm icon ghost" onclick={() => void reloadFolder()} disabled={cf.loading} title={t('Reload the folder')} aria-label={t('Reload')}><Icon name="refresh" /></button>
  {/snippet}
  <div class="body" class:dragging role="region" aria-label={t('Container files')} ondragover={onDragOver} ondragleave={() => (dragging = false)} ondrop={onDrop}>
    <div class="bar">
      <button class="btn sm icon ghost" onclick={() => void go(parentPath(path))} disabled={!listing || path === '/'} title={t('Parent folder')} aria-label={t('Parent folder')}><Icon name="up" /></button>
      {#if typed === null}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="crumbs mono" ondblclick={() => (typed = path)}>
          {#each crumbs(path) as c, i (c.path)}
            {#if i > 1}<span class="sep">/</span>{/if}
            <button class="crumb" onclick={() => void go(c.path)}>{c.name}</button>
          {/each}
        </span>
        <button class="btn sm icon ghost" onclick={() => (typed = path)} title={t('Type a path')} aria-label={t('Type a path')}><Icon name="edit" /></button>
      {:else}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="path mono" bind:value={typed} onkeydown={submitPath} onblur={() => { if (!typedPath) typed = null }} autofocus spellcheck="false" aria-label={t('Path in the container')} />
        {#if typedPath}<button class="btn sm" onmousedown={(e) => e.preventDefault()} onclick={() => { const p = typedPath; typed = null; void download(p, false) }} disabled={cf.busy !== null} title={t('Download this file, even from a container whose files cannot be listed')}><Icon name="download" /> {t('Download')}</button>{/if}
      {/if}
    </div>

    {#if cf.pending}
      {@const p = cf.pending}
      {@const n = p.paths.length + p.files.length}
      <div class="confirm" role="alert">
        <span>
          {tn(n, 'Send {n} file into {dir}?', 'Send {n} files into {dir}?', { dir: path })}
          {#if p.replaced.length}<b>{t('Replaces {names}.', { names: p.replaced.join(', ') })}</b>{/if}
          {#if cf.prod}<b>{t('This is production.')}</b>{/if}
        </span>
        <button class="btn sm primary" onclick={() => void confirmSend()}><Icon name="upload" /> {t('Send')}</button>
        <button class="btn sm ghost" onclick={cancelSend}>{t('Cancel')}</button>
      </div>
    {/if}
    {#if cf.note}<p class="note" class:ko={!cf.note.ok}>{cf.note.text}</p>{/if}

    <div class="list" class:split={!!cf.file}>
      {#if cf.loading && !listing}
        <State kind="loading" lines={6} />
      {:else if cf.error}
        <State kind="error" title={cf.error} detail={t('Type a file’s path (✎) to download it all the same.')} retry={() => void reloadFolder()} />
      {:else if listing}
        <ul class="entries" aria-label={t('Entries of {dir}', { dir: path })}>
          {#each entries as e (e.name)}
            {@const at = joinPath(path, e.name)}
            <li class:on={cf.file?.path === at}>
              <button class="entry" onclick={() => open(e)} title={e.link ? `${e.name} → ${e.link}` : e.name}>
                <span class="i" class:dir={e.dir}><Icon name={e.dir ? 'folder' : 'files'} /></span>
                <span class="n mono">{e.name}{#if e.link}<span class="muted"> → {e.link}</span>{/if}</span>
                <span class="s mono muted">{e.dir ? '' : size(e.size)}</span>
                <span class="d muted">{e.modified ? ago(e.modified) : ''}</span>
              </button>
              <button class="btn sm icon ghost dl" onclick={() => void download(at, e.dir)} disabled={cf.busy !== null} title={e.dir ? t('Download {name} (.tar.gz)', { name: e.name }) : t('Download {name}', { name: e.name })} aria-label={t('Download {name}', { name: e.name })}><Icon name={cf.busy === at ? 'pending' : 'download'} /></button>
            </li>
          {:else}
            <li class="nothing muted">{query ? t('Nothing matches.') : t('Empty folder.')}</li>
          {/each}
          {#if listing.truncated}<li class="nothing muted">{t('Only the first {n} entries are listed.', { n: listing.entries.length })}</li>{/if}
        </ul>
      {/if}
      {#if cf.write && dragging}<div class="drop">{t('Drop to send into {dir}', { dir: path })}</div>{/if}
    </div>

    {#if cf.file}
      {@const f = cf.file}
      {@const e = cf.edit?.path === f.path ? cf.edit : null}
      <section class="file" class:editing={!!e} aria-label={f.path}>
        <header>
          <b class="mono" title={f.path}>{f.path.split('/').pop()}</b>
          {#if f.text?.truncated}<span class="muted">{t('first MB only')}</span>{/if}
          {#if e}{#if editDirty()}<span class="dirty" title={t('Unsaved changes')}>●</span>{/if}{/if}
          <span class="grow"></span>
          {#if e}
            <button class="btn sm" class:armed={armed.is('cancel')} onclick={cancel} disabled={e.saving}>{armed.is('cancel') ? t('Click again to drop the changes') : t('Cancel')}</button>
            {#if !e.review && !e.conflict}<button class="btn sm primary" onclick={() => void saveEdit()} disabled={e.saving || !editDirty()} title={t('Save into the container (⌘S)')}>{e.saving ? t('Saving…') : t('Save')}</button>{/if}
          {:else}
            {#if editable()}<button class="btn sm" onclick={startEdit} disabled={cf.busy !== null} title={t('Edit this file in the container')}><Icon name="edit" /> {t('Edit')}</button>{/if}
            <button class="btn sm" onclick={() => void download(f.path, false)} disabled={cf.busy !== null}><Icon name="download" /> {t('Download')}</button>
          {/if}
          <button class="btn sm icon ghost" onclick={closeFile} title={t('Close')} aria-label={t('Close')}><Icon name="close" /></button>
        </header>
        {#if e?.review}
          <div class="confirm" role="alert">
            <span>{t('This is production: here is what changes.')}</span>
            <button class="btn sm ghost" onclick={() => (e.review = false)} disabled={e.saving}>{t('Back to editing')}</button>
            <button class="btn sm danger" class:armed={armed.is('prod')} onclick={() => armed.confirm('prod') && void saveEdit()} disabled={e.saving}>{e.saving ? t('Saving…') : armed.is('prod') ? t('Click again to save into production') : t('Save into production')}</button>
          </div>
        {:else if e?.conflict}
          <div class="confirm" role="alert">
            <span>{e.conflict.text === null ? t('{name} changed in the container since you opened it, and can no longer be shown.', { name: f.path.split('/').pop() ?? '' }) : t('{name} changed in the container since you opened it.', { name: f.path.split('/').pop() ?? '' })}</span>
            {#if e.conflict.text !== null}
              <button class="btn sm" onclick={compareConflict}>{t('Compare')}</button>
              <button class="btn sm ghost" onclick={takeTheirs}>{t('Take theirs')}</button>
            {/if}
            <button class="btn sm danger" class:armed={armed.is('overwrite')} onclick={() => armed.confirm('overwrite') && overwriteConflict()}>{armed.is('overwrite') ? t('Click again to overwrite') : t('Overwrite')}</button>
          </div>
        {:else if e?.comparing}
          <div class="confirm info">
            <span>{t('What differs from the container’s version: keep or put back each change, then Save.')}</span>
            <button class="btn sm ghost" onclick={() => (e.comparing = false)}>{t('Done comparing')}</button>
          </div>
        {/if}
        {#if e}
          <ContainerFileEditor path={e.path} bind:value={e.draft} against={e.review || e.comparing ? e.original : null} readOnly={e.review || e.saving} onsave={() => void saveEdit()} />
        {:else if f.loading}
          <State kind="loading" lines={4} />
        {:else if f.error}
          <State kind="error" title={f.error} />
        {:else if f.text?.binary}
          <State kind="empty" icon="files" title={t('A binary file: download it to open it.')} />
        {:else}
          {@const lines = colored?.of === f.text ? colored.lines : null}
          <pre class="mono">{#if lines}{#each lines as line, i (i)}{#each line as p, n (n)}<span style:color={p.color} class:it={p.italic} class:bd={p.bold}>{p.text}</span>{/each}{#if i < lines.length - 1}{'\n'}{/if}{/each}{:else}{f.text?.text ?? ''}{/if}</pre>
        {/if}
      </section>
    {/if}
  </div>
</Drawer>

<style>
  .what { min-width: 0; color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .prod { padding: 0 6px; border-radius: var(--r5); background: color-mix(in srgb, var(--ko) 16%, transparent); color: var(--ko); font-size: var(--fs-tag); font-weight: 700; text-transform: uppercase; }
  .grow { flex: 1; }
  .muted, .sep { color: var(--text-muted); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .filter { width: 160px; height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); outline: none; }
  .filter:focus, .path:focus { border-color: var(--info); }
  .body { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .bar { display: flex; align-items: center; gap: 4px; padding: 6px 10px; border-bottom: 1px solid var(--border); background: var(--surface-muted); }
  .crumbs { flex: 1; min-width: 0; display: flex; align-items: center; overflow-x: auto; white-space: nowrap; }
  .crumb { padding: 1px 3px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; cursor: pointer; }
  .crumb:hover { background: color-mix(in srgb, var(--text) 8%, transparent); }
  .path { flex: 1; min-width: 0; height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--bg); color: var(--text); outline: none; }
  .confirm { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--warn) 10%, transparent); font-size: var(--fs-meta); }
  .confirm span { flex: 1; min-width: 0; }
  .confirm b { margin-left: 4px; }
  .note { margin: 0; padding: 6px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); color: var(--ok); }
  .note.ko { color: var(--ko); }
  .list { position: relative; flex: 1; min-height: 0; overflow: auto; }
  .list.split { flex: 1 1 45%; }
  .entries { margin: 0; padding: 4px 0 12px; list-style: none; }
  .entries li { display: flex; align-items: center; padding-right: 8px; }
  .entries li:hover, .entries li.on { background: var(--surface-muted); }
  .entry { flex: 1; min-width: 0; display: grid; grid-template-columns: 16px minmax(0, 1fr) auto auto; align-items: center; column-gap: 8px; padding: 3px 12px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .entry .i { display: inline-flex; color: var(--text-muted); }
  .entry .i.dir { color: var(--info); }
  .entry .n { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .entry .s, .entry .d { white-space: nowrap; font-size: var(--fs-tag); }
  .entry .d { min-width: 64px; text-align: right; }
  .dl { opacity: 0.5; }
  .entries li:hover .dl, .dl:focus-visible { opacity: 1; }
  .nothing { padding: 10px 14px; font-size: var(--fs-meta); }
  .drop { position: absolute; inset: 8px; display: flex; align-items: center; justify-content: center; border: 2px dashed var(--info); border-radius: var(--r8, 8px); background: color-mix(in srgb, var(--info) 10%, var(--bg)); color: var(--text); font-weight: 600; pointer-events: none; }
  .file { flex: 1 1 55%; min-height: 0; display: flex; flex-direction: column; border-top: 1px solid var(--border); }
  .file header { display: flex; align-items: center; gap: 8px; padding: 6px 12px; background: var(--surface-muted); font-size: var(--fs-meta); }
  .file b { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .file.editing { flex-basis: 70%; }
  .dirty { color: var(--warn); font-size: var(--fs-tag); }
  .confirm.info { background: color-mix(in srgb, var(--info) 10%, transparent); }
  .armed { background: var(--ko-bg); color: var(--ko); }
  .file pre .it { font-style: italic; }
  .file pre .bd { font-weight: 600; }
  .file pre { flex: 1; min-height: 0; margin: 0; padding: 8px 14px 16px; overflow: auto; background: var(--bg); color: var(--text); font: 11.5px/1.5 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  @media (max-width: 719.98px) {
    :global(.cfiles .dh) { flex-wrap: wrap; row-gap: 6px; }
    .filter { flex: 1; min-width: 120px; }
    .entry { grid-template-columns: 16px minmax(0, 1fr) auto; }
    .entry .d { display: none; }
    .dl { opacity: 1; }
  }
</style>
