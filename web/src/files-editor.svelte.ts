import { diff as textDiff, unifiedMergeView } from '@codemirror/merge'
import { openSearchPanel, SearchQuery, setSearchQuery } from '@codemirror/search'
import { EditorSelection, EditorState, StateEffect, StateField, Transaction, type TransactionSpec } from '@codemirror/state'
import { Decoration, EditorView, keymap, type DecorationSet } from '@codemirror/view'
import { invoke, isDesktop } from './bridge'
import { agentFilePreview, type FilePreview } from './agents.svelte'
import { committedLines, gitGutter, hunks, kindOf, peekAt, revertChunk, setBase } from './editor/git-gutter'
import { languageOf } from './editor/languages'
import { baseExtensions, compartments, loadVim, prefsEffects, vimApi, type EditorPrefs } from './editor/setup'
import { pref, setPref } from './layout.svelte'
import { live } from './live.svelte'
import { toast } from './toast.svelte'
import { t } from './i18n.svelte'

/**
 * The files open in an agent's Files tab: their tabs, and each one's editor state, kept while the
 * tab or the agent changes (an edit not saved stays). A file is read once, then again only when
 * the agent changes it on disk (`checkStamps`): read again in place when it was not edited here,
 * otherwise kept, the difference offered (`conflict`). Saved as it was read until then
 * (`agent_file_save`'s `original`), never while the agent works: then once its turn is over.
 */

export interface Tab {
  path: string
  /** Kept: opened twice, or edited. A tab not kept is replaced by the next file opened. */
  pinned: boolean
}
interface AgentTabs {
  tabs: Tab[]
  active: string | null
  /** The second pane's file, side by side; null: one pane. */
  split: string | null
  /** Opened lately, latest first: quick open's first lines. */
  recent: string[]
}

export interface Meta {
  loading: boolean
  error: string
  /** Its text differs from the disk's as read. */
  dirty: boolean
  saving: boolean
  /** Saved once the agent's turn is over. */
  pending: boolean
  /** The disk's text, changed by the agent under an edit not saved. */
  conflict: string | null
  /** The Compare view of a conflict is open. */
  comparing: boolean
  /** On a phone: editing (read-only otherwise). */
  editing: boolean
  /** Moves as the text, the cursor or the selection change: what reads them follows. */
  version: number
}

interface Buffer {
  agent: string
  path: string
  preview: FilePreview | null
  /** Its text on disk, as last read or saved. */
  disk: string
  stamp: string
  state: EditorState | null
}

const KEY = 'agents.files.tabs'
const tabsAll = $state<Record<string, AgentTabs>>(pref<Record<string, AgentTabs>>(KEY, {}))
const saveTabs = () => setPref(KEY, $state.snapshot(tabsAll))
export const metas = $state<Record<string, Meta>>({})
const buffers = new Map<string, Buffer>()
/** The editor showing a buffer, if one does. */
const views = new Map<string, EditorView>()

export const keyOf = (agent: string, path: string) => `${agent}\u0000${path}`
const NONE: AgentTabs = { tabs: [], active: null, split: null, recent: [] }
/** Its tabs, to read (never written: `own`). */
export const tabsOf = (agent: string): AgentTabs => tabsAll[agent] ?? NONE
const own = (agent: string): AgentTabs => (tabsAll[agent] ??= { tabs: [], active: null, split: null, recent: [] })
export const metaOf = (agent: string, path: string): Meta | undefined => metas[keyOf(agent, path)]
export const bufferOf = (agent: string, path: string) => buffers.get(keyOf(agent, path))

/* ---------- Settings ---------- */

export const filesSettings = () => live.snapshot?.files ?? { vim: false, wrap: false, font_size: 11.5, live_reload: true, search_max: 2000 }
function prefsFor(key: string): EditorPrefs {
  const s = filesSettings()
  return { vim: s.vim, wrap: s.wrap, fontSize: s.font_size || 11.5, readOnly: !isDesktop() && !metas[key]?.editing }
}

