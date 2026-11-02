import { hasCore, invoke, isDesktop } from './bridge'
import { live } from './live.svelte'
import { agentsSettings, type AgentConversation } from './agents.svelte'
import { CLIS, nameOf } from './agent-cli'
import { sendPrompt } from './agent-inbox.svelte'
import { isWorking } from './rules'
import { USED_UP, planUseOf } from './usage'
import { INCLUDE_ALL, handoffs, relayText, suggestTarget, summaryRequest, type Handoff, type Include } from './handoff'
import { mrKey } from './mr-ref'
import { toast } from './toast.svelte'
import { t } from './i18n.svelte'
import type { Agent, AgentProvider, HandoffMaterial } from './types'

/**
 * Continue with… (agent_switch): another CLI takes an agent on, in its folder, from a relay — the
 * words it starts from (`handoff.ts`). Its window (AgentSwitch.svelte, on the Mac and the phone)
 * opens from the agent's CLI chip, its ⋯ menu, the palette, its limit banner and the alert; Back
 * to a CLI it ran before from its Activity tab. `[agents] handoff` picks the relay first,
 * `handoff_review` off sends it without the window when nothing is to wait for.
 */
export type Reason = 'limit' | 'choice'

export const switching = $state({
  open: false,
  id: '',
  to: null as AgentProvider | null,
  reason: 'choice' as Reason,
  handoff: 'prism' as Handoff,
  include: { ...INCLUDE_ALL } as Include,
  material: null as HandoffMaterial | null,
  /** The relay as it will go; `edited`: changed by hand, no longer written again from the material. */
  text: '',
  edited: false,
  model: '',
  effort: '',
  mode: '',
  busy: false,
  error: null as string | null,
  /** Relay `agent`: the summary asked of the former agent, since when (its turn read once over). */
  asked: null as number | null,
})

let installed = $state<AgentProvider[] | null>(null)
let loadingInstalled = false

/** The CLIs found on the Mac (read once): those an agent can continue with. */
export function installedClis(): AgentProvider[] | null {
  if (installed === null && !loadingInstalled && hasCore()) {
    loadingInstalled = true
    invoke<AgentProvider[]>('agent_installed').then(
      (list) => (installed = list),
      () => (installed = []),
    ).finally(() => (loadingInstalled = false))
  }
  return installed
}

/** How much of `p`'s plan is used now (its fullest window), when Prism reads it. */
export const planUse = (p: AgentProvider): number | null => (live.snapshot ? planUseOf(live.snapshot, p) : null)

/** Its CLI's plan is used up and it is not working: another CLI is offered (`[agents] suggest_switch`). */
export const limitReached = (a: Agent) => (agentsSettings().suggest_switch ?? true) && a.state !== 'prepared' && !isWorking(a.status) && (planUse(a.provider) ?? 0) >= USED_UP

/** The CLI offered when its own is used up: the least used of the others found. */
export const suggestedFor = (a: Agent) => suggestTarget(a.provider, installedClis() ?? [], planUse)

/** The CLIs it can continue with: the others found on the Mac (all of them until that is known). */
export const targetsOf = (a: Agent): AgentProvider[] => (installedClis() ?? (Object.keys(CLIS) as AgentProvider[])).filter((p) => p !== a.provider)

const agentOf = (id: string) => live.snapshot?.agents?.find((a) => a.id === id)
const lang = () => agentsSettings().prompt_language ?? 'en'

/** Writes the relay again from the material, unless it was changed by hand. */
export function compose() {
  const a = agentOf(switching.id)
  const m = switching.material
  if (!a || !m || switching.edited) return
  switching.text = relayText(m, switching.handoff, { from: nameOf(a), reason: switching.reason, ticket: a.ticket, mr: a.mr ? mrKey(a.mr) : null }, lang(), switching.include)
}

async function loadMaterial() {
  const id = switching.id
  try {
    const m = await invoke<HandoffMaterial>('agent_handoff_material', { id, turns: agentsSettings().handoff_turns ?? 8 })
    if (switching.id !== id) return
    switching.material = m
    if (!handoffs(m.readable).includes(switching.handoff)) switching.handoff = 'task'
    compose()
  } catch (e) {
    switching.error = String(e)
  }
}

