import type { CustomBlocksState } from '../../bindings/CustomBlocksState'
import type { CustomBlockView } from '../../bindings/CustomBlockView'
import type { CustomItem } from '../../bindings/CustomItem'
import type { CustomLevel } from '../../bindings/CustomLevel'
import type { CustomRead } from '../../bindings/CustomRead'
import { stateOf } from '../block'

export type { CustomBlocksState, CustomBlockView, CustomItem, CustomLevel, CustomRead }
export type { CustomBlockDef } from '../../bindings/CustomBlockDef'
export type { CustomAction } from '../../bindings/CustomAction'

/** The Rust block holding every custom block (`Snapshot.blocks.custom_blocks`). */
export const SOURCE = 'custom_blocks'

export const customState = (snap: { blocks?: Record<string, unknown> } | null | undefined) => stateOf<CustomBlocksState>({ id: SOURCE }, snap)

/** One custom block by its own id (`backups`, not `custom:backups`). */
export const customBlock = (snap: { blocks?: Record<string, unknown> } | null | undefined, id: string) => customState(snap)?.blocks.find((b) => b.id === id)

/** The tone class of a level (`.ok`, `.warn`, `.ko`, `.info`). */
export const tone = (l: CustomLevel | null | undefined) => l ?? ''

/** The worst level of what a block shows: its error, its alerts, its lines' or stats' states. */
export function worst(b: CustomBlockView): CustomLevel | null {
  if (b.error) return 'ko'
  const levels: (CustomLevel | undefined)[] = [b.output?.status?.state, ...(b.output?.alerts.map((a) => a.severity) ?? [])]
  const body = b.output?.body
  if (body?.kind === 'list') levels.push(...body.items.map((i) => i.state))
  if (body?.kind === 'table') levels.push(...body.rows.map((i) => i.state))
  if (body?.kind === 'stats') levels.push(...body.stats.map((s) => s.state ?? undefined))
  for (const l of ['ko', 'warn', 'ok'] as const) if (levels.includes(l)) return l
  return null
}

/** A field of a line as text. */
export const field = (i: CustomItem, key: string): string => {
  const v = i.fields[key]
  return v === null || v === undefined ? '' : String(v)
}