/** Every editor shown brought to the settings as they are now (a setting changed). */
export async function applySettings() {
  if (filesSettings().vim) await loadVim().then(defineVimSave)
  for (const [key, view] of views) view.dispatch({ effects: prefsEffects(prefsFor(key)) })
}

let vimSaveDefined = false
async function defineVimSave() {
  if (vimSaveDefined) return
  vimSaveDefined = true
  const Vim = await vimApi()
  const save = (cm: { cm6?: EditorView }) => {
    const key = [...views].find(([, v]) => v === cm.cm6)?.[0]
    if (key) void saveBuffer(key)
  }
  Vim.defineEx('write', 'w', save)
  Vim.defineEx('update', 'up', save)
}

/* ---------- Tabs ---------- */

/** Opens `path` in the agent's tab (the second pane with `side`), kept when `pin`; at `line`, its search shown. */
export function openFile(agent: string, path: string, opts: { pin?: boolean; side?: boolean; line?: number; search?: SearchQuery } = {}) {
  const at = own(agent)
  const i = at.tabs.findIndex((x) => x.path === path)
  if (i >= 0) {
    if (opts.pin) at.tabs[i].pinned = true
  } else {
    // The tab shown, not kept nor edited, gives its place.
    const tab = { path, pinned: !!opts.pin }
    const shown = opts.side ? at.split : at.active
    const loose = at.tabs.findIndex((x) => x.path === shown && !x.pinned && !metas[keyOf(agent, x.path)]?.dirty)
    if (loose >= 0) {
      forget(keyOf(agent, at.tabs[loose].path))
      at.tabs[loose] = tab
    } else at.tabs.push(tab)
  }
  if (opts.side) at.split = path === at.active ? null : path
  else {
    at.active = path
    if (at.split === path) at.split = null
  }
  at.recent = [path, ...at.recent.filter((p) => p !== path)].slice(0, 30)
  saveTabs()
  void ensureLoaded(agent, path).then(() => {
    if (opts.line) goToLine(agent, path, opts.line, opts.search)
  })
}

export function pinTab(agent: string, path: string) {
  const tab = tabsAll[agent]?.tabs.find((x) => x.path === path)
  if (tab && !tab.pinned) {
    tab.pinned = true
    saveTabs()
  }
}

/** Closes a tab; its edit not saved is lost (the caller asks first). */
export function closeTab(agent: string, path: string) {
  const at = own(agent)
  const i = at.tabs.findIndex((x) => x.path === path)
  if (i < 0) return
  at.tabs.splice(i, 1)
  if (at.active === path) at.active = at.tabs[Math.min(i, at.tabs.length - 1)]?.path ?? null
  if (at.split === path) at.split = null
  saveTabs()
  forget(keyOf(agent, path))
}

export function setSplit(agent: string, path: string | null) {
  const at = own(agent)
  at.split = path === at.active ? null : path
  saveTabs()
  if (path) void ensureLoaded(agent, path)
}

function forget(key: string) {
  buffers.delete(key)
  delete metas[key]
}

/* ---------- Buffers ---------- */

const flashEffect = StateEffect.define<{ from: number; to: number }[]>()
/** Lines the agent just changed, lit for a moment. */
const flashes = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes)
    for (const e of tr.effects) {
      if (e.is(flashEffect)) deco = e.value.length ? Decoration.set(e.value.map((r) => Decoration.line({ class: 'cm-flash' }).range(r.from)), true) : Decoration.none
    }
    return deco
  },
  provide: (f) => EditorView.decorations.from(f),
})

/**
 * A change of the git gutter peeked at: its place among the file's changes, the lines as
 * committed, and its buttons (the previous, the next, Revert hunk, close).
 */
