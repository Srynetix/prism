import { hasCore, invoke, isDesktop } from './bridge'
import { live } from './live.svelte'
import { agentsEnabled, agentsSettings } from './agents.svelte'
import { t } from './i18n.svelte'
import { toast } from './toast.svelte'
import type { Agent } from './types'
import type { Snapshot } from './bindings/Snapshot'
import type { AssistantProposal } from './bindings/AssistantProposal'
import type { Fact } from './bindings/Fact'
import type { FactKind } from './bindings/FactKind'
import type { AssistantMemory } from './bindings/AssistantMemory'

/**
 * Prism's assistant (`[agents.assistant]`): asked in your own words from the palette (« Ask
 * Prism »), a new conversation each time — an agent of the configured CLI in its own folder, with
 * Prism's MCP server, its tools and its memory. Its conversations and its memory live in the
 * Assistant panel (AssistantPanel.svelte); its proposals (a ticket, an agent, a note, a repository,
 * a fact) are cards in its Chat, applied in a click (AssistantCard.svelte).
 */

export const assistantView = $state({ open: false, selected: null as string | null, tab: 'conversations' as 'conversations' | 'memory', asking: false })

/** The assistant can be asked: the app (agents run there), the agents on, the assistant on. */
export const assistantOn = () => hasCore() && isDesktop() && agentsEnabled() && (agentsSettings().assistant?.enabled ?? true)

export function openAssistant(selected?: string | null, tab: 'conversations' | 'memory' = 'conversations') {
  assistantView.open = true
  assistantView.tab = tab
  if (selected !== undefined) assistantView.selected = selected
}

export const closeAssistant = () => (assistantView.open = false)
export const toggleAssistant = () => (assistantView.open ? closeAssistant() : openAssistant())

/** A conversation of the assistant (not one of the agents of the Agents view). */
export const isAssistant = (a: Pick<Agent, 'launch'>) => !!a.launch?.assistant

/** Its conversations, the latest first. */
export const conversations = (s: Snapshot | null | undefined): Agent[] =>
  (s?.agents ?? []).filter(isAssistant).sort((a, b) => (b.created_at ?? '').localeCompare(a.created_at ?? ''))

/** The proposals waiting for a click, of one conversation or all. */
export const proposalsOf = (s: Snapshot | null | undefined, agent?: string): AssistantProposal[] => (s?.assistant?.proposals ?? []).filter((p) => !agent || p.agent === agent)

/** A new request: its conversation opens in the panel. */
export async function askPrism(text: string): Promise<string | null> {
  const asked = text.trim()
  if (!asked) return null
  assistantView.asking = true
  try {
    const id = await invoke<string>('assistant_ask', { text: asked })
    openAssistant(id)
    return id
  } catch (e) {
    toast(String(e), 8000)
    return null
  } finally {
    assistantView.asking = false
  }
}

/** What became of a proposal, told to the assistant waiting for it. */
export const answerProposal = (id: number, status: string, detail: Record<string, unknown> = {}) => invoke<void>('assistant_answer', { id, status, detail })

/** Where a conversation stands, in a few words, for its line in the panel. */
export function conversationState(a: Agent, s: Snapshot | null | undefined): { label: string; tone: 'wait' | 'run' | 'done' | 'idle' } {
  if (proposalsOf(s, a.id).length) return { label: t('Proposal to validate'), tone: 'wait' }
  if (a.state === 'exited' || a.state === 'gone') return { label: t('Ended'), tone: 'idle' }
  if (a.status === 'needs_you' || a.status === 'waiting_tool') return { label: t('Waiting for you'), tone: 'wait' }
  if (a.status === 'running') return { label: t('Working…'), tone: 'run' }
  return { label: t('Answered'), tone: 'done' }
}

/** Its conversations grouped by day: Today, Yesterday, then dates. */
export function byDay(list: Agent[], now = new Date()): { day: string; list: Agent[] }[] {
  const key = (d: Date) => `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`
  const today = key(now)
  const yesterday = key(new Date(now.getTime() - 86_400_000))
  const out: { day: string; list: Agent[] }[] = []
  for (const a of list) {
    const d = new Date(a.created_at)
    const k = key(d)
    const day = k === today ? t('Today') : k === yesterday ? t('Yesterday') : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
    const last = out.at(-1)
    if (last?.day === day) last.list.push(a)
    else out.push({ day, list: [a] })
  }
  return out
}

/** Its memory, as the Memory tab shows it. */
export const memory = $state({ dir: '', facts: [] as Fact[], loaded: false, error: '' })

export async function loadMemory() {
  try {
    const m = await invoke<AssistantMemory>('assistant_memory')
    Object.assign(memory, { dir: m.dir, facts: m.facts, loaded: true, error: '' })
  } catch (e) {
    Object.assign(memory, { loaded: true, error: String(e) })
  }
}

/** A fact written: a new one without `id`. */
export async function saveFact(fact: { id?: string; kind: FactKind; text: string; source?: string }) {
  const saved = await invoke<Fact>('assistant_memory_save', { fact: { id: fact.id ?? '', kind: fact.kind, text: fact.text, source: fact.source ?? '' } })
  await loadMemory()
  return saved
}

export async function forgetFact(id: string) {
  await invoke<void>('assistant_memory_forget', { id })
  await loadMemory()
}

export async function forgetEverything() {
  const n = await invoke<number>('assistant_memory_clear')
  await loadMemory()
  return n
}

export const FACT_KINDS: FactKind[] = ['project', 'convention', 'decision', 'you']
export const factKindLabel = (k: FactKind) => ({ project: t('Project'), convention: t('Convention'), decision: t('Decision'), you: t('You') })[k]

/** The facts of `kind` (all: none). */
export const factsOf = (kind: FactKind | null) => (kind ? memory.facts.filter((f) => f.kind === kind) : memory.facts)

/** Live: the snapshot's conversations. */
export const liveConversations = () => conversations(live.snapshot)

/**
 * The repositories the assistant proposed, opened in Settings › Repositories' Add panel: told
 * `added` once the config has them (saved), by their proposal's id.
 */
const repoAdds = new Map<number, string>()
export const awaitRepo = (id: number, name: string) => repoAdds.set(id, name.toLowerCase())

export function watchRepoAdds(s: Snapshot | null | undefined) {
  if (!s || !repoAdds.size) return
  const known = new Set((s.repos ?? []).map((r) => r.name.toLowerCase()))
  const waiting = new Set(proposalsOf(s).map((p) => p.id))
  for (const [id, name] of repoAdds) {
    if (!waiting.has(id)) repoAdds.delete(id)
    else if (known.has(name)) {
      repoAdds.delete(id)
      void answerProposal(id, 'added', { repo: name }).catch((e) => toast(String(e)))
    }
  }
}
