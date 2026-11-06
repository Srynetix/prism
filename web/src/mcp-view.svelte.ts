import { invoke } from './bridge'
import { t } from './i18n.svelte'
import { toast } from './toast.svelte'
import type { Proposal } from './bindings/Proposal'
import type { Snapshot } from './bindings/Snapshot'

/**
 * Prism's MCP server, as the window shows it: the proposals of agents (`prism mcp`) waiting for an
 * answer, one drawer for them (McpProposal.svelte), opened from the status bar or the toast that
 * tells a new one.
 */
export const mcpView = $state({ open: false, id: 0 })

export function openProposal(id?: number) {
  Object.assign(mcpView, { open: true, id: id ?? 0 })
}

export const closeProposal = () => (mcpView.open = false)

/** The proposals waiting, oldest first. */
export const proposals = (s: Snapshot | null | undefined): Proposal[] => s?.mcp?.proposals ?? []

/** The one the drawer shows: the one asked for, or the oldest. */
export function shown(s: Snapshot | null | undefined): Proposal | undefined {
  const list = proposals(s)
  return list.find((p) => p.id === mcpView.id) ?? list[0]
}

/** What a proposal is, in a few words. */
export function title(p: Proposal): string {
  if (p.kind === 'custom_block') return t('New custom block #{id}', { id: p.id })
  if (p.kind === 'layout') return t('Pages proposal #{id}', { id: p.id })
  return t('Config proposal #{id}', { id: p.id })
}

/** Lines added and removed. */
export function counts(p: Proposal): { add: number; del: number } {
  return { add: p.diff.filter((l) => l.op === '+').length, del: p.diff.filter((l) => l.op === '-').length }
}

/** The answer, given back to the agent as its tool's result. */
export async function answer(id: number, action: 'apply' | 'refuse' | 'modified', extra: { reason?: string; text?: string } = {}) {
  await invoke('mcp_proposal_answer', { id, action, reason: extra.reason ?? null, text: extra.text ?? null })
}

/** A new proposal arrived: said once, with a button to open it. */
let seen = new Set<number>()
let primed = false
export function watchProposals(s: Snapshot | null | undefined) {
  // No snapshot yet: nothing seen, the first one is the launch's.
  if (!s) return
  const list = proposals(s)
  const fresh = list.filter((p) => !seen.has(p.id))
  seen = new Set(list.map((p) => p.id))
  // The ones there at launch are not news.
  if (!primed) {
    primed = true
    return
  }
  const p = fresh[fresh.length - 1]
  if (p && !mcpView.open) toast(t('{who} proposes a change of Prism', { who: p.client }), 8000, { label: t('Open'), run: () => openProposal(p.id) })
}
