import { invoke } from './bridge'
import { live } from './live.svelte'
import { agentsSettings } from './agents.svelte'
import { providerOf } from './agent-cli'
import { agentRepo, hasWorktrees } from './repositories'
import { templated, uniqueName } from './quick-action-source'
import { openSettings } from './settings.svelte'
import { routineFor } from './routines.svelte'
import { toast } from './toast.svelte'
import { t } from './i18n.svelte'
import type { QuickAction } from './bindings/QuickAction'
import type { SessionSource } from './bindings/SessionSource'
import type { DraftState } from './bindings/DraftState'

/**
 * Create a quick action… from a past session (an agent of the list, archived too; the last one
 * closed in a repository; a Claude Desktop session): the form of a quick action, prefilled from
 * it (`session_source`). Its prompt is first the session's first one, its repository and base
 * branch as `{repo}` and `{base}`; meanwhile a hidden agent of the session's CLI writes one that
 * stands on its own from all the prompts given in it (`quick_action_draft`, `prism_agents::draft`):
 * Prism reads it back from its reply and closes it, the form reads the draft's state
 * (`draft_state`). Keep this one stops it (`draft_stop`). Saved, it is added to `config.toml`
 * (`quick_action_add`), never to a pack; its ⚡ shows at once. Make it a routine too: Settings ›
 * Routines opens on a new one running it.
 */

/** Which session: an agent's id, a repository's (the last one closed there), a Desktop session's id. */
export type SessionRef = { agent: string } | { left: string } | { desktop: string }

type Mode = '' | 'default' | 'acceptEdits' | 'plan'

export const qaDraft = $state({
  open: false,
  loading: false,
  error: '',
  source: null as SessionSource | null,
  /** The repository it ran in; empty: none of the repositories. */
  repo: '',
  /** What it was made from: the session's title, its CLI, when. */
  name: '',
  prompt: '',
  /** The session's first prompt, as the form shows it before the draft. */
  first: '',
  /** On every repository rather than the session's. */
  everywhere: false,
  worktree: false,
  mode: '' as Mode,
  kind: '',
  model: '',
  effort: '',
  /** The model and effort were the session's other CLI's: left out (a quick action runs its repository's). */
  otherCli: false,
  /** The drafting agent's id while it writes; `drafted`: its prompt is in the form. */
  drafter: '',
  drafted: false,
  /** Its conversation cannot be read: no draft. */
  noDraft: false,
  routine: false,
  saving: false,
})

const MODES: Mode[] = ['default', 'acceptEdits', 'plan']
/** Each opening's number: a session read late for a form opened since (or closed) is dropped. */
let opened = 0
const repoInfo = (name: string) => live.snapshot?.repos?.find((r) => r.name === name)

/** The repository a session ran in. */
function repoOfRef(ref: SessionRef, dir: string): string {
  if ('left' in ref) return ref.left
  if ('agent' in ref) {
    const a = live.snapshot?.agents.find((x) => x.id === ref.agent)
    if (a) return agentRepo(a, live.snapshot) ?? ''
  }
  return agentRepo({ cwd: dir, worktree: dir }, live.snapshot) ?? ''
}

/** The form, prefilled from that session; its draft asked of an agent when its prompts can be read. */
export async function openQuickActionDraft(ref: SessionRef) {
  const n = ++opened
  await stopDrafting()
  Object.assign(qaDraft, { open: true, loading: true, error: '', source: null, prompt: '', first: '', drafted: false, noDraft: false, routine: false, otherCli: false })
  try {
    const source = await invoke<SessionSource>('session_source', { source: ref })
    if (n !== opened || !qaDraft.open) return
    const repo = repoOfRef(ref, source.dir)
    const base = repoInfo(repo)?.base || 'main'
    const launch = source.launch
    const first = templated(source.prompts[0] ?? launch?.prompt ?? '', repo, base)
    // A quick action runs its repository's CLI: another CLI's model and effort would not hold.
    const sameCli = source.provider === providerOf(repo || undefined)
    const mode = (launch?.permission_mode ?? '') as Mode
    const taken = (agentsSettings().quick_actions ?? []).map((a) => a.name)
    Object.assign(qaDraft, {
      loading: false,
      source,
      repo,
      name: uniqueName(source.title, taken),
      prompt: first,
      first,
      everywhere: false,
      worktree: source.worktree && (!repo || hasWorktrees(repo)),
      mode: MODES.includes(mode) ? mode : '',
      kind: launch?.category ?? live.snapshot?.agents.find((a) => 'agent' in ref && a.id === ref.agent)?.category ?? '',
      model: sameCli ? (launch?.model ?? '') : '',
      effort: sameCli ? (launch?.effort ?? '') : '',
      otherCli: !sameCli && !!(launch?.model || launch?.effort),
      noDraft: !source.readable || !source.prompts.length,
    })
    if (!qaDraft.noDraft) void draftWithAgent(ref, source, repo)
  } catch (e) {
    if (n === opened) Object.assign(qaDraft, { loading: false, error: String(e) })
  }
}

