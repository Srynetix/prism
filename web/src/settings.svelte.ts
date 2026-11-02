import { hasCore, invoke, isDesktop } from './bridge'
import { t } from './i18n.svelte'
import { closeHelp } from './help-view.svelte'
import type { AgentCli } from './settings-schema'
import type { FileView } from './bindings/FileView'
import type { PackInfo } from './bindings/PackInfo'
import type { PackFile } from './bindings/PackFile'
export type { FileView, PackInfo, PackFile }

/**
 * Settings view: the mirror of `config.toml`. The file is read
 * as is; each modified field becomes a pending edit (`edits`, by key path), whose preview
 * the backend computes (the text that would be written, comments and order kept)
 * until saving (⌘S). `null` removes the key: the packs' value, otherwise the default, takes over.
 */

/** A repository's pack source (`glab:…` / `gh:…`): its ref after `@`, empty for the default branch; otherwise null. */
export const packRef = (source: string | null | undefined) => (source && /^(glab|gh):/.test(source) ? (source.split(':').pop()!.split('@')[1] ?? '') : null)

export const settingsView = $state({ open: false, section: 'console' })

export function openSettings(section?: string) {
  closeHelp()
  settingsView.open = true
  if (section) settingsView.section = section
  if (!cfg.file && !cfg.loading) void loadConfig()
  void loadChoices()
}
export const closeSettings = () => (settingsView.open = false)
export const toggleSettings = () => (settingsView.open ? closeSettings() : openSettings())

export const cfg = $state({
  file: null as FileView | null,
  loading: false,
  error: '',
  edits: {} as Record<string, unknown>,
  /** The text that saving would write. */
  preview: '',
  saving: false,
  /** Last successful save, for a brief "Saved". */
  savedAt: 0,
})

export const editable = isDesktop

export async function loadConfig() {
  if (!isDesktop()) return
  cfg.loading = true
  try {
    cfg.file = await invoke<FileView>('read_config')
    cfg.preview = cfg.file.text
    cfg.error = ''
  } catch (e) {
    cfg.error = String(e)
  } finally {
    cfg.loading = false
  }
}

const key = (path: string[]) => path.join('.')
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)

const walk = (root: unknown, path: string[]): unknown => {
  let v = root
  for (const p of path) v = v && typeof v === 'object' ? (v as Record<string, unknown>)[p] : undefined
  return v
}
/** The value written in the file, or `undefined`. */
export const fileValue = (path: string[]) => walk(cfg.file?.values, path)
/** The default value, according to the backend (`null`: no default). */
export const defaultOf = (path: string[]) => walk(cfg.file?.defaults, path)

/** The value the packs give, or `undefined`. */
export const inheritedValue = (path: string[]) => walk(cfg.file?.inherited, path)

/** The displayed value: edited, otherwise the file's, otherwise the packs', otherwise the default. */
export function valueOf<T>(path: string[], fallback?: T): T {
  const k = key(path)
  const def = (inheritedValue(path) ?? fallback ?? defaultOf(path)) as T
  if (k in cfg.edits) return (cfg.edits[k] ?? def) as T
  return (fileValue(path) ?? def) as T
}

/** A pack gives this key: removing it from the file comes back to the pack's value. */
export const hasInherited = (path: string[]) => inheritedValue(path) !== undefined

/** Where an inherited value comes from: the pack's name when there is a single one. */
export function packLabel(): string {
  const packs = cfg.file?.packs.filter((p) => !p.missing) ?? []
  return packs.length === 1 ? t('pack {name}', { name: packs[0].name }) : t('a pack')
}

/** The key will be in the file after saving (otherwise: default value). */
export function isSet(path: string[]): boolean {
  const k = key(path)
  return k in cfg.edits ? cfg.edits[k] !== null : fileValue(path) !== undefined
}

export const isChanged = (path: string[]) => key(path) in cfg.edits

export function setValue(path: string[], value: unknown, fallback?: unknown) {
  fallback = inheritedValue(path) ?? fallback ?? defaultOf(path) ?? undefined
  const k = key(path)
  const inFile = fileValue(path)
  // Going back to what the file already says is no longer an edit; nor is the pack's value or the
  // default without a key.
  if (same(value, inFile) || (inFile === undefined && fallback !== undefined && same(value, fallback))) delete cfg.edits[k]
  else cfg.edits[k] = value
  schedulePreview()
}

