import { untrack } from 'svelte'
import { agentsView, type AgentsLayout } from './agents.svelte'
import { pref, setPref } from './layout.svelte'
import { isMac } from './platform'
import { waitsForYou } from './rules'
import type { Agent } from './types'
import { t } from './i18n.svelte'

/**
 * Agents side by side: 1, 2 (columns or rows) or 4 panes, each a live agent (`agents.layout`,
 * `agents.panes`, remembered). The focused pane is the selected agent's: the list, ⌘[ / ⌘], ⌘.
 * and the header act on it, and an agent selected elsewhere opens there. An agent shows in one
 * pane at most; its terminal stays mounted wherever it goes, only its place in the grid changes.
 * Its panel (Diff, Activity…) opens inside its pane, in place of its terminal. ⌘↵ zooms the
 * focused pane over the whole view, for a while.
 */
export const LAYOUTS: [AgentsLayout, string, string][] = [
  ['1', t('One agent'), '⌃1'],
  ['2', t('Two side by side'), '⌃2'],
  ['2r', t('Two one above the other'), ''],
  ['4', t('Four in a grid'), '⌃4'],
]
export const layoutHint = (k: string) => (k ? (isMac ? k : k.replace('⌃', 'Ctrl+Alt+')) : '')
export const TEMPLATE: Record<AgentsLayout, string> = { '1': "'p0' minmax(0, 1fr) / minmax(0, 1fr)", '2': "'p0 p1' minmax(0, 1fr) / minmax(0, 1fr) minmax(0, 1fr)", '2r': "'p0' minmax(0, 1fr) 'p1' minmax(0, 1fr) / minmax(0, 1fr)", '4': "'p0 p1' minmax(0, 1fr) 'p2 p3' minmax(0, 1fr) / minmax(0, 1fr) minmax(0, 1fr)" }

/**
 * Follow who waits, one step: an agent waiting for you comes into an empty pane (else the last,
 * unless it is focused or waiting too); one it brought, answered, leaves its pane empty. `followed`:
 * those it brought; `dismissed`: those taken out by hand, not brought back while they still wait.
 */
export function followStep(visible: (string | null)[], waits: Set<string>, focusAt: number, followed: string[], dismissed: string[]) {
  const next = [...visible]
  let kept = followed
  next.forEach((id, i) => {
    if (id && kept.includes(id) && !waits.has(id) && i !== focusAt) {
      next[i] = null
      kept = kept.filter((x) => x !== id)
    }
  })
  const gone = dismissed.some((id) => !waits.has(id)) ? dismissed.filter((id) => waits.has(id)) : dismissed
  const incoming = [...waits].find((id) => !next.includes(id) && !gone.includes(id))
  if (incoming) {
    const empty = next.findIndex((id, i) => !id && i !== focusAt)
    const last = next.length - 1
    const target = empty >= 0 ? empty : last !== focusAt && !(next[last] && waits.has(next[last]!)) ? last : -1
    if (target >= 0) {
      next[target] = incoming
      kept = [...kept, incoming]
    }
  }
  return { next, followed: kept, dismissed: gone }
}