function renderPeek(dom: HTMLElement, at: number, view: EditorView): () => void {
  const all = hunks(view.state)
  const i = all.findIndex((c) => c.fromB === at)
  const c = all[i]
  if (!c) return () => {}
  const kind = kindOf(c)
  const el = (tag: string, cls: string, text = '') => Object.assign(document.createElement(tag), { className: cls, textContent: text })
  const head = el('div', 'cm-git-peek-head')
  head.append(el('b', '', t('Change {n} of {total}', { n: i + 1, total: all.length })), el('span', 'cm-git-peek-kind', kind === 'add' ? t('lines added') : kind === 'del' ? t('lines removed') : t('lines changed')), el('span', 'cm-git-peek-grow'))
  const go = (n: number) => {
    const next = all[(n + all.length) % all.length]
    view.dispatch({ effects: [peekAt.of(next.fromB), EditorView.scrollIntoView(next.fromB, { y: 'center' })] })
  }
  const button = (label: string, title: string, run: () => void, cls = '') => {
    const b = el('button', `cm-button ${cls}`, label) as HTMLButtonElement
    b.title = title
    b.onmousedown = (e) => e.preventDefault()
    b.onclick = run
    head.append(b)
  }
  if (all.length > 1) {
    button('↑', t('The previous change'), () => go(i - 1))
    button('↓', t('The next change'), () => go(i + 1))
  }
  if (!view.state.readOnly) button(t('Revert'), t('Put these lines back as committed (saved with the file)'), () => revertChunk(view, c))
  button('✕', t('Close'), () => view.dispatch({ effects: peekAt.of(null) }), 'cm-git-peek-close')
  dom.append(head)
  const lines = committedLines(view.state, c)
  if (lines.length) for (const l of lines.slice(0, 200)) dom.append(el('div', 'cm-git-peek-line', l || ' '))
  else dom.append(el('div', 'cm-git-peek-note', t('Not in the last commit.')))
  return () => {}
}

function touch(key: string, patch: Partial<Meta> = {}) {
  const m = metas[key]
  if (m) Object.assign(m, patch, { version: m.version + 1 })
}

function makeState(key: string, text: string, base: string | null): EditorState {
  const state = EditorState.create({
    doc: text,
    extensions: baseExtensions(prefsFor(key), [
      gitGutter(renderPeek),
      flashes,
      keymap.of([{ key: 'Mod-s', preventDefault: true, run: () => (void saveBuffer(key), true) }]),
      EditorView.updateListener.of((u) => {
        const b = buffers.get(key)
        if (!b) return
        b.state = u.state
        if (!u.docChanged && !u.selectionSet) return
        const dirty = u.state.doc.toString() !== b.disk
        // Edited: its tab is kept.
        if (u.docChanged && dirty && u.transactions.some((tr) => tr.isUserEvent('input') || tr.isUserEvent('delete') || tr.isUserEvent('revert') || tr.isUserEvent('undo') || tr.isUserEvent('redo'))) pinTab(b.agent, b.path)
        touch(key, { dirty })
      }),
    ]),
  })
  return base === null ? state : state.update({ effects: setBase.of(base) }).state
}

/** What its gutter compares it to: as committed, nothing for a new file, itself when unchanged. */
const baseOf = (p: Extract<FilePreview, { kind: 'text' }>) => p.head ?? (p.new ? '' : p.text)

/** Reads `path` once (later calls wait for that read). */
const loading = new Map<string, Promise<void>>()
export function ensureLoaded(agent: string, path: string): Promise<void> {
  const key = keyOf(agent, path)
  const pending = loading.get(key)
  if (pending) return pending
  if (buffers.has(key) && !metas[key]?.error) return Promise.resolve()
  metas[key] = { loading: true, error: '', dirty: false, saving: false, pending: false, conflict: null, comparing: false, editing: false, version: 0 }
  const job = (async () => {
    try {
      const p = await agentFilePreview(agent, path)
      const b: Buffer = { agent, path, preview: p, disk: p.kind === 'text' ? p.text : '', stamp: p.kind === 'text' ? p.stamp : '', state: null }
      buffers.set(key, b)
      if (p.kind === 'text') {
        if (filesSettings().vim) await loadVim().then(defineVimSave)
        b.state = makeState(key, p.text, baseOf(p))
        const lang = await languageOf(path)
        if (lang) apply(key, { effects: compartments.language.reconfigure(lang) })
      }
      touch(key, { loading: false })
    } catch (e) {
      touch(key, { loading: false, error: String(e) })
    } finally {
      loading.delete(key)
    }
  })()
  loading.set(key, job)
  return job
}