/* ---------- [[repos]]: merged by name ---------- */

type Row = Record<string, unknown>
/** `[[tables]]` the packs and config.toml merge entry by entry (packs.rs `KEYED`), by this key. */
const KEYED: Record<string, string> = { repos: 'name' }
/** The key a `[[table]]`'s entries merge by; none: the file's list replaces the packs'. */
export const keyedBy = (path: string[]) => (path.length === 1 ? KEYED[path[0]] : undefined)

const ownRows = (path: string[]): Row[] => {
  const k = key(path)
  return ((k in cfg.edits ? cfg.edits[k] : fileValue(path)) as Row[] | null | undefined) ?? []
}
const packRows = (path: string[]) => (inheritedValue(path) as Row[] | undefined) ?? []
/** Empty, false, absent: the same for a cell (an unset toggle reads false). */
const blank = (v: unknown) => v === undefined || v === null || v === false || v === '' || (Array.isArray(v) && !v.length)

/** The entries as the app reads them: each pack's entry with the file's on top, then the file's own ones. */
export function mergedRows(path: string[]): Row[] {
  const id = keyedBy(path)!
  const own = ownRows(path)
  const rows = packRows(path).map((p) => ({ ...p, ...own.find((o) => o[id] === p[id]) }))
  return [...rows, ...own.filter((o) => !packRows(path).some((p) => p[id] === o[id])).map((o) => ({ ...o }))]
}

/** A pack names this entry: removing it from the file only drops the file's changes. */
export const fromPack = (path: string[], row: Row) => packRows(path).some((p) => p[keyedBy(path)!] === row[keyedBy(path)!])

/**
 * Edits the merged entries: the file keeps, for a pack's entry, only its name and what differs
 * from the pack (a clone's path), and the other entries whole. A pack's value cannot be unset,
 * only changed (a toggle turned off is written `false`).
 */
export function setMergedRows(path: string[], rows: Row[]) {
  const id = keyedBy(path)!
  const own = rows.flatMap((r) => {
    const p = packRows(path).find((x) => x[id] === r[id])
    if (!p) return [r]
    const diff = Object.entries(r).filter(([k, v]) => k !== id && !(blank(v) && blank(p[k])) && !same(v, p[k]))
    return diff.length ? [{ [id]: r[id], ...Object.fromEntries(diff) }] : []
  })
  if (own.length) setValue(path, own)
  else resetValue(path)
}

/** Removes the key from the file: the packs' value, otherwise the default, takes over. */
export function resetValue(path: string[]) {
  const k = key(path)
  if (fileValue(path) === undefined) delete cfg.edits[k]
  else cfg.edits[k] = null
  schedulePreview()
}

const asEdits = () => Object.entries(cfg.edits).map(([k, value]) => ({ path: k.split('.'), value }))
export const changes = () => Object.keys(cfg.edits)

let timer: ReturnType<typeof setTimeout> | undefined
function schedulePreview() {
  clearTimeout(timer)
  timer = setTimeout(async () => {
    if (!isDesktop()) return
    if (!changes().length) return void (cfg.preview = cfg.file?.text ?? '')
    try {
      cfg.preview = await invoke<string>('preview_config', { edits: asEdits() })
      cfg.error = ''
    } catch (e) {
      cfg.error = String(e)
    }
  }, 150)
}

export async function saveConfig() {
  if (!isDesktop() || !changes().length || cfg.saving) return
  cfg.saving = true
  try {
    // Applied as it is saved: the collectors start again from it when what they read changed.
    cfg.file = await invoke<FileView>('save_config', { edits: asEdits() })
    cfg.edits = {}
    cfg.preview = cfg.file.text
    cfg.error = ''
    cfg.savedAt = Date.now()
  } catch (e) {
    cfg.error = String(e)
  } finally {
    cfg.saving = false
  }
}

/**
 * Saves a single key right away (the theme picked in the Theme menu), without touching the
 * other pending edits of the Settings view.
 */
export async function saveNow(path: string[], value: unknown) {
  try {
    await saveEdits([{ path, value }])
  } catch (e) {
    cfg.error = String(e)
  }
}