/** The agent writing the prompt, launched by Prism in the repository's clone (else the session's folder). */
async function draftWithAgent(ref: SessionRef, source: SessionSource, repo: string) {
  try {
    const id = await invoke<string>('quick_action_draft', { source: ref, repo, name: qaDraft.name })
    if (!qaDraft.open || qaDraft.source !== source) return void invoke('draft_stop', { id }).catch(() => null)
    qaDraft.drafter = id
    void waitForDraft(id)
  } catch (e) {
    qaDraft.noDraft = true
    qaDraft.error = t('No draft: {error}', { error: String(e) })
  }
}

/**
 * Its state read every two seconds, until Prism has the prompt (and closed its agent). Given up
 * (the first prompt kept) when Prism did: it stopped, asked for a permission, or took too long;
 * lost when Prism no longer knows it.
 */
async function waitForDraft(id: string) {
  while (qaDraft.drafter === id) {
    await new Promise((r) => setTimeout(r, 2000))
    if (qaDraft.drafter !== id) return
    const s = await invoke<DraftState | null>('draft_state', { id }).catch(() => undefined)
    if (qaDraft.drafter !== id) return
    if (s === undefined || s?.state === 'waiting') continue
    qaDraft.drafter = ''
    if (s === null) {
      // Unknown to Prism (restarted, or the draft long over): said, not waited for forever.
      qaDraft.noDraft = true
      qaDraft.error = t('The draft was lost: {error}', { error: t('Prism no longer knows it (restarted, or it ended a while ago)') })
    } else if (s.state === 'failed') {
      qaDraft.noDraft = true
      qaDraft.error = t('No draft: {error}', { error: s.why })
    } else if (s?.state === 'ready') {
      // Taken unless the prompt was changed by hand meanwhile.
      if (qaDraft.prompt === qaDraft.first) qaDraft.prompt = s.text
      qaDraft.drafted = true
    }
  }
}

/** Keep this one (or the form closed): its drafting agent closed, its draft not waited for. */
export async function stopDrafting() {
  const id = qaDraft.drafter
  qaDraft.drafter = ''
  if (id) await invoke('draft_stop', { id }).catch(() => null)
}

/** Back to the session's first prompt. */
export const backToFirst = () => (qaDraft.prompt = qaDraft.first)

export async function closeQuickActionDraft() {
  qaDraft.open = false
  await stopDrafting()
}

/** Added to config.toml; with Make it a routine, Settings › Routines on a new one running it. */
export async function saveQuickActionDraft() {
  if (qaDraft.saving) return
  const d = qaDraft
  const action: QuickAction = {
    name: d.name.trim(),
    prompt: d.prompt.trim(),
    repos: d.everywhere || !d.repo ? [] : [d.repo],
    worktree: d.worktree,
    mode: d.mode || null,
    kind: d.kind || null,
    model: d.model.trim() || null,
    effort: d.effort.trim() || null,
  }
  qaDraft.saving = true
  qaDraft.error = ''
  try {
    await invoke('quick_action_add', { action })
    await closeQuickActionDraft()
    if (d.routine) {
      Object.assign(routineFor, { action: action.name, repos: action.repos.length ? action.repos : d.repo ? [d.repo] : [] })
      openSettings('routines')
    }
    toast(t('Quick action {name} added to config.toml', { name: action.name }), 6000)
  } catch (e) {
    qaDraft.error = String(e)
  } finally {
    qaDraft.saving = false
  }
}