/** The panes of the Agents view; made in its component (its effects live as long as it does). */
export class AgentPanes {
  layout = $state<AgentsLayout>(pref<AgentsLayout>('agents.layout', '1'))
  #panes = $state<(string | null)[]>(pref<(string | null)[]>('agents.panes', []))
  /** The focused pane. */
  slot = $state(0)
  zoomed = $state(false)
  follow = $state(pref<boolean>('agents.follow', false))
  #followed = $state<string[]>([])
  #dismissed = $state<string[]>([])
  count = $derived(this.layout === '1' ? 1 : this.layout === '4' ? 4 : 2)
  /** The agent of each pane shown (`null`: empty). */
  visible = $derived(Array.from({ length: this.count }, (_, i) => this.#panes[i] ?? null))

  agents: () => Agent[]
  select: (id: string) => void

  /** `agents`: the agents as they are; `select`: an agent selected, the view's way. */
  constructor(agents: () => Agent[], select: (id: string) => void) {
    this.agents = agents
    this.select = select
    // The selected agent is the focused pane's: one selected elsewhere (the list, a block) opens there.
    $effect(() => {
      const id = agentsView.selected
      if (this.count === 1 || !id || !this.agents().some((a) => a.id === id)) return
      const at = this.visible.indexOf(id)
      if (at >= 0) {
        if (untrack(() => this.slot) !== at) this.slot = at
        return
      }
      const next = [...this.visible]
      next[untrack(() => this.slot)] = id
      this.#set(next)
    })
    // A closed agent leaves its pane empty.
    $effect(() => {
      const agents = this.agents()
      if (!agents.length) return
      if (this.visible.some((id) => id && !agents.some((a) => a.id === id))) this.#set(this.visible.map((id) => (id && agents.some((a) => a.id === id) ? id : null)))
    })
    $effect(() => {
      if (!this.follow || this.count === 1) return
      const waits = new Set(this.agents().filter((a) => a.state === 'running' && waitsForYou(a.status)).map((a) => a.id))
      const visible = this.visible
      const step = untrack(() => followStep(visible, waits, this.slot, this.#followed, this.#dismissed))
      this.#followed = step.followed
      if (step.dismissed !== untrack(() => this.#dismissed)) this.#dismissed = step.dismissed
      this.#set(step.next)
    })
  }

  #set(next: (string | null)[]) {
    if (JSON.stringify(next) === JSON.stringify(this.#panes)) return
    setPref('agents.panes', (this.#panes = next))
  }

  setLayout(l: AgentsLayout) {
    setPref('agents.layout', (this.layout = l))
    this.zoomed = false
    if (this.slot >= this.count) this.slot = 0
  }

  setFollow(on: boolean) {
    setPref('agents.follow', (this.follow = on))
  }

  /** An agent into a pane, swapping places with what it was shown in; the pane takes the focus. */
  placeInto(i: number, id: string) {
    const next = [...this.visible]
    const at = next.indexOf(id)
    if (at >= 0) next[at] = next[i]
    next[i] = id
    this.#followed = this.#followed.filter((x) => x !== id)
    this.#dismissed = this.#dismissed.filter((x) => x !== id)
    this.#set(next)
    this.slot = i
    this.select(id)
  }

  /** A click in the list: the focused pane; ⌘-click: an empty pane, otherwise the next one. */
  pick(ev: MouseEvent, id: string) {
    // A Claude Desktop session has no terminal here: it never goes into a pane.
    if (this.count > 1 && (ev.metaKey || ev.ctrlKey) && this.agents().some((a) => a.id === id)) {
      const empty = this.visible.indexOf(null)
      this.placeInto(empty >= 0 ? empty : (this.slot + 1) % this.count, id)
    } else this.select(id)
  }

  focus(i: number) {
    const id = this.visible[i]
    if (!id || id === agentsView.selected) return
    this.slot = i
    this.select(id)
  }

  /** Empties a pane (the agent goes on); the focus moves to another one if it was there. */
  close(i: number) {
    const next = [...this.visible]
    next[i] = null
    if (this.visible[i]) this.#dismissed = [...this.#dismissed, this.visible[i]!]
    if (this.visible[i] === agentsView.selected) {
      const other = next.findIndex(Boolean)
      if (other < 0) return
      this.slot = other
      agentsView.selected = next[other]
    }
    this.#set(next)
  }

  /** ⌥+arrows: the next agent on that side, skipping empty panes. */
  move(arrow: 'ArrowLeft' | 'ArrowRight' | 'ArrowUp' | 'ArrowDown') {
    const cols = this.layout === '2' || this.layout === '4' ? 2 : 1
    const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -cols, ArrowDown: cols }[arrow]
    for (let n = 1, i = this.slot; n < this.count; n++) {
      i = (i + step + this.count) % this.count
      if (this.visible[i]) return this.focus(i)
    }
  }
}
