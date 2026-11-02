import { invoke } from './bridge'
import { live } from './live.svelte'
import type { AgentCost } from './types'

/**
 * The book of what every agent cost (`agent_costs`), out of the snapshot: read by the Actual
 * sections when they show, and again when the snapshot says it changed (`agent_costs_rev`, which
 * a working agent moves on each tick), at most every `EVERY`. Shared by every section shown.
 */
const EVERY = 10_000

let costs = $state<AgentCost[]>([])
/** The revision the book was read at, the one being read, when the last read began. */
let readRev: number | null = null
let reading: number | null = null
let readAt = 0
let timer: ReturnType<typeof setTimeout> | null = null

function read(rev: number) {
  reading = rev
  readAt = Date.now()
  invoke<AgentCost[]>('agent_costs')
    .then((list) => {
      costs = list
      readRev = rev
    })
    .catch(() => {})
    .finally(() => {
      if (reading === rev) reading = null
    })
}

/** Reads the book if it changed since: at once, or once `EVERY` has passed since the last read. */
function follow(rev: number) {
  if (rev === readRev || rev === reading) return
  if (timer) clearTimeout(timer)
  const wait = readRev === null ? 0 : readAt + EVERY - Date.now()
  if (wait <= 0) return read(rev)
  timer = setTimeout(() => {
    timer = null
    const now = live.snapshot?.agent_costs_rev
    if (now !== undefined) follow(now)
  }, wait)
}

/** The book, kept up to date while the component that calls this (in its script) is shown. */
export function useAgentCosts(): { readonly list: AgentCost[] } {
  $effect(() => {
    const rev = live.snapshot?.agent_costs_rev
    if (rev !== undefined) follow(rev)
  })
  return {
    get list() {
      return costs
    },
  }
}
