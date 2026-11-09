import { invoke } from '../bridge'
import type { Component } from 'svelte'
import type { BlockHelp } from '../block-help'
import type { Alert } from '../rules'
import type { Snapshot } from '../types'
import type { SectionSpec } from '../settings-schema'
import type { TreeNode } from '../settings-tree'

/**
 * A block with a source of its own: its Rust side (`src-tauri/crates/blocks/src/<id>/`) publishes its
 * state in `Snapshot.blocks[id]` and answers its calls (`call`); its window side is a folder of
 * `web/src/blocks/`, declared once in `blocks/index.ts`. The core knows none of them by name:
 * the "+ Block" menu, Help, Settings and Alerts read their declaration.
 *
 * This file only holds types and the call: a block's declaration imports nothing of the
 * core at run time (the core imports the declarations first).
 */
export interface BlockDef<S = unknown> {
  id: string
  /** Its name, in the "+ Block" menu, Help and its alerts' pill. */
  label: string
  /** Its group in the "+ Block" menu: one of `BLOCK_CATEGORIES` (views.ts), or a new one, listed after them. */
  category?: string
  /** The Rust block whose state it shows (`Snapshot.blocks`, `errors`); absent: its own id. */
  source?: string
  help: BlockHelp
  /**
   * A guide of Help (`help/NN-id.md`, the same format): its file name, for its order, and its text.
   * `images`: the URLs of its images by the relative path its text gives them (`images/x.png`);
   * `translations`: its text in another language, by language (`fr`).
   */
  guide?: { file: string; text: string; images?: Record<string, string>; translations?: Partial<Record<string, string>> }
  /** Its line on the boot screen, while it is on: what it reads, then what it brought. */
  boot?: { label: string; detail: (state: S) => string }
  /** A count on the rail's icon of the pages showing it (broken things), with its tooltip. */
  badge?: (state: S) => { count: number; title: string }
  /**
   * What its Rust block declares for the core (`Block::stages`, `Block::images`): Settings offers
   * its `source` in `[clusters] stages` and `[vulnerabilities] images`.
   */
  provides?: ('stages' | 'images')[]
  /** Its former ids: a saved layout naming one shows this block instead. */
  aliases?: string[]
  /**
   * An item of its own in the status bar, placed after the item `after`; its component is
   * `components['<id>:status']` (`StatusProps`).
   */
  status?: { label: string; side: 'left' | 'right'; after?: string }
  /** What it shows while off (its state absent): how to turn it on (`BlockOff`). */
  off?: { title: string; logo: string; color: string; section: string; setting: string }
  /** Its Settings sections (its config table) and its page in the Settings tree, under `parent`. */
  settings?: { sections: SectionSpec[]; parent: string; node: TreeNode }
  /**
   * Its lines in Alerts, from its state; `notification`: a notification when broken. For a block
   * of another repository only: Prism's own say theirs in Rust (`Block::alerts`), in `Snapshot.alerts`.
   */
  alerts?: (state: S) => BlockAlert[]
}

export type BlockAlert = Omit<Alert, 'kind' | 'block'>

/**
 * What the window's grid gives a block's component: its state (absent: off), its read's error, and
 * the whole snapshot, for what it shows of the rest (MRs, pipelines…).
 */
export interface BlockProps<S> {
  state: S | undefined
  error?: string
  snap: Snapshot
}

export type BlockComponent<S> = Component<BlockProps<S>>

/** What the status bar gives a block's item: the snapshot, and the time it ticks at. */
export interface StatusProps {
  snap: Snapshot
  now: number
}

/** A block's state in the snapshot, by the id of its Rust side (`source`). */
export const stateOf = <S>(def: Pick<BlockDef, 'id' | 'source'>, snap: { blocks?: Record<string, unknown> } | null | undefined) => snap?.blocks?.[def.source ?? def.id] as S | undefined

/** A command of a block's Rust side (`Block::call`). */
export const call = <T>(block: string, command: string, args: Record<string, unknown> = {}) => invoke<T>('block_call', { block, command, args })
