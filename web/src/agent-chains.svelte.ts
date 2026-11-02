import { invoke } from './bridge'
import { live } from './live.svelte'
import { toast } from './toast.svelte'
import type { Chain } from './bindings/Chain'
import type { StepKind } from './bindings/StepKind'
import type { StepWhen as When } from './bindings/StepWhen'
import type { LaunchStep as Step } from './bindings/LaunchStep'
import { t } from './i18n.svelte'

/**
 * Chains: steps that follow an agent once it has done its work ("when it's done, then…"), set in
 * the launcher. Each step starts when the one before has finished (`done`), only if it succeeded
 * (`ok`), or waits for your go (`ask`). A task failing goes back to the agent once, with the end of
 * its output, then runs again; failing twice, the chain stops there. Nothing merges, nothing pushes
 * to a protected branch (`agent_open_mr` refuses).
 *
 * Prism runs them (`prism_agents::chains`), whatever the window shows: a launch's "Then" steps
 * start with it, from the Mac or a phone. Their state comes with the snapshot (`chains`); Start,
 * Skip, Stop and Hide go to Prism (`chain_go`, `chain_order`).
 */

export type { StepKind } from './bindings/StepKind'
export type { StepWhen as When } from './bindings/StepWhen'
/** A step: what it does, when (`kind`, `when`); a task's command; a prompt's or a new agent's text (`{mr}`, `{ticket}`, `{branch}` filled in). */
export type { LaunchStep as Step } from './bindings/LaunchStep'
export type { Chain }
export type { RunState } from './bindings/RunState'

export const STEP_LABEL: Record<StepKind, string> = { task: t('Run a task'), prompt: t('Prompt this agent'), mr: t('Push and open a draft MR'), agent: t('Launch a review agent'), notify: t('Notify me') }
export const WHEN_LABEL: Record<When, string> = { done: t('once it finished'), ok: t('if it succeeded'), ask: t('ask me first') }

/** Ready-made chains, for the launcher. */
export const RECIPES: { name: string; steps: Step[] }[] = [
  { name: t('Test → draft MR'), steps: [{ kind: 'task', when: 'done', command: 'npm test' }, { kind: 'mr', when: 'ok' }, { kind: 'notify', when: 'done' }] },
  { name: t('Test → MR → review'), steps: [{ kind: 'task', when: 'done', command: 'npm test' }, { kind: 'mr', when: 'ok' }, { kind: 'agent', when: 'ask', text: 'Review the code of MR !{mr}.' }] },
  { name: t('Self-review'), steps: [{ kind: 'prompt', when: 'done', text: 'Review your own changes as a strict reviewer would, then fix what you find.' }, { kind: 'notify', when: 'done' }] },
]

const order = (id: string, action: 'go' | 'skip' | 'stop' | 'forget') =>
  (action === 'go' ? invoke('chain_go', { id }) : invoke('chain_order', { id, action })).catch((e) => toast(String(e), 5000))

export const chainOf = (id: string): Chain | undefined => live.snapshot?.chains?.[id]
/** The chain still has something to do. */
export const chainActive = (c: Chain) => !c.stopped && c.current < c.steps.length

export const stopChain = (id: string) => void order(id, 'stop')
export const forgetChain = (id: string) => void order(id, 'forget')
/** A held step: your go, or skipping it (the next step then counts it as failed for `ok`). */
export const goHeld = (id: string) => void order(id, 'go')
export const skipHeld = (id: string) => void order(id, 'skip')
