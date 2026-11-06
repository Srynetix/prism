import { live } from './live.svelte'
import { around, type ChangesState, type Range } from './blocks/changes/changes'
import { rememberPanel } from './recent-panels.svelte'

/** What the Changes drawer shows: an environment, a range, an application, the change to highlight. */
export interface ChangesFocus {
  env: string | null
  /** A fixed range (around a crash); absent: the block's period. */
  range?: Range | null
  /** `1h`, `since_yesterday`… when no range is fixed. */
  period?: string | null
  app?: string | null
  highlight?: string | null
  /** The first crash, scrolled to. */
  crash?: string | null
}

/**
 * The Changes drawer ("What changed"): the Changes block's timeline, wherever it is opened from
 * (a pod's logs, a crash alert, an Argo CD application, the palette), the block on a page or not.
 */
export const changesView = $state({ open: false, focus: null as ChangesFocus | null })

export function openChanges(focus: ChangesFocus) {
  Object.assign(changesView, { open: true, focus })
}

export function closeChanges() {
  const { open, focus } = changesView
  if (open && focus) rememberPanel({ key: `changes:${focus.env ?? ''}:${focus.app ?? ''}`, kind: 'changes', title: `${focus.env ?? ''}`, hint: focus.app ?? '', reopen: () => openChanges(focus) })
  Object.assign(changesView, { open: false, focus: null })
}

/** The Changes block is on (its state published). */
export const changesOn = () => !!live.snapshot?.blocks?.changes

/** What changed around the crash of `app` on `env`: from 2 h before its first crash, the change linked to it highlighted. */
export function openWhatChanged(env: string | null, app: string) {
  const state = live.snapshot?.blocks?.changes as ChangesState | undefined
  const a = around(state?.events ?? [], env, app, Date.now())
  openChanges({ env, app, range: a.range, highlight: a.highlight, crash: a.crash })
}
