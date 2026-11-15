import { live } from './live.svelte'
import { saveEdits } from './settings.svelte'
import { toast } from './toast.svelte'
import { t } from './i18n.svelte'
import { allViews, colorName, fileRows, pageRows, viewColor, viewSlug, views, type View } from './views'
import { currentView, editing, forgetViews, layout, setView, toggleEditing } from './layout.svelte'
import type { ViewConfig } from './types'

/**
 * The pages themselves, managed where they are seen (the rail, the page's header): rename, icon
 * and color, duplicate, move, hide, delete, add, back to the pack's. Each operation changes
 * config.toml's `[[views]]` (`pageRows` → `fileRows`), as Settings › Interface › Pages does; a
 * page keeps its `id` whatever its name, and with it its sizes and its blocks' states. A pack's
 * page changed here becomes config.toml's own: Prism says so, with Undo.
 */

/** What the rail and the header show of the pages being managed. */
export const pageUi = $state({
  /** A page's menu: right-click or its ⋯, at this point. */
  menu: null as { id: string; x: number; y: number } | null,
  /** The page whose name is being typed, in the rail or in the header. */
  renaming: null as { id: string; at: 'rail' | 'header' } | null,
  /** Its icon and color being chosen: shown in the rail as they are picked, until applied (Esc: as before). */
  styling: null as { id: string; icon: string; color: string; x: number; y: number } | null,
  /** The « + » menu of the rail, at this point. */
  adding: null as { x: number; y: number } | null,
  /** « Hidden pages », at this point. */
  hiddenMenu: null as { x: number; y: number } | null,
})

/** A page as the rail shows it: the icon and color being picked, otherwise its own. */
export function look(v: View): { icon: string; color: string } {
  const s = pageUi.styling
  return s && s.id === v.id ? { icon: s.icon, color: viewColor(s.color) ?? v.color } : { icon: v.icon, color: v.color }
}

/** Writes the pages to config.toml. */
async function write(rows: ViewConfig[]): Promise<boolean> {
  try {
    await saveEdits([{ path: ['views'], value: fileRows(rows) }])
    return true
  } catch (e) {
    toast(t('Pages not written to config.toml: {error}', { error: String(e) }), 5000)
    return false
  }
}

/** A pack's page changed here: it no longer follows its pack; Undo puts the pack's back. */
export function leftPack(v: Pick<View, 'id'>) {
  toast(t('This page no longer follows the pack'), 6000, { label: t('Undo'), run: () => void resetToPack(v.id) })
}

/** Changes page `id` (`fn` returns its new row); a pack's page followed becomes config.toml's. */
async function change(id: string, fn: (row: ViewConfig) => ViewConfig) {
  const rows = pageRows()
  const i = rows.findIndex((r) => r.id === id)
  if (i < 0) return
  const followed = rows[i].origin === 'pack'
  rows[i] = { ...fn(rows[i]), origin: rows[i].origin === 'pack' ? undefined : rows[i].origin }
  if ((await write(rows)) && followed) leftPack({ id })
}

/** Waits for the snapshot to have page `id` (config.toml written, read again): a second at most. */
function whenPage(id: string): Promise<boolean> {
  return new Promise((done) => {
    const start = Date.now()
    const tick = () => {
      if (allViews().some((v) => v.id === id)) return done(true)
      if (Date.now() - start > 3000) return done(false)
      setTimeout(tick, 50)
    }
    tick()
  })
}

/** An identifier no page has: `base`, `base-2`… */
function freeId(base: string, rows = pageRows()): string {
  const taken = new Set(rows.map((r) => r.id))
  let id = base || 'page'
  for (let n = 2; taken.has(id); n++) id = `${base}-${n}`
  return id
}

/** A name no page has: `name`, `name 2`… */
function freeName(name: string, rows = pageRows()): string {
  const taken = new Set(rows.map((r) => r.name))
  let out = name
  for (let n = 2; taken.has(out); n++) out = `${name} ${n}`
  return out
}

export function renamePage(id: string, name: string) {
  pageUi.renaming = null
  const next = name.trim()
  const row = pageRows().find((r) => r.id === id)
  if (!next || !row || next === row.name) return
  void change(id, (r) => ({ ...r, name: next }))
}

/** Starts typing page `id`'s name; F2 for the page shown. */
export function startRename(id: string, at: 'rail' | 'header' = 'rail') {
  pageUi.menu = null
  pageUi.renaming = { id, at }
}

/** Opens the icon and color grid of page `id`, at this point. */
export function startStyling(id: string, x: number, y: number) {
  const v = allViews().find((x) => x.id === id)
  if (!v) return
  const row = pageRows().find((r) => r.id === id)
  pageUi.menu = null
  pageUi.styling = { id, icon: v.icon, color: colorName(row?.color ?? 'accent'), x, y }
}

/** The grid closed: the icon and color picked written (`apply`), or the page as before. */
export function endStyling(apply: boolean) {
  const s = pageUi.styling
  if (!s) return
  const row = pageRows().find((r) => r.id === s.id)
  if (apply && row && (s.icon !== (row.icon ?? '') || s.color !== (row.color ?? ''))) {
    // Written before the grid goes, so that the rail does not flash the former look meanwhile.
    void change(s.id, (r) => ({ ...r, icon: s.icon, color: s.color })).then(() => whenPage(s.id)).then(() => {
      if (pageUi.styling?.id === s.id) pageUi.styling = null
    })
    return
  }
  pageUi.styling = null
}

