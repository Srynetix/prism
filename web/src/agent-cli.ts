import { agentsEnabled, agentsSettings } from './agents.svelte'
import { live } from './live.svelte'
import type { Agent, AgentProvider } from './types'
import type { IconName } from './icons'
import { PROVIDER_SPECS } from './bindings/provider-specs'
import type { ProviderSpec } from './bindings/ProviderSpec'

/**
 * Vocabulary of the agents' CLI (`[agents] provider`): Claude Code, Command Code, OpenCode or Codex. What
 * Prism's agents run follows the provider, or their repository's (`[[repos]] agent`); what opens in
 * Claude Desktop (`[agents] target = "desktop"`, the desktop sessions) stays Claude.
 */

/** How a CLI shows; what it can do is Rust's (`Provider::spec`, `ProviderSpec`). */
interface Look {
  /** Short name, in counters and titles. */
  name: string
  /** Full name. */
  cli: string
  /** Its glyph (Icon); its logo is Logo's of the same name. */
  icon: IconName
  /** How it picks a conversation back up, as typed in a terminal. */
  resume: string
  /** Its key to grant a permission once. */
  allow: string
  /** Its colour, a theme's variable: Claude's own, the others borrowed. */
  color: string
}
type Cli = Look & ProviderSpec

const LOOKS: Record<AgentProvider, Look> = {
  claude: { name: 'Claude', cli: 'Claude Code', icon: 'claude', resume: 'claude --resume', allow: '“1. Yes”', color: 'var(--src-claude)' },
  'command-code': { name: 'Command Code', cli: 'Command Code', icon: 'commandcode', resume: 'cmd --session', allow: '“1. Yes”', color: 'var(--violet)' },
  opencode: { name: 'OpenCode', cli: 'OpenCode', icon: 'opencode', resume: 'opencode --session', allow: '“Allow once”', color: 'var(--text-muted)' },
  codex: { name: 'Codex', cli: 'Codex', icon: 'codex', resume: 'codex resume', allow: '“Yes, proceed (y)”', color: 'var(--src-k8s)' },
}

export const CLIS = Object.fromEntries(Object.entries(LOOKS).map(([p, look]) => [p, { ...look, ...PROVIDER_SPECS[p as AgentProvider] }])) as Record<AgentProvider, Cli>

/** The CLI of `[agents] provider`, or an agent's own (absent from an older Prism: Claude Code). */
export const cliOf = (provider?: AgentProvider | null): Cli => CLIS[provider ?? 'claude'] ?? CLIS.claude

/** The agent actions of the menus launch a Prism agent, not a Claude Desktop session. */
export const actionsInPrism = (): boolean => agentsEnabled() && agentsSettings().target === 'agent'

/** The CLI of the agents launched in repository `repo` (`[[repos]] agent`), failing that `[agents] provider`. */
export const providerOf = (repo?: string | null): AgentProvider =>
  (repo ? live.snapshot?.repositories?.find((r) => r.name === repo)?.agent : null) ?? agentsSettings().provider

/** Who runs the agent actions (in repository `repo`): the agents' CLI, or Claude (Desktop too). */
export const actorProvider = (repo?: string | null): AgentProvider => (actionsInPrism() ? providerOf(repo) : 'claude')

/** Short name of the agents' CLI, in counters and titles: Claude, Command Code, OpenCode, Codex. */
export const agentName = (): string => cliOf(agentsSettings().provider).name

/** Full name of the agents' CLI: Claude Code, Command Code, OpenCode, Codex. */
export const agentCli = (): string => cliOf(agentsSettings().provider).cli

/** Who does an agent action's job (in repository `repo`): the CLI of its agents, otherwise Claude (Desktop too). */
export const actorName = (repo?: string | null): string => cliOf(actorProvider(repo)).name

/** The glyph of the agents' CLI (Icon): Claude's asterisk, Command Code's ⌘, OpenCode's square, Codex's prompt. */
export const agentIcon = (): IconName => cliOf(agentsSettings().provider).icon

/** An agent's own CLI, whatever the setting is now: it keeps the one it was launched with. */
/** Its name as shown: its account's (`Claude pro`, `[[agents.accounts]]`), else its CLI's. */
export const nameOf = (a: Agent): string => a.account ?? cliOf(a.provider).name
