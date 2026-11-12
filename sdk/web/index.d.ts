/**
 * `@prism/sdk`: what a module's components (`components.js`) reach of Prism, at run time, as
 * Prism's own instances (one Svelte, one snapshot, one set of preferences). Its names are those of
 * `exports.json` (a test of Prism keeps them one); a name added is added to the module contract.
 */
import type { Component } from 'svelte'
import type { Snapshot } from './types/bindings/Snapshot'
import type { Alert } from './types/bindings/Alert'
import type { ArgoApp } from './types/bindings/ArgoApp'
import type { ArgoCd } from './types/bindings/ArgoCd'
import type { ArgoInstance } from './types/bindings/ArgoInstance'
import type { Conventions } from './types/bindings/Conventions'
import type { EnvironmentStage } from './types/bindings/EnvironmentStage'
import type { ForgeProvider } from './types/bindings/ForgeProvider'
import type { ManualJob } from './types/bindings/ManualJob'
import type { PlayedJob } from './types/bindings/PlayedJob'
import type { IconName, LogoName } from './types/icons'

export type { Snapshot, Alert, ArgoApp, ArgoCd, ArgoInstance, Conventions, EnvironmentStage, ForgeProvider, IconName, LogoName, ManualJob, PlayedJob }
export type { MergeRequest } from './types/bindings/MergeRequest'
export type { Pipeline } from './types/bindings/Pipeline'
export type { Vars } from './words'

// Words.
export { t, tc, tn } from './words'

// Blocks: what `blocks()` of `defs.js` returns, what a block's component receives.
export interface BlockHelp {
  purpose: string
  points: string[]
  source: string
}

export interface BlockDef<S = unknown> {
  id: string
  label: string
  /** Its group in + Block (`Fun`, `Code`, `Infra`…). */
  category?: string
  /** The source its errors go under, when not its id. */
  source?: string
  help: BlockHelp
  boot?: { label: string; detail: (state: S) => string }
  badge?: (state: S) => { count: number; title: string }
  provides?: ('stages' | 'images')[]
  aliases?: string[]
  /** Its status-bar item (`<id>:status` of `components`). */
  status?: { label: string; side: 'left' | 'right'; after?: string }
  /** Shown when Prism publishes no state for it (its settings off). */
  off?: { title: string; logo: string; color: string; section: string; setting: string }
  alerts?: (state: S) => Alert[]
  /** A guide of Help (`NN-id.md`, the same format as Prism's): its file name, for its order, and its text. */
  guide?: { file: string; text: string; images?: Record<string, string>; translations?: Partial<Record<string, string>> }
  /** Its settings page: its config tables' sections, under the Settings node `parent` (`ci`, `infra`…). */
  settings?: { sections: SettingsSection[]; parent: string; node: SettingsNode }
}

/** The kinds of a settings field. */
export type SettingsKind = 'text' | 'longtext' | 'path' | 'number' | 'toggle' | 'list' | 'select' | 'checks' | 'table'
/** The agent CLIs, as `[agents] provider` names them. */
export type AgentCli = 'claude' | 'command-code' | 'opencode' | 'codex'
/** Choice lists read from the computer. */
export type SettingsSource = 'kube' | 'apps' | 'envs' | 'stages' | 'calendars' | 'kinds' | `models:${AgentCli}` | `efforts:${AgentCli}`
/** A live check of a value; `kube_namespace` takes its context from the neighboring key `context`. */
export type SettingsCheck = 'gitlab_project' | 'kube_context' | 'git_repo' | 'directory' | 'gcloud' | 'agent_tools' | 'agent_cli' | { kind: 'kube_namespace'; context: string } | { kind: 'agent_bin'; cli: AgentCli }

/** A column of a `table` field: a key of each row. */
export interface SettingsColumn {
  key: string
  label: string
  kind: Exclude<SettingsKind, 'checks' | 'table'>
  placeholder?: string
  /** Empty: the key is removed from the row. */
  optional?: boolean
  options?: SettingsSource | string[]
  width?: string
  check?: SettingsCheck
}

/** A field of a settings section: a key of its table. */
export interface SettingsField {
  key: string
  label: string
  hint?: string
  kind: SettingsKind
  unit?: string
  /** Step of a number; absent: integer. */
  step?: number
  placeholder?: string
  optional?: boolean
  /** Fixed choices, or a list read from the computer. */
  options?: SettingsSource | string[]
  /** `checks`: the words shown for each option, by value. */
  labels?: Record<string, string>
  /** `table`: its columns, and a new row's value. */
  columns?: SettingsColumn[]
  blank?: Record<string, unknown>
  check?: SettingsCheck
}