/** Several keys at once (the pages wizard); the error goes up to the caller. */
export async function saveEdits(edits: { path: string[]; value: unknown }[]) {
  if (!isDesktop()) return
  const file = await invoke<FileView>('save_config', { edits })
  if (cfg.file) {
    cfg.file = file
    for (const e of edits) delete cfg.edits[key(e.path)]
    schedulePreview()
  }
}

/* ---------- Packs ---------- */

/** `text` (what saving would write) merged on top of its packs, as the app reads it before the defaults. */
export const mergedToml = (text: string) => invoke<string>('merged_config', { text })

export const packBusy = $state({ name: '' })

/**
 * Settings › General › Packs: import (a file path, `glab:group/project:path@branch` or `gh:owner/repo:path@branch`), update from its
 * source, remove. Each returns the file view, applied as a save is.
 */
async function packCommand(busy: string, cmd: string, args: Record<string, string | null>): Promise<boolean> {
  if (!isDesktop() || packBusy.name) return false
  packBusy.name = busy
  try {
    cfg.file = await invoke<FileView>(cmd, args)
    cfg.error = ''
    schedulePreview()
    return true
  } catch (e) {
    cfg.error = String(e)
    return false
  } finally {
    packBusy.name = ''
  }
}
export const importPack = (source: string) => packCommand('+import', 'pack_import', { source })
/** `reference`: a repository's pack read at that branch, tag or commit from now on (empty: the default branch). */
export const updatePack = (name: string, reference: string | null = null) => packCommand(name, 'pack_update', { name, reference })
export const removePack = (name: string) => packCommand(`-${name}`, 'pack_remove', { name })

/** Relaunches Prism to apply the settings read at launch. */
export const restartApp = () => invoke('restart_app')
/** Quits Prism (the macOS menu's Quit; elsewhere Ctrl+Q or the palette, closing only minimises). */
export const quitApp = () => invoke('quit_app')
/** The window in full screen, or back (the macOS menu's; elsewhere F11 or the palette). */
export const toggleFullscreen = () => invoke('toggle_fullscreen')

/* ---------- Choices read from the machine ---------- */

export const choices = $state({ kube: [] as string[], apps: [] as string[], calendars: [] as string[], loaded: false, models: {} as Partial<Record<AgentCli, string[]>>, efforts: {} as Record<string, string[]> })
export async function loadChoices() {
  if (!isDesktop() || choices.loaded) return
  choices.loaded = true
  const [kube, apps, calendars] = await Promise.allSettled([invoke<string[]>('kube_contexts'), invoke<string[]>('installed_apps'), invoke<string[]>('calendar_names')])
  if (kube.status === 'fulfilled') choices.kube = kube.value
  if (apps.status === 'fulfilled') choices.apps = apps.value
  if (calendars.status === 'fulfilled') choices.calendars = calendars.value
}

/** The models an agent CLI offers (its catalog, or asked of it), read once: Settings' and the launcher's suggestions. */
export async function loadModels(cli: AgentCli) {
  if (!isDesktop() || choices.models[cli]) return
  choices.models[cli] = []
  try {
    choices.models[cli] = await invoke<string[]>('agent_models', { cli })
  } catch {
    // No list: the name is typed.
  }
}

/** Where `loadEfforts(cli, model)` keeps its list: a Codex model has its own levels. */
export const effortsKey = (cli: AgentCli, model?: string | null) => `${cli}|${model ?? ''}`
/**
 * The reasoning efforts an agent CLI takes (for `model`; its own when absent), lowest first, as
 * the CLI tells them, read once: Settings', the launcher's and an agent's header's choices.
 */
export async function loadEfforts(cli: AgentCli, model?: string | null) {
  const key = effortsKey(cli, model)
  // A phone asks too: its agents' menu offers them.
  if (!hasCore() || choices.efforts[key]) return
  choices.efforts[key] = []
  try {
    choices.efforts[key] = await invoke<string[]>('agent_efforts', { cli, model: model ?? null })
  } catch {
    // No list: the level is typed in Settings, none offered elsewhere.
  }
}

export function discardConfig() {
  cfg.edits = {}
  cfg.preview = cfg.file?.text ?? ''
  cfg.error = ''
}