/** A copy of page `id` after it, its sizes and its blocks' states with it. */
export async function duplicatePage(id: string) {
  pageUi.menu = null
  const rows = pageRows()
  const i = rows.findIndex((r) => r.id === id)
  if (i < 0) return
  const name = freeName(t('{name} copy', { name: rows[i].name ?? '' }), rows)
  const copy: ViewConfig = { ...rows[i], name, id: freeId(viewSlug(name), rows), origin: undefined, hidden: undefined }
  copyLayout(id, copy.id!)
  rows.splice(i + 1, 0, copy)
  if (await write(rows)) void open(copy.id!, false)
}

/** Page `from`'s sizes and its blocks' states, for page `to`. */
function copyLayout(from: string, to: string) {
  if (layout.views[from]) layout.views[to] = structuredClone($state.snapshot(layout.views[from]))
  for (const [k, mode] of Object.entries(layout.modes)) if (k.startsWith(`${from}:`)) layout.modes[`${to}:${k.slice(from.length + 1)}`] = mode
}

/** Moves page `id` to place `to` among the rail's pages (the hidden ones keep their place). */
export function movePage(id: string, to: number) {
  pageUi.menu = null
  const rows = pageRows()
  const shown = rows.filter((r) => !r.hidden && r.name?.trim())
  const from = shown.findIndex((r) => r.id === id)
  if (from < 0 || to < 0 || to >= shown.length || to === from) return
  const order = shown.map((r) => r.id)
  order.splice(to, 0, ...order.splice(from, 1))
  // The rail's pages take the places they had, in their new order.
  const byId = new Map(shown.map((r) => [r.id, r]))
  let k = 0
  void write(rows.map((r) => (!r.hidden && r.name?.trim() ? byId.get(order[k++]) ?? r : r)))
}

/** Hides page `id` from the rail (`on`), or shows it again; never the last one shown. */
export async function hidePage(id: string, on = true) {
  pageUi.menu = null
  pageUi.hiddenMenu = null
  if (on && views().filter((v) => !v.hidden).length <= 1) return toast(t('The last page shown stays in the rail'))
  const rows = pageRows().map((r) => (r.id === id ? { ...r, hidden: on || undefined } : r))
  if (!(await write(rows))) return
  if (on && currentView().id === id) void setView(views().find((v) => v.id !== id)?.id ?? '')
  if (!on && (await whenPage(id))) void setView(id)
}

/** Deletes page `id`, with Undo a few seconds; never the last one. */
export async function deletePage(id: string) {
  pageUi.menu = null
  const before = pageRows()
  const row = before.find((r) => r.id === id)
  if (!row) return
  if (before.filter((r) => r.name?.trim()).length <= 1) return toast(t('The last page cannot be deleted'))
  const shown = currentView().id === id
  if (!(await write(before.filter((r) => r.id !== id)))) return
  if (shown) void setView(views().find((v) => v.id !== id)?.id ?? '')
  toast(t('Page {name} deleted', { name: row.name ?? '' }), 6000, { label: t('Undo'), run: () => void write(before) })
}

/** Page `id` as its pack has it again, followed from then on; its layout starts again from it. */
export async function resetToPack(id: string) {
  pageUi.menu = null
  const rows = pageRows()
  const i = rows.findIndex((r) => r.id === id)
  if (i < 0) return
  rows[i] = { id, name: rows[i].name, hidden: rows[i].hidden, columns: [], origin: 'pack' }
  forgetViews([id])
  if (await write(rows)) toast(t('Page back to its pack'))
}

/**
 * A new page after the one shown: empty, a copy of the one shown, or `template`'s (a page of a
 * pack's template). It opens in Edit layout, its name ready to be typed.
 */
export async function addPage(from: 'empty' | 'copy' | ViewConfig) {
  pageUi.adding = null
  const rows = pageRows()
  const current = currentView().id
  const at = Math.max(0, rows.findIndex((r) => r.id === current)) + 1
  const source = from === 'copy' ? rows.find((r) => r.id === current) : from === 'empty' ? undefined : from
  const name = freeName(from === 'empty' ? t('New page') : from === 'copy' ? t('{name} copy', { name: source?.name ?? '' }) : (source?.name ?? t('New page')), rows)
  const page: ViewConfig = { ...(source ?? { columns: [] }), name, id: freeId(viewSlug(name), rows), origin: undefined, hidden: undefined }
  if (from === 'copy') copyLayout(current, page.id!)
  // Without pages of its own, config.toml gets the empty Dashboard the rail showed, then this one.
  rows.splice(at, 0, page)
  if (await write(rows)) void open(page.id!, true)
}

/** Shows page `id` once the snapshot has it; `rename`: in Edit layout, its name being typed. */
async function open(id: string, rename: boolean) {
  if (!(await whenPage(id))) return
  await setView(id)
  if (!rename) return
  if (!editing.on) toggleEditing()
  startRename(id, 'header')
}

/** The pages of the packs' templates, offered by « + » (a template's pages by its name). */
export function templatePages(): { template: string; page: ViewConfig }[] {
  return (live.snapshot?.templates ?? []).flatMap((tp) => tp.views.filter((v) => v.name?.trim()).map((page) => ({ template: tp.label, page })))
}