/** A change to a buffer: through its editor when one shows it, else to its state. */
function apply(key: string, spec: TransactionSpec) {
  const view = views.get(key)
  if (view) return view.dispatch(spec)
  const b = buffers.get(key)
  if (b?.state) b.state = b.state.update(spec).state
}

/** An editor shows buffer `key` now (`null`: none). Brought to the settings of the moment. */
export function attachView(key: string, view: EditorView | null, previous: string | null) {
  if (previous && views.get(previous) === view) views.delete(previous)
  if (!view) return
  const b = buffers.get(key)
  if (!b?.state) return
  views.set(key, view)
  if (view.state !== b.state) view.setState(b.state)
  view.dispatch({ effects: prefsEffects(prefsFor(key)) })
}
export const detachView = (key: string, view: EditorView) => views.get(key) === view && views.delete(key)

export function goToLine(agent: string, path: string, line: number, search?: SearchQuery) {
  const key = keyOf(agent, path)
  const b = buffers.get(key)
  if (!b?.state) return
  const n = Math.max(1, Math.min(line, b.state.doc.lines))
  const l = b.state.doc.line(n)
  const view = views.get(key)
  apply(key, { selection: EditorSelection.cursor(l.from), effects: [EditorView.scrollIntoView(l.from, { y: 'center' }), ...(search ? [setSearchQuery.of(search)] : [])] })
  if (view) {
    if (search) openSearchPanel(view)
    view.focus()
  }
}

/** On a phone: editing it (passkey on save), or reading it again. */
export function setEditing(agent: string, path: string, on: boolean) {
  const key = keyOf(agent, path)
  touch(key, { editing: on })
  apply(key, { effects: prefsEffects(prefsFor(key)) })
}

/* ---------- Saving ---------- */

/** It works: its folder is not written under it. */
export const agentWorking = (agent: string) => {
  const a = live.snapshot?.agents?.find((x) => x.id === agent)
  return a?.status === 'running' || a?.status === 'waiting_tool'
}

export async function saveBuffer(key: string) {
  const b = buffers.get(key)
  const m = metas[key]
  if (!b?.state || !m || m.saving) return
  const text = b.state.doc.toString()
  if (text === b.disk && !m.conflict) return
  if (m.conflict) return toast(t('The agent changed this file: compare first, or take one of the versions'), 4000)
  if (agentWorking(b.agent)) {
    touch(key, { pending: true })
    return toast(t('Saved once the agent’s turn is over'), 3500)
  }
  touch(key, { saving: true, pending: false })
  try {
    await invoke('agent_file_save', { id: b.agent, path: b.path, original: b.disk, text })
    b.disk = text
    const [stamp] = await invoke<(string | null)[]>('agent_files_stamps', { id: b.agent, paths: [b.path] }).catch(() => [null])
    if (stamp) b.stamp = stamp
    touch(key, { saving: false, dirty: (b.state?.doc.toString() ?? text) !== text })
    toast(t('{name} saved', { name: b.path }))
  } catch (e) {
    touch(key, { saving: false })
    const msg = String(e)
    // Changed on disk meanwhile: what is there now, to compare with.
    if (/changed since/.test(msg)) await reread(key, true)
    else toast(msg, 6000)
  }
}

/** Saves what waited for the agent's turn to end. */
export function savePending(agent: string) {
  for (const [key, b] of buffers) if (b.agent === agent && metas[key]?.pending) void saveBuffer(key)
}

/** Some file of this agent waits to be saved. */
export const hasPending = (agent: string) => [...buffers].some(([key, b]) => b.agent === agent && metas[key]?.pending)

/* ---------- The agent writing ---------- */

/**
 * Open files of the agent changed on disk since read: read again. True when one did (the tree's
 * statuses may have changed too).
 */
