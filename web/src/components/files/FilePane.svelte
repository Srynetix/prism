<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import { EditorView } from '@codemirror/view'
  import { isDesktop } from '../../bridge'
  import Icon from '../Icon.svelte'
  import Markdown from '../Markdown.svelte'
  import { agentsSettings, openAgentFile } from '../../agents.svelte'
  import { attachView, agentWorking, bufferOf, compare, detachView, endCompare, ensureLoaded, keepMine, keyOf, metas, saveBuffer, setEditing, takeTheirs } from '../../files-editor.svelte'
  import { hunks } from '../../editor/git-gutter'
  import { languageName } from '../../editor/languages'
  import { symbolsAt } from '../../editor/symbols'
  import { ask, quote, refOf, type Passage } from '../../editor/ask'
  import { promptAgent } from '../../note-send'
  import { draftOf, setDraft } from '../../chat-drafts.svelte'
  import { addNote } from '../../notes.svelte'
  import { agentRepo } from '../../repositories'
  import { pref, setPref } from '../../layout.svelte'
  import { live } from '../../live.svelte'
  import { copy, toast } from '../../toast.svelte'
  import { t, tn } from '../../i18n.svelte'
  import type { Agent } from '../../types'

  /**
   * One file of the Files tab, in its editor (CodeMirror): its header (where it is, its symbol,
   * its changes against HEAD, its actions), the editor or its preview (Markdown rendered, an
   * image…), its status line. Lines selected: handed to the agent (Ask agent, Add to prompt) or
   * to a note. The agent changed it under an edit: a banner to compare, take its version or keep
   * yours. A second pane shows another file beside it.
   */
  let { agent, path, status, onweb, ondiff, ondownload, onfocus }: {
    agent: Agent
    path: string
    /** Its git status in the list (`M`, `A`, `?`…). */
    status: string | null
    onweb: (path: string) => void
    ondiff: () => void
    ondownload: (path: string) => void
    onfocus?: () => void
  } = $props()

  const STATUS: Record<string, string> = { M: t('Modified'), A: t('Added'), D: t('Deleted'), R: t('Renamed'), '?': t('Untracked') }
  const isMd = (p: string) => /\.(md|mdx|markdown)$/i.test(p)
  const isHtml = (p: string) => /\.html?$/i.test(p)
  const WEB = /\.(html?|svg|pdf)$/i
  const size = (n: number) => (n < 1024 ? `${n} B` : n < 1 << 20 ? `${Math.round(n / 1024)} KB` : `${(n / (1 << 20)).toFixed(1)} MB`)
  const folderOf = (p: string) => p.slice(0, p.lastIndexOf('/') + 1)
  const phone = !isDesktop()
  /** The Mac's editor: not from a phone, which cannot open an app there. */
  const editor = $derived(isDesktop() ? live.snapshot?.open_with?.editor : undefined)

  const key = $derived(keyOf(agent.id, path))
  const meta = $derived(metas[key])
  // Read again as its text, cursor or selection move.
  const buf = $derived(meta && meta.version >= 0 ? bufferOf(agent.id, path) : undefined)
  const preview = $derived(buf?.preview ?? null)
  const est = $derived(meta && meta.version >= 0 ? buf?.state ?? null : null)
  const working = $derived(agentWorking(agent.id))

  let mdSource = $state(pref<boolean>('agents.side.mdSource', false))
  const setMdSource = (on: boolean) => setPref('agents.side.mdSource', (mdSource = on))
  const rendered = $derived(preview?.kind === 'text' && isMd(path) && !mdSource && !meta?.comparing)

  $effect(() => {
    void ensureLoaded(agent.id, path)
  })

  /* ---------- The editor ---------- */

  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | null = null
  let shownKey: string | null = null
  $effect(() => {
    const k = key
    const ready = !!est && !meta?.loading
    if (!host || !ready) return
    untrack(() => {
      const b = bufferOf(agent.id, path)
      if (!b?.state) return
      if (!view) view = new EditorView({ state: b.state, parent: host })
      if (shownKey !== k) {
        attachView(k, view, shownKey)
        shownKey = k
        asking = false
      }
    })
  })
  onDestroy(() => {
    if (view && shownKey) detachView(shownKey, view)
    view?.destroy()
  })

  /* ---------- Where the cursor is ---------- */

  const head = $derived(est?.selection.main.head ?? 0)
  const line = $derived(est ? est.doc.lineAt(head) : null)
  const crumbs = $derived(est && !rendered ? symbolsAt(est, head) : [])
  /** Its changes against the last commit, as the gutter marks them. */
  const changes = $derived({ count: est ? hunks(est).length : 0 })

  /* ---------- Lines selected ---------- */

  const passage = $derived.by((): Passage | null => {
    if (!est) return null
    const sel = est.selection.main
    if (sel.empty) return null
    const doc = est.doc
    const first = doc.lineAt(sel.from)
    let last = doc.lineAt(sel.to)
    // Up to the start of a line: that line is not taken.
    if (sel.to === last.from && last.number > first.number) last = doc.line(last.number - 1)
    return { path, from: first.number, to: last.number, text: doc.sliceString(first.from, last.to) }
  })
  let asking = $state(false)
  let askField: HTMLInputElement | undefined = $state()
  $effect(() => askField?.focus())
  let question = $state('')
  let paneEl: HTMLDivElement | undefined = $state()
  /** Where its chip goes: under the selection, inside the pane. */
  const chipAt = $derived.by(() => {
    if (!passage || !view || !paneEl || rendered) return null
    const c = view.coordsAtPos(est!.selection.main.head)
    if (!c) return null
    const box = paneEl.getBoundingClientRect()
    const top = Math.min(box.height - 40, Math.max(30, c.bottom - box.top + 6))
    const left = Math.max(6, Math.min(box.width - 330, c.left - box.left - 40))
    return { top, left }
  })
  const lang = $derived(agentsSettings().prompt_language)
  const name = $derived(agent.title || agent.id)
  async function sendAsk() {
    if (!passage || !question.trim()) return
    if (await promptAgent(agent, ask(passage, question, lang))) {
      question = ''
      asking = false
    }
  }
  function addToPrompt() {
    if (!passage) return
    const now = draftOf(agent.id)
    setDraft(agent.id, `${now ? `${now.replace(/\s+$/, '')}\n\n` : ''}${quote(passage, lang)}\n\n`)
    toast(t('Added to the prompt of {name} (Chat)', { name }), 3500)
  }
  function toNote() {
    if (!passage) return
    addNote(quote(passage, lang), 'sticky', { status: 'todo', repo: agentRepo(agent) })
    toast(t('Note added (TODO)'), 3000)
  }
  const keep = (e: MouseEvent) => e.preventDefault()
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="pane" bind:this={paneEl} onfocusin={onfocus} onpointerdown={onfocus}>
  <div class="ph">
    <span class="file" title={path}><span class="nm">{path.slice(folderOf(path).length)}</span>{#if folderOf(path)}<span class="dir">{folderOf(path)}</span>{/if}</span>
    {#if meta?.dirty}<span class="dot" title={t('Not saved')}></span>{/if}
    {#if status}<span class="pill s{status === '?' ? 'u' : status}">{STATUS[status] ?? status}</span>{/if}
    <span class="grow"></span>
    {#if meta?.dirty && !meta.conflict}<button class="save" onclick={() => void saveBuffer(key)} disabled={meta.saving} title={working ? t('The agent works: saved once its turn is over') : t('Save (⌘S)')}><Icon name="save" />{meta.saving ? t('Saving…') : meta.pending ? t('Waits') : t('Save')}</button>{/if}
    {#if phone && preview?.kind === 'text' && !meta?.editing}<button onclick={() => setEditing(agent.id, path, true)} title={t('Edit it here')}><Icon name="edit" />{t('Edit')}</button>{/if}
    {#if preview?.kind === 'text' && isMd(path)}<button class:on={mdSource} onclick={() => setMdSource(!mdSource)} title={t('Show the Markdown source')}>{t('Source')}</button>{/if}
    {#if WEB.test(path)}<button onclick={() => onweb(path)} title={t('Open it in the Web tab')}><Icon name="web" />Web</button>{/if}
    {#if status}<button onclick={ondiff} title={t('The Diff tab')}><Icon name="diff" />Diff</button>{/if}
    {#if editor}<button onclick={() => void openAgentFile(agent.id, path).catch((e) => toast(String(e), 6000))} title={t('Open it in {editor}', { editor })}><Icon name="code" /></button>{/if}
    <button onclick={() => ondownload(path)} title={t('Download it')}><Icon name="download" /></button>
    <button onclick={() => copy(path, t('{name} copied', { name: path }))} title={t('Copy its path')}><Icon name="copy" /></button>
  </div>
  {#if est && !rendered}
    <div class="crumb">
      {#if crumbs.length}<span class="sym" title={crumbs.join(' › ')}>{crumbs.join(' › ')}</span>{:else}<span class="muted">{tn(est.doc.lines, '{n} line', '{n} lines')}</span>{/if}
      <span class="grow"></span>
      {#if changes.count}<span class="chg" title={t('Against the last commit: click the marks in the gutter to see what changed')}>{tn(changes.count, '{n} change', '{n} changes')} {t('vs HEAD')}</span>{/if}
    </div>
  {:else if preview && preview.kind !== 'text'}
    <div class="crumb"><span class="muted">{size(preview.size)}</span></div>
  {/if}

  {#if meta?.conflict != null}
    <div class="banner w">
      {#if meta.comparing}
        <span>{t('Its version against yours: Accept keeps your change, Reject takes its line.')}</span>
        <span class="grow"></span>
        <button onclick={() => endCompare(key)}>{t('Done')}</button>
      {:else}
        <b>{t('The agent changed this file')}</b><span>{t('while you edited it. Yours is kept.')}</span>
        <span class="grow"></span>
        <button onclick={() => void compare(key)}>{t('Compare')}</button>
        <button onclick={() => takeTheirs(key)}>{t('Take its version')}</button>
        <button onclick={() => keepMine(key)} title={t('Saved over its version')}>{t('Keep mine')}</button>
      {/if}
    </div>
  {:else if meta?.pending}
    <div class="banner"><span>{t('Saved once the agent’s turn is over')}</span><span class="grow"></span><button onclick={() => meta && (meta.pending = false)}>{t('Cancel')}</button></div>
  {/if}
  {#if meta?.error}<p class="err">{meta.error}</p>{/if}

  {#if meta?.loading}
    <p class="muted empty">{t('Reading…')}</p>
  {:else if preview?.kind === 'image'}
    <div class="img"><img src={preview.url} alt={path} /></div>
  {:else if preview?.kind === 'binary'}
    <p class="muted empty">{t('A binary file ({size}): no preview.', { size: size(preview.size) })}</p>
  {:else if preview?.kind === 'too_large'}
    <p class="muted empty">{t('Too large to preview ({size}).', { size: size(preview.size) })}</p>
  {:else if rendered && est}
    <div class="md"><Markdown text={est.doc.toString()} reading /></div>
  {:else if preview?.kind === 'text' && isHtml(path)}
    <p class="hint">{t('An HTML page:')} <button class="link" onclick={() => onweb(path)}>{t('open it in the Web tab')}</button> {t('to see it rendered.')}</p>
  {/if}
  <div class="cm" bind:this={host} class:hidden={!est || rendered || meta?.loading}></div>

  {#if chipAt && passage}
    <div class="chip" style:top="{chipAt.top}px" style:left="{chipAt.left}px">
      {#if asking}
        <input bind:this={askField} bind:value={question} placeholder={t('Ask {name} about lines {lines}…', { name, lines: passage.to > passage.from ? `${passage.from}–${passage.to}` : `${passage.from}` })} onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter') void sendAsk(); else if (e.key === 'Escape') { asking = false; view?.focus() } }} />
        <button class="primary" onmousedown={keep} onclick={sendAsk} disabled={!question.trim()}><Icon name="send" /></button>
      {:else}
        <button class="primary" onmousedown={keep} onclick={() => (asking = true)} title={t('A question to the agent about these lines')}><Icon name="send" />{t('Ask agent')}</button>
        <button onmousedown={keep} onclick={addToPrompt} title={t('Added to its Chat prompt, to send with more')}><Icon name="plus" />{t('Add to prompt')}</button>
        <button onmousedown={keep} onclick={toNote} title={t('A TODO note of this repository, with these lines')}><Icon name="pin" />{t('Note')}</button>
        <button onmousedown={keep} onclick={() => copy(refOf(passage!), t('{name} copied', { name: refOf(passage!) }))} title={t('Copy where these lines are')}><Icon name="copy" />{refOf(passage).slice(path.length + 1)}</button>
      {/if}
    </div>
  {/if}

  {#if est && !rendered}
    <div class="status">
      <span>{t('Ln {line}, Col {col}', { line: line?.number ?? 1, col: head - (line?.from ?? 0) + 1 })}{passage && passage.to > passage.from ? ` · ${tn(passage.to - passage.from + 1, '{n} line selected', '{n} lines selected')}` : ''}</span>
      <span>{languageName(path)}</span>
      <span class="grow"></span>
      {#if phone && !meta?.editing}<span>{t('Read-only')}</span>{/if}
      <span class:busy={working}>{working ? t('Agent working — saves wait') : t('Agent idle — editable')}</span>
    </div>
  {/if}
</div>

<style>
  .pane { position: relative; flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .ph { display: flex; align-items: center; gap: 5px; padding: 5px 8px 3px 10px; font-size: var(--fs-meta); }
  .ph button, .banner button { display: inline-flex; align-items: center; gap: 4px; padding: 1px 6px; border: 1px solid transparent; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .ph button:hover { border-color: var(--border); color: var(--text); }
  .ph button.on { border-color: var(--src-claude); color: var(--src-claude); }
  .ph button.save { border-color: var(--src-claude); color: var(--src-claude); }
  .ph button:disabled { opacity: 0.5; cursor: default; }
  .file { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
  .file .nm { font-weight: 600; white-space: nowrap; }
  .file .dir { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: var(--fs-tag) var(--mono); color: var(--text-muted); direction: rtl; text-align: left; }
  .dot { flex-shrink: 0; width: 7px; height: 7px; border-radius: 50%; background: var(--text); }
  .pill { flex-shrink: 0; padding: 0 6px; border-radius: 9px; font-size: var(--fs-tag); font-weight: 600; background: var(--surface-muted); }
  .pill.sM, .pill.sR { background: var(--warn-bg); }
  .pill.sA, .pill.su { background: var(--ok-bg); }
  .pill.sD { background: var(--ko-bg); }
  .crumb { display: flex; align-items: center; gap: 6px; padding: 0 10px 4px; border-bottom: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; min-width: 0; }
  .crumb .sym { overflow: hidden; text-overflow: ellipsis; font-family: var(--mono); color: var(--text); }
  .crumb .chg { color: var(--warn); }
  .banner { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; padding: 5px 10px; background: var(--info-bg); color: var(--info); font-size: var(--fs-meta); border-bottom: 1px solid var(--border); }
  .banner.w { background: var(--warn-bg); color: var(--warn); }
  .banner button { border-color: currentColor; color: inherit; }
  .err { margin: 0; padding: 6px 10px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .empty { padding: 12px; font-size: var(--fs-meta); }
  .grow { flex: 1; }
  .cm { flex: 1; min-height: 0; overflow: hidden; }
  .cm.hidden { display: none; }
  .md { flex: 1; min-height: 0; overflow: auto; padding: 14px 20px 22px; }
  .img { flex: 1; min-height: 0; display: grid; place-items: center; padding: 10px; overflow: auto; background: repeating-conic-gradient(var(--surface-muted) 0 25%, var(--surface) 0 50%) 0 0 / 16px 16px; }
  .img img { max-width: 100%; max-height: 100%; object-fit: contain; }
  .hint { margin: 0; padding: 5px 10px; font-size: var(--fs-meta); color: var(--text-muted); border-bottom: 1px solid var(--border); }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .status { display: flex; align-items: center; gap: 12px; padding: 2px 10px; border-top: 1px solid var(--border); background: var(--surface-muted); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; }
  .status .busy { color: var(--warn); font-weight: 600; }
  .chip { position: absolute; z-index: 5; display: flex; align-items: center; gap: 2px; padding: 3px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: 0 6px 18px rgba(0, 0, 0, 0.16); font-size: var(--fs-meta); }
  .chip button { display: inline-flex; align-items: center; gap: 4px; padding: 2px 7px; border: 1px solid transparent; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; cursor: pointer; white-space: nowrap; }
  .chip button:hover { color: var(--text); border-color: var(--border); }
  .chip button.primary { background: color-mix(in srgb, var(--src-claude) 85%, #000); color: #fff; }
  .chip button:disabled { opacity: 0.5; }
  .chip input { width: 260px; padding: 3px 8px; border: 1px solid var(--src-claude); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; outline: none; }
  :global(html.remote) .chip input { font-size: 16px; }
  :global(html.remote) .chip button { min-height: 32px; }
</style>