/** Opens its window: to `to` (failing that the one suggested), for `reason`; `show` false: its state only. */
export function openSwitch(a: Agent, to?: AgentProvider | null, reason: Reason = 'choice', show = true): Promise<void> {
  const target = to ?? suggestedFor(a) ?? targetsOf(a)[0] ?? null
  Object.assign(switching, { open: show, id: a.id, to: target, reason, handoff: (agentsSettings().handoff ?? 'prism') as Handoff, include: { ...INCLUDE_ALL }, material: null, text: '', edited: false, model: '', effort: '', mode: a.launch?.permission_mode ?? '', busy: false, error: null, asked: null })
  // Its own summary needs it to answer: not at its limit.
  if (switching.handoff === 'agent' && reason === 'limit') switching.handoff = 'prism'
  return loadMaterial()
}

export function closeSwitch() {
  switching.open = false
  switching.asked = null
}

/** Relay picked: written again from the material (the hand edits dropped). */
export function pickHandoff(h: Handoff) {
  switching.handoff = h
  switching.edited = false
  switching.asked = null
  compose()
}

/**
 * Relay `agent`: the former agent is asked for its summary (a prompt, as any other: queued while
 * its menu is open; from a phone, typed to it), its next answer read as the relay once its turn is over.
 */
export async function askSummary() {
  const a = agentOf(switching.id)
  if (!a) return
  const text = summaryRequest(lang())
  try {
    if (hasCore() && !isDesktop()) await invoke('agent_type', { id: a.id, text })
    else await sendPrompt(a.id, text)
    switching.asked = Date.now()
    switching.error = null
  } catch (e) {
    switching.error = String(e)
  }
}

/** Its summary asked, its turn over since: the material read again, its last answer the relay. */
export async function summaryLanded(a: Agent): Promise<boolean> {
  if (!switching.asked || a.id !== switching.id || isWorking(a.status) || a.state !== 'running') return false
  const since = a.last_activity ? Date.parse(a.last_activity) : 0
  // Its turn has not started yet: the prompt is still on its way.
  if (since < switching.asked) return false
  switching.asked = null
  switching.edited = false
  await loadMaterial()
  return true
}

/** Hands it over: the relay as shown (or, unseen, as written now). */
export async function confirmSwitch(): Promise<boolean> {
  const a = agentOf(switching.id)
  if (!a || !switching.to || !switching.text.trim()) return false
  switching.busy = true
  switching.error = null
  try {
    await invoke('agent_switch', {
      id: a.id,
      switch: { to: switching.to, message: switching.text, handoff: switching.handoff, reason: switching.reason, model: switching.model || null, effort: switching.effort || null, permission_mode: switching.mode || null },
    })
    toast(t('{title} continues with {cli}', { title: a.title, cli: CLIS[switching.to].cli }), 4000)
    closeSwitch()
    return true
  } catch (e) {
    switching.error = String(e)
    return false
  } finally {
    switching.busy = false
  }
}

/**
 * Continue with `to` from a menu: its window, or straight away when `[agents] handoff_review` is
 * off and the relay is Prism's or the task (the agent's own summary is always waited for there).
 */
export async function continueWith(a: Agent, to: AgentProvider, reason: Reason = 'choice') {
  // The agent's own summary is waited for in the window.
  const review = (agentsSettings().handoff_review ?? true) || (agentsSettings().handoff ?? 'prism') === 'agent'
  await openSwitch(a, to, reason, review)
  if (review) return
  if (switching.error) return toast(switching.error, 6000)
  if (!(await confirmSwitch())) toast(switching.error ?? t('Could not hand it over'), 6000)
}

/** Back to the CLI of its stretch `leg`: the same window, that CLI picked. */
export const backTo = (a: Agent, to: AgentProvider) => openSwitch(a, to, 'choice')

/** The conversation of its stretch `leg`, for its Chat's earlier turns. */
export const legConversation = (id: string, leg: number, turns = 30) => invoke<AgentConversation | null>('agent_leg_conversation', { id, leg, turns })