export async function checkStamps(agent: string): Promise<boolean> {
  const loaded = [...buffers.values()].filter((b) => b.agent === agent && b.state)
  if (!loaded.length) return false
  const stamps = await invoke<(string | null)[]>('agent_files_stamps', { id: agent, paths: loaded.map((b) => b.path) }).catch(() => null)
  if (!stamps) return false
  let changed = false
  for (const [i, b] of loaded.entries()) {
    const s = stamps[i]
    if (!s || s === b.stamp) continue
    changed = true
    await reread(keyOf(agent, b.path), false)
  }
  return changed
}

/**
 * Reads the file again. Not edited here: its text replaced in place (where you were kept, its
 * changed lines lit). Edited: the disk's text kept aside as a conflict (`asked`: after a refused save).
 */
async function reread(key: string, asked: boolean) {
  const b = buffers.get(key)
  if (!b?.state) return
  let p: FilePreview
  try {
    p = await agentFilePreview(b.agent, b.path)
  } catch {
    return
  }
  if (p.kind !== 'text') return
  b.stamp = p.stamp
  b.preview = p
  const mine = b.state.doc.toString()
  if (p.text === b.disk && !asked) return
  if (mine !== b.disk) {
    if (p.text === mine) {
      b.disk = p.text
      touch(key, { dirty: false, conflict: null, pending: false })
    } else touch(key, { conflict: p.text, pending: false })
    apply(key, { effects: setBase.of(baseOf(p)) })
    return
  }
  const changes = textDiff(mine, p.text).map((c) => ({ from: c.fromA, to: c.toA, insert: p.text.slice(c.fromB, c.toB) }))
  b.disk = p.text
  apply(key, { changes, annotations: [Transaction.addToHistory.of(false), Transaction.userEvent.of('reload')], effects: setBase.of(baseOf(p)) })
  // The lines it wrote, lit.
  const doc = buffers.get(key)!.state!.doc
  const lit = textDiff(mine, p.text).filter((c) => c.toB > c.fromB).flatMap((c) => {
    const out: { from: number; to: number }[] = []
    for (let n = doc.lineAt(c.fromB).number; n <= doc.lineAt(Math.max(c.fromB, c.toB - 1)).number && out.length < 200; n++) out.push({ from: doc.line(n).from, to: doc.line(n).to })
    return out
  })
  apply(key, { effects: flashEffect.of(lit) })
  setTimeout(() => apply(key, { effects: flashEffect.of([]) }), 1700)
  touch(key, { dirty: false, conflict: null })
}

/** A conflict: the editor takes the disk's version. */
export function takeTheirs(key: string) {
  const b = buffers.get(key)
  const theirs = metas[key]?.conflict
  if (!b?.state || theirs == null) return
  endCompare(key)
  b.disk = theirs
  apply(key, { changes: { from: 0, to: b.state.doc.length, insert: theirs } })
  touch(key, { conflict: null, dirty: false })
}

/** A conflict: your version kept, written over the disk's at the next save. */
export function keepMine(key: string) {
  const b = buffers.get(key)
  const theirs = metas[key]?.conflict
  if (!b || theirs == null) return
  endCompare(key)
  b.disk = theirs
  touch(key, { conflict: null, dirty: b.state?.doc.toString() !== theirs })
}

/** A conflict compared: the disk's version as the original, each change kept or put back. */
export async function compare(key: string) {
  const theirs = metas[key]?.conflict
  if (theirs == null) return
  apply(key, { effects: compartments.merge.reconfigure(unifiedMergeView({ original: theirs, mergeControls: true, gutter: false, syntaxHighlightDeletions: true })) })
  touch(key, { comparing: true })
}

/** Done comparing: what the editor holds is yours, to save over the disk's. */
export function endCompare(key: string) {
  if (!metas[key]?.comparing) return
  apply(key, { effects: compartments.merge.reconfigure([]) })
  keepMineAfterCompare(key)
  views.get(key)?.focus()
}
function keepMineAfterCompare(key: string) {
  const b = buffers.get(key)
  const theirs = metas[key]?.conflict
  if (b && theirs != null) b.disk = theirs
  touch(key, { comparing: false, conflict: null, dirty: !!b?.state && b.state.doc.toString() !== b.disk })
}

/* ---------- The palette ---------- */

export { paletteFiles } from './palette-files.svelte'
