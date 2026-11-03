import { isDesktop } from './bridge'

/**
 * What was typed to an agent and not sent yet, per agent: it stays in its box when you switch
 * tab, agent or view, and over a reload (this window's convenience only). Cleared once sent,
 * forgotten after three days (an agent long gone, words pasted in it); on a phone, kept for its
 * tab only (`sessionStorage`), not in its browser for good.
 */
const KEY = 'prism.chat-drafts'
const KEPT = 3 * 24 * 3600_000

function store(): Storage | null {
  try {
    return isDesktop() ? localStorage : sessionStorage
  } catch {
    return null
  }
}

function load(): Record<string, { text: string; at: number }> {
  try {
    const v = JSON.parse(store()?.getItem(KEY) ?? '{}')
    if (!v || typeof v !== 'object') return {}
    const now = Date.now()
    return Object.fromEntries(Object.entries(v).filter((e): e is [string, { text: string; at: number }] => {
      const d = e[1] as { text?: unknown; at?: unknown }
      return typeof d?.text === 'string' && typeof d.at === 'number' && now - d.at < KEPT
    }))
  } catch {
    return {}
  }
}

// A phone's of a former Prism, kept in its browser for good: gone. (The computer's, words without
// their time, are not read.)
try {
  if (!isDesktop()) localStorage.removeItem(KEY)
} catch {
  // Storage unavailable.
}

const drafts = $state<Record<string, { text: string; at: number }>>(load())
let timer: ReturnType<typeof setTimeout> | undefined

export const draftOf = (id: string): string => drafts[id]?.text ?? ''

export function setDraft(id: string, text: string) {
  if (text) drafts[id] = { text, at: Date.now() }
  else delete drafts[id]
  clearTimeout(timer)
  timer = setTimeout(() => {
    try {
      store()?.setItem(KEY, JSON.stringify(drafts))
    } catch {
      // Storage unavailable: the draft stays for this run only.
    }
  }, 300)
}

/**
 * What was sent to an agent and is not in its transcript yet, per agent: shown in its Chat
 * meanwhile, still there when you come back from its terminal or another agent (this run only).
 * With the time of its Chat's last turn when it was sent: a later turn (or its own) means it landed.
 */
const pendings = $state<Record<string, { text: string; after?: number }>>({})

export const pendingOf = (id: string): string | null => pendings[id]?.text ?? null
export const pendingAfter = (id: string): number | undefined => pendings[id]?.after

export function setPending(id: string, text: string | null) {
  if (text) pendings[id] = { text }
  else delete pendings[id]
}

export function setPendingAfter(id: string, after: number) {
  if (pendings[id]) pendings[id].after = after
}
