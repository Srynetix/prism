import { agentsSettings, setAgentEffort } from './agents.svelte'
import { choices, effortsKey, loadEfforts } from './settings.svelte'
import type { Agent } from './types'
import { t } from './i18n.svelte'
import { cliOf } from './agent-cli'

/**
 * An agent's reasoning effort (Claude Code, Codex), on the Mac (its header) and on a phone (its
 * ⋯ menu): its own, else its CLI's (`[agents.<cli>] effort`), its levels as its CLI tells them.
 */
export const takesEffort = (a: Agent) => cliOf(a.provider).takes_effort
/** Its own, picked in the launcher or its header; `null`: its CLI's. */
export const effortOf = (a: Agent) => a.launch?.effort ?? null
/** Its CLI's, from Settings; `null`: the CLI's default. */
export const cliEffort = (a: Agent) => agentsSettings()[a.provider]?.effort ?? null
/** The model its levels are read for: Codex's differ by model. */
const effortModel = (a: Agent) => (cliOf(a.provider).efforts_by_model ? (a.launch?.model ?? agentsSettings().codex?.model ?? null) : null)
/** Reads its levels once (from an effect or an attachment: it writes state). */
export const loadEffortsOf = (a: Agent) => loadEfforts(a.provider, effortModel(a))
/** Its levels, lowest first, its own included if its CLI no longer lists it. */
export function effortsOf(a: Agent): string[] {
  const levels = choices.efforts[effortsKey(a.provider, effortModel(a))] ?? []
  const own = effortOf(a)
  return own && !levels.includes(own) ? [...levels, own] : levels
}

/** Sets it (`null`: its CLI's); what to tell: at once (Claude Code at its prompt), or from its next resume. */
export async function pickEffort(a: Agent, effort: string | null): Promise<string> {
  const now = await setAgentEffort(a.id, effort)
  const what = effort ? t('Effort {effort}', { effort }) : t('Back to {effort}', { effort: cliEffort(a) ?? t('its CLI’s default') })
  if (now) return t('{what}: from its next message', { what })
  if (a.state !== 'running') return t('{what}: from its resume', { what })
  return a.provider === 'claude' ? t('{what}: from its next resume (it is working: pick it again once its turn is over to apply it now)', { what }) : t('{what}: from its next resume', { what })
}
