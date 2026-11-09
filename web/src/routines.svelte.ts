import { invoke, isLocal } from './bridge'
import { live } from './live.svelte'
import { pref, setPref } from './layout.svelte'
import { openAgents } from './agents.svelte'
import { ROUTINE_OPENED } from './agent-archive.svelte'
import { toast } from './toast.svelte'
import { t } from './i18n.svelte'
import { retro, retroMarkdown, type RetroSummary } from './retro'
import { pointsOf } from './retro-points.svelte'
import { notes } from './notes.svelte'
import { BUILTINS } from './routines'
import type { TrailState } from './blocks/trail/trail'
import type { Snapshot } from './types'
import type { RoutineRun } from './bindings/RoutineRun'

export * from './routines'

/**
 * Routines (`[[agents.routines]]`): a quick action launched by a cron or at a pace, without me —
 * run by Prism's core (`prism_agents::routines`), the window open or not. Here, what acts: the
 * gestures of "To review" (Open, Archive, Rerun), Settings' (Run now, Trust, on and off), and what
 * the builtins read of the window (the open alerts, the retro's figures). The rest: `routines.ts`.
 */

/** A routine's agent opened from "To review" (Open, Reply): in the lists from then on (agent-archive). */
export function markOpened(id: string) {
  const list = pref<string[]>(ROUTINE_OPENED, [])
  if (!list.includes(id)) setPref(ROUTINE_OPENED, [...list.slice(-199), id])
}

export function openRunAgent(r: RoutineRun) {
  if (!r.agent) return
  if (!live.snapshot?.agents?.some((a) => a.id === r.agent)) return toast(t('Its agent is closed'))
  markOpened(r.agent)
  openAgents(r.agent)
}

const failed = (e: unknown) => toast(String(e), 8000)
/** A routine to start from: a quick action just made (Make it a routine); Settings › Routines opens a new one on it. */
export const routineFor = $state({ action: '', repos: [] as string[] })

export const archiveRuns = (runs: string[]) => invoke('routine_archive', { runs }).catch(failed)
export const rerunRun = (run: string) => invoke('routine_rerun', { run }).then(() => toast(t('Launched again'))).catch(failed)
export const runRoutine = (name: string) => invoke('routine_run', { name }).then(() => toast(t('{name}: launched', { name }))).catch(failed)
export const trustRoutine = (name: string, hash: string) => invoke('routine_trust', { name, hash }).then(() => toast(t('{name} trusted: it runs from its next time', { name }))).catch(failed)
export const enableRoutine = (name: string, on: boolean) => invoke('routine_enable', { name, on }).catch(failed)
export const removeRoutine = (name: string) => invoke('routine_remove', { name }).catch(failed)

/* What the builtins read: the window's own figures, given to the core when they change. */

let sent = ''
/** The last state given, read when the timer fires: nothing is computed in between. */
let latest: { s: Snapshot } | null = null
let timer: ReturnType<typeof setTimeout> | null = null
/**
 * The current sprint's retro figures, given to the core (`routine_inputs`) when a builtin routine
 * is declared and they changed: at most once a minute, computed only then (not at each snapshot).
 * The open alerts are the core's own (`Snapshot.alerts`).
 */
export function publishInputs(s: Snapshot) {
  if (!isLocal()) return
  latest = { s }
  if (timer) return
  if (!(s.agents_settings?.routines ?? []).some((r) => (BUILTINS as readonly string[]).includes(r.action.trim()))) return
  timer = setTimeout(() => {
    timer = null
    if (!latest) return
    const { s } = latest
    latest = null
    let figures: string | null = null
    if ((s.agents_settings?.routines ?? []).some((r) => r.action.trim() === 'builtin:retro-draft')) {
      const r = retro(s, s.blocks?.trail as unknown as TrailState | undefined, notes, pref<RetroSummary[]>('retro.history', []))
      figures = r ? retroMarkdown(r, pointsOf(r.name)) : null
    }
    const payload = JSON.stringify({ retro: figures })
    if (payload === sent) return
    sent = payload
    invoke('routine_inputs', { retro: figures }).catch((e) => {
      sent = ''
      console.error('routine_inputs', e)
    })
  }, sent ? 60_000 : 2_000)
}
