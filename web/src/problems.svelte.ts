import { invoke, isDesktop, onCommandFailed } from './bridge'
import { pref, setPref } from './layout.svelte'
import { packUpdates, updates } from './updates.svelte'
import { t } from './i18n.svelte'
import type { Snapshot } from './types'
import type { CustomBlocksState } from './bindings/CustomBlocksState'

/**
 * Prism's problems, in one list behind the status bar (ProblemsItem.svelte): what fails now (a
 * source that does not read, the config or a pack, a pack's source out of reach), and what failed
 * lately (the core's warnings and errors, `Snapshot.problems`, and the commands it failed here).
 * Otherwise they only reach `prism.log`, or a toast gone in two seconds.
 */
export interface Problem {
  /** `ko` fails now or is an error, `warn` a warning. */
  level: 'ko' | 'warn'
  /** Where: the source, the module or the command. */
  where: string
  message: string
  /** Its last time, in ms; absent for what fails now (it lasts). */
  at?: number
  count: number
  /** Where it is fixed. */
  settings?: string
}

/** The commands a page calls expecting they may fail (a picture, a probe): not problems. */
const EXPECTED = new Set(['forge_avatar', 'forge_custom_emoji', 'forge_emoji_image', 'ticket_image', 'agent_files_stamps', 'reduce_transparency', 'youtube_relay', 'feedback_url', 'feedback_draft', 'set_help_guides', 'check_setting', 'token_state'])
/** The sources that failing says nothing wrong (`prism_core::alerts` too). */
const QUIET_SOURCES = new Set(['usage'])
const KEPT = 30

/** `since`: when the list was seen before this opening, what came after shown as new. */
export const problems = $state<{ open: boolean; since: number; failed: Problem[] }>({ open: false, since: 0, failed: [] })

/** Listens to the commands this window calls: the desktop's only (a phone has its own messages). */
export function watchProblems() {
  if (!isDesktop()) return
  onCommandFailed((command, e) => {
    if (EXPECTED.has(command)) return
    const message = String(e)
    const same = problems.failed.find((p) => p.where === command && p.message === message)
    const count = (same?.count ?? 0) + 1
    problems.failed = [{ level: 'warn' as const, where: command, message, at: Date.now(), count }, ...problems.failed.filter((p) => p !== same)].slice(0, KEPT)
  })
}

/** What fails now. */
export function current(snap: Snapshot | null): Problem[] {
  const out: Problem[] = Object.entries(snap?.errors ?? {})
    .filter(([source]) => !QUIET_SOURCES.has(source))
    .map(([source, message]) => ({ level: source === 'config' && message.startsWith('no config yet') ? 'warn' : 'ko', where: source, message, count: 1, settings: source === 'config' ? 'file' : undefined }))
  // A custom block whose last read failed: its reason and the end of its error output (never in prism.log).
  for (const b of (snap?.blocks?.custom_blocks as CustomBlocksState | undefined)?.blocks ?? []) {
    if (b.error && !b.pending) out.push({ level: 'ko', where: `custom:${b.id}`, message: b.stderr ? `${b.error}\n${b.stderr}` : b.error, count: 1, settings: 'custom-blocks' })
  }
  for (const e of (snap?.blocks?.custom_blocks as CustomBlocksState | undefined)?.errors ?? []) out.push({ level: 'ko', where: 'custom_blocks', message: e, count: 1, settings: 'custom-blocks' })
  // A routine whose declaration does not hold (cron and every both, a cron that does not read, an action unknown): it does not run.
  for (const r of snap?.routines?.routines ?? []) {
    if (r.error) out.push({ level: 'ko', where: t('routine {name}', { name: r.name }), message: r.error, count: 1, settings: 'routines' })
    else if (r.pending) out.push({ level: 'warn', where: t('routine {name}', { name: r.name }), message: t('changed outside Settings: it waits for your trust'), count: 1, settings: 'routines' })
  }
  for (const p of packUpdates.list) {
    if (p.error) out.push({ level: 'ko', where: t('pack {name}', { name: p.name }), message: t('its source does not read: {error}', { error: p.error }), count: 1, settings: 'packs' })
    else if (p.too_new) out.push({ level: 'warn', where: t('pack {name}', { name: p.name }), message: t('its source needs Prism {version}: update Prism to get it', { version: p.requires ?? '?' }), count: 1, settings: 'updates-now' })
  }
  if (packUpdates.error) out.push({ level: 'warn', where: t('packs'), message: packUpdates.error, count: 1, settings: 'packs' })
  if (updates.error) out.push({ level: 'warn', where: t('updates'), message: updates.error, count: 1, settings: 'updates-now' })
  return out
}

/** What failed lately, most recent first: the core's (but a source's error still shown in `current`) and the commands'. */
export function recent(snap: Snapshot | null): Problem[] {
  const live = new Set(Object.entries(snap?.errors ?? {}).map(([source, err]) => `${source}: ${err}`))
  const core: Problem[] = (snap?.problems ?? [])
    .filter((p) => !live.has(p.message))
    .map((p) => ({ level: p.level === 'error' ? 'ko' : 'warn', where: p.target, message: p.message, at: new Date(p.at).getTime(), count: p.count }))
  return [...core, ...problems.failed].sort((a, b) => b.at! - a.at!)
}

/** When the list was last opened: what came after is new. */
export const seenAt = () => pref<number>('problemsSeen', 0)

export function openProblems(open = true) {
  if (open && !problems.open) {
    problems.since = seenAt()
    setPref('problemsSeen', Date.now())
  }
  problems.open = open
}

/** Forgets what failed lately; what fails now stays. */
export async function clearProblems() {
  problems.failed = []
  await invoke('problems_clear')
}

/** The list as text, to paste in an issue or a message. */
export function problemsText(now: Problem[], lately: Problem[]): string {
  const line = (p: Problem) => `[${p.level === 'ko' ? 'error' : 'warn'}] ${p.at ? `${new Date(p.at).toISOString()} ` : ''}${p.where}: ${p.message}${p.count > 1 ? ` (×${p.count})` : ''}`
  return [...now.map(line), ...lately.map(line)].join('\n')
}