/** A section of Settings: a config table (`[shipping]`, `[shipping.branch]`), its fields. */
export interface SettingsSection {
  id: string
  label: string
  table: string
  path: string[]
  intro?: string
  /** The source whose last state is shown at its top. */
  source?: string
  fields: SettingsField[]
}

/** A page of Settings: its sections; `toggle` the section whose `enabled` switches it. */
export interface SettingsNode {
  id: string
  label: string
  sections?: string[]
  toggle?: string
  tabs?: boolean
}

export interface BlockProps<S> {
  state: S | undefined
  error?: string
  snap: Snapshot
}

export interface StatusProps {
  snap: Snapshot
  now: number
}

export type BlockComponent<S> = Component<BlockProps<S>>

// Data: the snapshot, a block's state, its commands.
export declare const live: { readonly snapshot: Snapshot | null; readonly connected: boolean }
/** A command of a block's back end (`Block::call`): its answer. */
export declare const call: <T>(block: string, command: string, args?: Record<string, unknown>) => Promise<T>
export declare const stateOf: <S>(def: Pick<BlockDef, 'id' | 'source'>, snap: { blocks?: Record<string, unknown> } | null | undefined) => S | undefined
/** A core answers this page (not the browser preview). */
export declare const hasCore: () => boolean
export declare const featureOn: (f: 'boot' | 'animations' | 'theme_motion' | 'celebrations' | 'easter_eggs' | 'friday_guard') => boolean

// Layout: the block's size, preferences, where it is placed.
export interface View {
  id: string
  label: string
  icon: string
  color: string
  columns: string[][]
  bands?: number[]
  sizes: { columns: number[]; rows: number[][]; bands?: number[] }
}
export declare const columnsOf: (v: View) => string[][]
export declare const isCollapsed: (id: string) => boolean
export declare const isShort: (id: string) => boolean
export declare function pref<T>(key: string, fallback: T): T
export declare function setPref(key: string, value: unknown): void
export declare function showBlock(id: string): void
export declare function views(): View[]

// Prism's actions: a link, an MR, a ticket, a job's logs, a toast.
export declare function linkOnly(e: MouseEvent): void
export declare function openLink(href: string): void
export declare function openMr(ref: { iid: number; repo?: string | null; forge?: ForgeProvider | null }): void
export declare const canPreview: () => boolean
export declare function previewClick(id: string): (e: MouseEvent) => void
export declare const canShowLogs: () => boolean
export declare function logsClick(url: string, label: string, project?: string | null): (e: MouseEvent) => void
export declare function openJob(jobId: number, label: string, project?: string | null): Promise<void>
export declare function openPipeline(pipelineId: number, label: string, project?: string | null): Promise<void>
export declare function toast(message: string, ms?: number, action?: { label: string; run: () => void } | null): void

// Popups and keys.
export declare function anchored(trigger: () => HTMLElement | null | undefined, placement?: { align?: 'start' | 'end' | 'center'; side?: 'below' | 'right'; gap?: number }): (el: HTMLElement) => () => void
export declare function exclusive(close: () => void): () => () => void
/** A shortcut as this computer writes it (`⌘K`, `Ctrl+K`). */
export declare function keys(text: string): string

// Formats.
export declare function locale(): string | undefined
export declare function ago(iso: string | null | undefined): string
export declare const minutesSince: (iso: string | null | undefined) => number

// The forge and the team's conventions; ArgoCD.
export declare const conventions: () => Conventions
export declare const isRelease: (branch: string) => boolean
export declare const ticketPrefix: () => string
export declare const mrNumber: (iid: number | string, forge?: ForgeProvider | null) => string
export interface ArgoRef {
  app: ArgoApp
  instance: ArgoInstance
}
export declare const argoUrl: (r: ArgoRef) => string | null
export declare function findApp(argo: ArgoCd | null | undefined, component: string, env: EnvironmentStage['role']): ArgoRef | null
export declare function rollout(r: ArgoRef, expected: string | null): { tone: 'ok' | 'warn' | 'ko' | 'info'; label: string; live: boolean }

// Components.
export declare const Icon: Component<{ name: IconName; size?: string | number; spin?: boolean; stroke?: number; title?: string; class?: string }>
export declare const Logo: Component<{ name: LogoName | 'agent' | 'commandcode' | 'codex' | 'alert'; size?: number; repo?: string }>
export declare const BlockControls: Component<{ id: string }>
export declare const StageManualJobs: Component<{ stage: string; jobs: ManualJob[]; played?: PlayedJob[]; button?: string; heading?: string; playedHeading?: string; labelOf?: (j: ManualJob | PlayedJob) => string; risky?: boolean }>
