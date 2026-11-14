import { live } from './live.svelte'
import { isDone, ticketIndex } from './rules'
import { conventions, mrTickets, namesTicket } from './conventions'
import { hasWorktrees, primaryRepo, repoName, repoOf, worktreeRepos } from './repositories'
import { mrKey } from './mr-ref'
import type { MergeRequest, Pipeline, Ticket } from './types'
import type { JobInfo } from './logs.svelte'
import type { EventDetails } from './agenda.svelte'
import { failedJobsContext, tail } from './fix-context'
import { oneLine, untrusted } from './untrusted'
import { afterLaunch, agentsEnabled, agentsSettings, launchNow, openLauncher, type Launch } from './agents.svelte'
import { isDesktop } from './bridge'
import { changeStatus, setMine } from './ticket-view.svelte'
import { openLink } from './links'
import type { MenuItem } from './components/ActionMenu.svelte'
import { mrNumber, mrWord } from './forge'
import { actorName, agentIcon } from './agent-cli'
import { t as tr } from './i18n.svelte'
import { used } from './feature-use.svelte'

/**
 * Contextual actions that hand a job to Claude, prompt ready, in the right place. In the
 * app, they launch a Prism agent (Agents view): the launcher opens prefilled, ⇧-click
 * goes straight away. Each job is asked in a few plain words, never by a skill's name: with a
 * plugin whose skill does that job, Claude picks it from its description. The place follows the work: a worktree for what writes code (the branch's one if it
 * exists, otherwise a new one, on the MR's branch where applicable), the main repository for what
 * reads.
 *
 * The same job stays openable in Claude Desktop (`href`): documented link
 * `claude://code/new?q=…&folder=…`, prompt not sent, folder to confirm: the same prompt.
 */
/** Stable identifier of an action: what looks it up, the label being display only. */
export type ClaudeActionId =
  | 'refine'
  | 'implement'
  | 'validate'
  | 'open-agent'
  | 'cve'
  | 'repo'
  | 'investigate'
  | 'flaky'
  | 'fix-pipeline'
  | 'fix-branch'
  | 'diagnose'
  | 'diagnose-crash'
  | 'dev-server'
  | 'retro'
  | 'meeting'
  | 'review'
  | 'review-fix'
  | 'rebase'
  | 'backport'
  | 'prompt'

export interface ClaudeAction {
  id: ClaudeActionId
  label: string
  /** The agent's prompt; a `/…` command passes as is. */
  prompt: string
  /** Launch by Prism, or `null` if the action is not possible (see `why`). */
  launch: Launch | null
  /** The same job in Claude Desktop, or `null` (see `why`). */
  href: string | null
  why?: string
  /** Effect in ClickUp once the agent is launched (or Claude Desktop opened), not before. */
  onLaunch?: () => Promise<void>
  /** What the action also does, stated in the menu. */
  note?: string
  /** Read by Prism at launch and added to the prompt (a failed job's log end): the agent starts from it. */
  context?: () => Promise<string>
}

/**
 * The prompts, in each language of `[agents] prompt_language`: the job in a few words. A team's
 * skill for it is picked by Claude from its description; without one, Claude does the job its way.
 */
interface Prompts {
  refine: (id: string) => string
  implement: (id: string) => string
  review: (iid: number) => string
  reviewFix: (iid: number) => string
  rebase: (base: string, iid: number) => string
  rebaseConflicts: (base: string, iid: number) => string
  cve: (pkg: string) => string
  validateMr: (iid: number) => string
  validateTicket: (id: string, merged: string, open: string) => string
  cveBase: (pkg: string, images: string, repo: string, ids: string) => string
  investigate: (workload: string, namespace: string, context: string, env: string, issue: string) => string
  flaky: (job: string, urls: string) => string
  backport: (iid: number, main: string) => string
  switchFirst: (branch: string, iid: number) => string
  fixPipeline: (iid: number) => string
  fixBranch: (branch: string, url: string) => string
  failedJobs: string
  diagnoseJob: (job: string, url: string) => string
  diagnoseCrash: (app: string, env: string, where: string, linked: string) => string
  jobLog: string
  devServer: (command: string, where: string, tail: string) => string
  retro: (figures: string) => string
  meeting: (when: string, details: string) => string
}
const PROMPTS: Record<'fr' | 'en', Prompts> = {
  fr: {
    refine: (id: string) => `Fais le refinement du ticket ${id}.`,
    implement: (id: string) => `Implémente le ticket ${id}.`,
    review: (iid: number) => `Fais la revue de code de la ${mrWord()} ${mrNumber(iid)}.`,
    reviewFix: (iid: number) => `Applique les retours de revue de la ${mrWord()} ${mrNumber(iid)}.`,
    rebase: (base: string, iid: number) => `Rebase la ${mrWord()} ${mrNumber(iid)} sur ${base}.`,
    rebaseConflicts: (base: string, iid: number) => `Rebase la ${mrWord()} ${mrNumber(iid)} sur ${base} et résous ses conflits, puis pousse la branche.`,
    cve: (pkg: string) => `Corrige la vulnérabilité du paquet ${pkg} signalée par Trivy.`,
    validateMr: (iid: number) => `Aide-moi à valider la ${mrWord()} ${mrNumber(iid)} en local.`,
    validateTicket: (id: string, merged: string, open: string) =>
      `Aide-moi à valider le ticket ${id} en local, à partir de ce qu'il demande.${merged ? ` Son code est mergé (${merged}).` : ''}${open ? ` ${mrWord()} encore ouverte : ${open}, à valider sur sa branche.` : ''}`,
    cveBase: (pkg: string, images: string, repo: string, ids: string) =>
      `Corrige les vulnérabilités ${ids} du paquet ${pkg} dans les images de base ${images} (dépôt ${repo}).`,
    investigate: (workload: string, namespace: string, context: string, env: string, issue: string) =>
      `Diagnostique le workload « ${workload} » (namespace ${namespace}, contexte ${context}, ${env}) : ${issue}. Lecture seule sur le cluster.`,
    flaky: (job: string, urls: string) => `Le job CI « ${job} » est instable (${urls}) : trouve la cause.`,
    backport: (iid: number, main: string) => `Backporte la ${mrWord()} ${mrNumber(iid)} sur ${main}.`,
    switchFirst: (branch: string, iid: number) => `Place-toi sur la branche ${branch} de la ${mrWord()} ${mrNumber(iid)}. Ensuite, `,
    fixPipeline: (iid: number) => `Corrige le pipeline en échec de la ${mrWord()} ${mrNumber(iid)}.`,
    fixBranch: (branch: string, url: string) => `Le pipeline de la branche ${branch} est en échec (${url}) : trouve la cause et corrige-la sur une nouvelle branche.`,
    failedJobs: 'Jobs en échec et fin de leurs logs (lus par Prism) :',
    diagnoseJob: (job: string, url: string) => `Diagnostique l'échec du job CI « ${job} » (${url}) : trouve la cause et propose un correctif, sans rien modifier.`,
    jobLog: 'Fin de son log (lue par Prism) :',
    diagnoseCrash: (app: string, env: string, where: string, linked: string) =>
      `Diagnostique le crash de « ${app} » sur ${env} (${where}) : lis ses logs (le conteneur d'avant son redémarrage aussi) et ses Events, et dis si le changement juste avant en est la cause${linked ? ` (${linked}, son diff à lire)` : ''}. Propose un correctif sans rien modifier, lecture seule sur le cluster. Ce qui a bougé sur l'environnement (frise de Prism) :`,
    meeting: (when: string, details: string) =>
      `Prépare-moi pour la réunion ci-dessous (${when}) : de quoi elle va parler, ce que je dois savoir sur les participants et leur lien avec mon travail en cours (tickets, MR, notes), les points à soulever et les questions à poser. Une page courte. Ne modifie aucun fichier.\n\n${details}`,
    retro: (figures: string) => `Prépare mes points pour la rétro d'équipe à partir de ces chiffres de mon sprint : 2 ou 3 choses à garder, 2 ou 3 à changer, concrètes, sans juger personne. Ne modifie aucun fichier.\n\n${figures}`,
    devServer: (command: string, where: string, output: string) => `Le serveur de dev \`${command}\` (${where}) s'est arrêté sur une erreur. Trouve la cause et corrige-la, puis relance-le.\n\n${output}`,
  },
  en: {
    refine: (id: string) => `Refine ticket ${id}.`,
    implement: (id: string) => `Implement ticket ${id}.`,
    review: (iid: number) => `Review the code of ${mrWord()} ${mrNumber(iid)}.`,
    reviewFix: (iid: number) => `Apply the review feedback of ${mrWord()} ${mrNumber(iid)}.`,
    rebase: (base: string, iid: number) => `Rebase ${mrWord()} ${mrNumber(iid)} onto ${base}.`,
    rebaseConflicts: (base: string, iid: number) => `Rebase ${mrWord()} ${mrNumber(iid)} onto ${base} and resolve its conflicts, then push the branch.`,
    cve: (pkg: string) => `Fix the vulnerability of package ${pkg} reported by Trivy.`,
    validateMr: (iid: number) => `Help me validate ${mrWord()} ${mrNumber(iid)} locally.`,
    validateTicket: (id: string, merged: string, open: string) =>
      `Help me validate ticket ${id} locally, against what it asks for.${merged ? ` Its code is merged (${merged}).` : ''}${open ? ` ${mrWord()} still open: ${open}, to validate on its branch.` : ''}`,
    cveBase: (pkg: string, images: string, repo: string, ids: string) =>
      `Fix vulnerabilities ${ids} of package ${pkg} in the base images ${images} (repository ${repo}).`,
    investigate: (workload: string, namespace: string, context: string, env: string, issue: string) =>
      `Diagnose the workload "${workload}" (namespace ${namespace}, context ${context}, ${env}): ${issue}. Read-only on the cluster.`,
    flaky: (job: string, urls: string) => `The CI job "${job}" is flaky (${urls}): find the cause.`,
    backport: (iid: number, main: string) => `Backport ${mrWord()} ${mrNumber(iid)} onto ${main}.`,
    switchFirst: (branch: string, iid: number) => `Switch to branch ${branch} of ${mrWord()} ${mrNumber(iid)}. Then, `,
    fixPipeline: (iid: number) => `Fix the failed pipeline of ${mrWord()} ${mrNumber(iid)}.`,
    fixBranch: (branch: string, url: string) => `The pipeline of branch ${branch} failed (${url}): find the cause and fix it on a new branch.`,
    failedJobs: 'Failed jobs and the end of their logs (read by Prism):',
    diagnoseJob: (job: string, url: string) => `Diagnose the failure of CI job "${job}" (${url}): find the cause and propose a fix, changing nothing.`,
    jobLog: 'End of its log (read by Prism):',
    diagnoseCrash: (app: string, env: string, where: string, linked: string) =>
      `Diagnose the crash of "${app}" on ${env} (${where}): read its logs (the container before its restart too) and its Events, and tell whether the change just before caused it${linked ? ` (${linked}, its diff to read)` : ''}. Propose a fix changing nothing, read-only on the cluster. What changed on the environment (Prism's timeline):`,
    meeting: (when: string, details: string) =>
      `Prepare me for the meeting below (${when}): what it will be about, what I should know about the attendees and how they relate to my current work (tickets, MRs, notes), the points to raise and the questions to ask. One short page. Change no file.\n\n${details}`,
    retro: (figures: string) => `Draft my points for the team retro from these figures of my sprint: 2 or 3 things to keep, 2 or 3 to change, concrete, judging no one. Change no file.\n\n${figures}`,
    devServer: (command: string, where: string, output: string) => `The dev server \`${command}\` (${where}) stopped on an error. Find the cause and fix it, then start it again.\n\n${output}`,
  },
}
const prompts = (): Prompts => PROMPTS[agentsSettings().prompt_language] ?? PROMPTS.en

/** Where the agent works: a folder as is, or a new worktree. */
type Place = { cwd: string | undefined } | { worktree: { name?: string; branch?: string; repo?: string } }

const link = (prompt: string, folder: string | undefined) =>
  `claude://code/new?q=${encodeURIComponent(prompt)}${folder ? `&folder=${encodeURIComponent(folder)}` : ''}`

const worktrees = () => (live.snapshot?.worktrees ?? []).filter((w) => !w.prunable)
/** Folder of a repository (`[[repos]]` in the config), by its name. */
const repoPath = (name: string | undefined) => (name ? live.snapshot?.repos?.find((r) => r.name === name)?.path : undefined)
/** The primary repository's name, where tickets and team-wide actions run. */
const primary = () => primaryRepo()?.name

/** In a repository's clone as is: the primary one by default. */
const inRepo = (repo = primary()): Place => ({ cwd: repoPath(repo) })
/** On a branch of a repository: its worktree if it exists, otherwise a new one that checks it out. */
const onBranch = (branch: string, repo = primary()): Place => {
  const wt = worktrees().find((w) => !w.is_main && w.branch === branch && w.repo === repo)
  return wt ? { cwd: wt.path } : { worktree: { branch, repo } }
}
const folderOf = (p: Place) => ('cwd' in p ? p.cwd : repoPath(p.worktree.repo ?? primary()))
/** Agents create worktrees in that repository. */
const takesWorktrees = (repo: string | undefined) => worktreeRepos().some((r) => r.name === (repo ?? primary()))

/**
 * `title` names the agent (and its new worktree); `desktop`: the prompt for Claude Desktop,
 * if it differs from the agent's.
 */
function action(id: ClaudeActionId, label: string, title: string, prompt: string, place: Place, opts: { desktop?: string; why?: string } = {}): ClaudeAction {
  const why = opts.why ?? ('cwd' in place ? (place.cwd ? undefined : tr('No repository folder')) : takesWorktrees(place.worktree.repo) ? undefined : tr('No repository where agents create worktrees ([[repos]] worktrees)'))
  if (why) return { id, label, prompt, launch: null, href: null, why }
  return { id, label, prompt, launch: { title, prompt, ...(id === 'prompt' ? {} : { from: label }), ...place }, href: link(opts.desktop ?? prompt, folderOf(place)) }
}

/** Read-only, whatever its prompt says: the agent starts in plan mode. */
const planOnly = (a: ClaudeAction): ClaudeAction => (a.launch ? { ...a, launch: { ...a.launch, permission_mode: 'plan' } } : a)

/**
 * Refine a ticket in the repo (read-only: feasibility, solution, estimate);
 * implement it in its worktree if it exists, otherwise in a new worktree named after it.
 */
export function ticketActions(t: Ticket): ClaudeAction[] {
  if (isDone(t)) return []
  const repo = t.repo ?? primary()
  return [action('refine', tr('Refine'), `${t.id} · Refine`, prompts().refine(t.id), inRepo(repo)), implementAction(t, repo), ...validateActions(t)]
}

/**
 * Implement ticket `t` in repository `repo` (its own by default; a project's, whatever repository
 * the ticket names, for a new workstream): its worktree there if it has one, otherwise a new one
 * named after it. `clone`: in the clone where agents make no worktrees, rather than not at all.
 */
export function implementAction(t: Ticket, repo = t.repo ?? primary(), clone = false): ClaudeAction {
  const wt = worktrees().find((w) => !w.is_main && w.ticket === t.id && w.repo === repo)
  const place: Place = wt ? { cwd: wt.path } : clone && !takesWorktrees(repo) ? { cwd: repoPath(repo) } : { worktree: { name: `${t.id} ${t.name}`, repo } }
  return { ...action('implement', tr('Implement'), `${t.id} · Implement`, prompts().implement(t.id), place), ...takeImplementation(t) }
}

/**
 * Implementing a ticket takes it: once the agent is launched, a ticket still before dev moves to
 * the first in-progress status (`in_progress_statuses`), and it is assigned to me if it is not yet.
 */
function takeImplementation(t: Ticket): { onLaunch?: () => Promise<void>; note?: string } {
  const before = conventions().before_dev_statuses.map((x) => x.toLowerCase()).includes(t.status.toLowerCase())
  const target = before ? conventions().in_progress_statuses[0] : undefined
  const me = live.snapshot?.tracker_me
  const assign = !!me && !t.assignees.some((a) => a.id === me.id)
  if (!target && !assign) return {}
  return { onLaunch: takeTicket(t, target), note: [target && `→ ${target}`, assign && tr('assign me')].filter(Boolean).join(', ') }
}

/**
 * What implementing ticket `id` takes on launch (Prism's assistant proposing it), when Prism shows
 * the ticket: its status and assignee, as the ticket's own Implement does.
 */
export function takeImplementationOf(id: string): (() => Promise<void>) | undefined {
  const s = live.snapshot
  if (!s) return undefined
  const found = [...ticketIndex(s).values()].find((x) => x.t.id.toLowerCase() === id.toLowerCase())
  return found ? takeImplementation(found.t).onLaunch : undefined
}

/**
 * An agent on the ticket, with no defined job: the launcher opens under the ticket's name, empty prompt,
 * in its worktree if it has one, otherwise the repository (the launcher offers a new worktree).
 */
export function openAgentAction(t: Ticket): ClaudeAction {
  const repo = t.repo ?? primary()
  const wt = worktrees().find((w) => !w.is_main && w.ticket === t.id && w.repo === repo)
  return { ...action('open-agent', tr('Open agent'), `${t.id} ${t.name}`, '', wt ? { cwd: wt.path } : inRepo(repo)), note: tr('empty prompt') }
}

/**
 * Ticket in a validation status (`[clickup] validation_statuses`): validate it locally, the ticket
 * and not its MRs, which are in principle merged (cited with their target branch). An MR still
 * open is named for the agent to validate on its branch; when it is the only one left, the agent
 * works on that branch, otherwise in a new worktree named after the ticket. From the first status
 * (to validate), I take the validation: once the agent is launched, the ticket moves to the last
 * one and is assigned to me.
 */
function validateActions(t: Ticket): ClaudeAction[] {
  const statuses = conventions().validation_statuses.map((x) => x.toLowerCase())
  const status = t.status.toLowerCase()
  if (!statuses.includes(status)) return []
  const target = statuses[statuses.length - 1]
  const take = statuses.length > 1 && status === statuses[0] ? takeTicket(t, target) : undefined
  const mrs = (live.snapshot?.merge_requests ?? []).filter((m) => namesTicket(m, t.id) && (m.repo || primary()) === (t.repo ?? primary())).sort((a, b) => b.updated_at.localeCompare(a.updated_at))
  const open = mrs.filter((m) => m.state === 'opened')
  const merged = mrs.filter((m) => m.state === 'merged')
  // "!12, !14 → develop; !15 → main": each target branch with the MRs merged into it.
  const targets = [...new Set(merged.map((m) => m.target_branch))]
  const mergedText = targets.map((b) => `${merged.filter((m) => m.target_branch === b).map((m) => mrNumber(m.iid)).join(', ')} → ${b}`).join('; ')
  const prompt = prompts().validateTicket(t.id, mergedText, open.map((m) => mrNumber(m.iid)).join(', '))
  const place: Place = open.length === 1 ? onBranch(open[0].source_branch, t.repo ?? undefined) : { worktree: { name: `${t.id} validate`, ...(t.repo ? { repo: t.repo } : {}) } }
  return [{ ...action('validate', tr('Validate locally'), `${t.id} · Validate`, prompt, place), onLaunch: take, note: take ? tr('→ {status}, assign me', { status: target }) : undefined }]
}

/**
 * An MR's validation takes its ticket's, as from the ticket: when that ticket waits in the first
 * validation status, launching moves it to the last one and assigns it to me.
 */
function mrValidation(m: MergeRequest): { onLaunch?: () => Promise<void>; note?: string } {
  const statuses = conventions().validation_statuses.map((x) => x.toLowerCase())
  if (!mrTickets(m).length || statuses.length < 2) return {}
  const s = live.snapshot
  const tickets = [...(s?.sprint?.tickets ?? []), ...(s?.next_sprint?.tickets ?? []), ...(s?.outside_sprint?.tickets ?? []), ...(s?.pickup?.boards.flatMap((b) => b.tickets) ?? [])]
  // Of the tickets it names, the first one waiting in the first validation status.
  const t = tickets.find((x) => namesTicket(m, x.id) && (!x.repo || x.repo === (m.repo || primary())) && x.status.toLowerCase() === statuses[0])
  if (!t) return {}
  const target = statuses[statuses.length - 1]
  return { onLaunch: takeTicket(t, target), note: tr('{ticket} → {status}, assign me', { ticket: t.id, status: target }) }
}

/** Move the ticket to that status (if any) and assign it to me (if I am not already). */
function takeTicket(t: Ticket, status: string | undefined): () => Promise<void> {
  return async () => {
    if (status) await changeStatus(t.task_id, status, [])
    const me = live.snapshot?.tracker_me
    if (me && !t.assignees.some((a) => a.id === me.id)) await setMine(t.task_id, true)
  }
}

/**
 * Dependency vulnerability: Claude (or a team's skill doing that job) looks for it in every project and updates
 * it, in a new worktree. In the base images of another repository (`repo`, `images`:
 * their folders), the fix is made in their Dockerfile, from that repository.
 */
export function cveAction(pkg: string, base?: { repo: string; images: string[]; ids: string[] }): ClaudeAction {
  if (!base) return action('cve', tr('Fix CVE'), `CVE · ${pkg}`, prompts().cve(pkg), { worktree: { name: `cve ${pkg}` } })
  const folder = repoPath(base.repo)
  const images = base.images.join(', ')
  return action(
    'cve',
    tr('Fix CVE'),
    `CVE · ${pkg} (${base.repo})`,
    prompts().cveBase(pkg, images, base.repo, base.ids.join(', ')),
    { cwd: folder },
    { why: folder ? undefined : tr('Repository {repo} not found in [[repos]]', { repo: base.repo }) },
  )
}

/** New Claude session in a repository, without a prompt (Repos menu). */
export const repoAction = (name: string, path: string): ClaudeAction => action('repo', actorName(), name, '', { cwd: path })

/**
 * Struggling workload: Claude diagnoses with kubectl, read-only, and proposes a fix — in plan
 * mode: the agent has the user's kube contexts, and "read-only" is only words of its prompt.
 */
export const investigateAction = (workload: string, namespace: string, context: string, env: string, issue: string): ClaudeAction =>
  planOnly(action('investigate', tr('Investigate'), `Investigate ${workload} (${env})`, prompts().investigate(workload, namespace, context, env, issue), inRepo()))

/**
 * A crash on the Changes timeline: the agent reads the pod's logs and events and the diff of the
 * change linked to it, in plan mode; the timeline of its application joins the prompt at launch
 * (`timeline`, built then).
 */
export const diagnoseCrashAction = (app: string, env: string, where: string, linked: string, timeline: () => string): ClaudeAction => ({
  ...planOnly(action('diagnose-crash', tr('Diagnose'), `Crash ${app} (${env})`, prompts().diagnoseCrash(app, env, where, linked), inRepo())),
  context: async () => untrusted('Changes timeline', timeline()),
})

/** Flaky job: Claude reads the logs of its last failures (glab or gh) and proposes a fix, in the repo. */
export const flakyAction = (job: string, urls: string[]): ClaudeAction =>
  action(
    'flaky',
    tr('Analyse'),
    `Flaky ${job}`,
    prompts().flaky(oneLine(job), urls.join(', ')),
    inRepo(),
  )

/** The failed jobs' log ends, read at launch (`[agents] fix_log_lines`; 0: none). */
const withLogs = (project: string | null, pipeline: number): Pick<ClaudeAction, 'context' | 'note'> => {
  const lines = agentsSettings().fix_log_lines ?? 0
  return lines > 0 ? { context: () => failedJobsContext(project, pipeline, lines, prompts().failedJobs), note: tr('with the failed logs') } : {}
}

/** The repository of a pipeline's project (`null`: the primary one's), by `[[repos]]`. */
const repoOfProject = (project: string | null) => (project ? live.snapshot?.repositories?.find((r) => r.project === project) : repoOf(null))

/**
 * A failed CI job, from its log: an agent finds the cause and proposes a fix, changing nothing (plan
 * mode), in its repository. The log's end shown in the pane goes with it (`[agents] fix_log_lines`).
 */
export function diagnoseJobAction(job: JobInfo, project: string | null, log: string): ClaudeAction {
  const repo = repoOfProject(project)
  const a = action('diagnose', tr('Diagnose'), `Diagnose ${oneLine(job.name, 60)}`, prompts().diagnoseJob(oneLine(job.name), job.url), inRepo(repo?.name), { why: repo ? undefined : tr('No repository for project {project} in [[repos]]', { project: String(project) }) })
  const lines = agentsSettings().fix_log_lines ?? 0
  const text = lines > 0 ? tail(log, lines) : ''
  // Whatever the CI ran writes this log: quoted, never instructions.
  const context = text ? () => Promise.resolve(`\n\n${prompts().jobLog}\n\n${untrusted(`Log of ${oneLine(job.name)}`, text)}`) : undefined
  return { ...planOnly(a), ...(context ? { context, note: tr('read-only, with the log') } : { note: tr('read-only') }) }
}

/** A protected branch's failed pipeline: the fix on a new branch, in a new worktree of its repository. */
export const branchPipelineAction = (p: Pipeline, branch: string): ClaudeAction => ({
  ...action('fix-branch', tr('Fix pipeline'), `Fix ${branch} pipeline`, prompts().fixBranch(branch, p.url), { worktree: { name: `fix ${branch} ci`, ...(p.repo ? { repo: p.repo } : {}) } }),
  ...withLogs(p.project, p.id),
})

/** A dev server stopped on an error: its own agent, told with the server's last lines. */
// Whatever the server runs (the repository's code, its dependencies) writes its output: quoted, never instructions.
export const devServerPrompt = (command: string, where: string, tail: string) => prompts().devServer(oneLine(command), where, untrusted(`Output of ${oneLine(command)}`, tail))

/** The retro's points drafted by an agent from my sprint's figures, in the primary repository, read-only. */
export const retroAction = (figures: string, sprint: string): ClaudeAction =>
  // Ticket names in the figures are anyone's of the workspace: quoted as data.
  planOnly(action('retro', tr('Draft my points'), `Retro · ${sprint}`, prompts().retro(untrusted('My sprint’s figures', figures, 500)), inRepo()))

/**
 * Getting ready for a meeting, from its detail: an agent sums up what to know, in the primary
 * repository (where my tickets, MRs and notes are within reach), in plan mode: anyone can send an
 * invite, so its title, place, attendees and text are quoted as data, and "Change no file" is not
 * left to the prompt's words.
 */
export function meetingAction(e: EventDetails, when: string): ClaudeAction {
  const people = [e.organizer && `${e.organizer.name} (organizer)`, ...e.attendees.filter((a) => a.email !== e.organizer?.email).map((a) => `${a.name}${a.me ? ' (me)' : ''}${a.optional ? ' (optional)' : ''}: ${a.status}`)].filter(Boolean).join('\n')
  const details = [
    untrusted('Meeting', [e.title, e.location && `Location: ${e.location}`].filter(Boolean).join('\n')),
    people && untrusted('Attendees', people),
    e.notes && untrusted('Invitation', e.notes, 500),
  ].filter(Boolean).join('\n\n')
  return planOnly(action('meeting', tr('Prepare'), `Meeting · ${oneLine(e.title, 60)}`, prompts().meeting(when, details), inRepo()))
}

/** Refinement from the queue: a team's refinement skill adapts its deliverable to the status (specify, challenge, estimate). */
export const refineAction = (t: Ticket, label: string): ClaudeAction =>
  action('refine', label, `${t.id} · ${label}`, prompts().refine(t.id), inRepo())

/**
 * Review in the repo (a review skill may take its own temporary worktree); review-fix, rebase and
 * pipeline fix on the MR's branch, in its worktree (existing, or a new one that
 * checks it out). Backport to main: plain prompt, in the repo.
 */
export function mrActions(m: MergeRequest): ClaudeAction[] {
  // In a repository where agents work (`[[repos]] worktrees`): nothing for the others' MRs.
  if (!hasWorktrees(m.repo)) return []
  const repo = repoName(m.repo)
  const key = mrKey(m)
  // Backport of an MR not already targeting main, open or merged, from the repo (the primary one's branches).
  const main = conventions().main_branch
  const backport = m.repo || m.target_branch === main ? [] : [action('backport', tr('Backport to {branch}', { branch: main }), `${key} · Backport`, prompts().backport(m.iid, main), inRepo(repo))]
  if (m.state !== 'opened') return backport
  const branch = onBranch(m.source_branch, repo)
  // For Claude Desktop, without a local worktree, the session starts from the repo: it must first switch to the branch there.
  const switchFirst =
    'worktree' in branch ? prompts().switchFirst(m.source_branch, m.iid) : ''
  const lower = (s: string) => (switchFirst ? s[0].toLowerCase() + s.slice(1) : s)
  const fix = prompts().fixPipeline(m.iid)
  const reviewFix = prompts().reviewFix(m.iid)
  // In conflict, or the forge's rebase failed on a commit: the agent resolves the conflicts too.
  const conflicting = m.conflicts || (!!m.merge_error && /rebase/i.test(m.merge_error))
  const rebase = (conflicting ? prompts().rebaseConflicts : prompts().rebase)(m.target_branch, m.iid)
  const validate = prompts().validateMr(m.iid)
  const head = m.head_pipeline
  const fixPipeline = m.pipeline === 'failed' ? [{ ...action('fix-pipeline', tr('Fix pipeline'), `${key} · Fix pipeline`, fix, branch, { desktop: switchFirst + lower(fix) }), ...(head ? withLogs(m.project ?? null, head.id) : {}) }] : []
  return [
    ...fixPipeline,
    action('review', tr('Review'), `${key} · Review`, prompts().review(m.iid), inRepo(repo)),
    action('review-fix', tr('Review-fix'), `${key} · Review-fix`, reviewFix, branch, { desktop: switchFirst + lower(reviewFix) }),
    action('rebase', conflicting ? tr('Rebase & fix conflicts') : tr('Rebase onto {branch}', { branch: m.target_branch }), `${key} · Rebase`, rebase, branch, { desktop: switchFirst + lower(rebase) }),
    { ...action('validate', tr('Validate locally'), `${key} · Validate`, validate, branch, { desktop: switchFirst + lower(validate) }), ...mrValidation(m) },
    ...backport,
  ]
}

/** Console: an agent on a prompt as typed (natural language, or a `/plugin:skill` command), in the primary repository. */
export const promptAction = (prompt: string): ClaudeAction => action('prompt', actorName(), prompt.slice(0, 60), prompt, inRepo())

/** Actions launch an agent in the app (`[agents] target`); otherwise, and in the browser preview, Claude Desktop. */
export const agentsTarget = () => agentsEnabled() && agentsSettings().target === 'agent'

/**
 * Launches the action: the prefilled launcher, or the agent directly (`direct`, ⇧-click in the
 * window). Text from elsewhere in its prompt (`context`: CI logs) is seen before it is sent: the
 * launcher, ⇧ or not.
 */
export async function runClaude(a: ClaudeAction, direct = false) {
  if (!a.launch) return
  used(`action:${a.id}`)
  if (direct && a.context && isDesktop()) direct = false
  const extra = a.context && !direct ? await a.context().catch(() => '') : ''
  const launch = extra ? { ...a.launch, prompt: `${a.launch.prompt ?? ''}${extra}` } : a.launch
  // Its ←: what the action is on (`SHOP-433` of `SHOP-433 · Implement`).
  if (!direct) return openLauncher(launch, a.href, a.onLaunch, launch.title.split(' · ')[0])
  if (!(await launchNow(launch))) return
  await afterLaunch(a.onLaunch, tr('Agent started'))
}

/** Claude Desktop: the action's effect fires with the opening of the link. */
const openDesktop = (a: ClaudeAction) => void afterLaunch(a.onLaunch, tr('Claude Desktop opened'))

/** `onclick` of an action link: a Prism agent rather than Claude Desktop. */
export const claudeClick = (a: ClaudeAction) => (e: MouseEvent) => {
  if (!agentsTarget() || !a.launch) return openDesktop(a)
  e.preventDefault()
  // ⇧-click launches at once in the window; a phone's tap opens its launcher, as a click does.
  void runClaude(a, e.shiftKey && isDesktop())
}

/** ⋯ menu line of an action. */
export const claudeItem = (a: ClaudeAction): MenuItem =>
  agentsTarget() && a.launch
    ? { key: `claude-${a.id}`, icon: agentIcon(), label: a.label, hint: a.note ?? ('worktree' in a.launch ? tr('worktree') : undefined), run: () => runClaude(a) }
    : a.onLaunch && a.href
      ? { key: `claude-${a.id}`, icon: 'claude', label: a.label, hint: a.note, run: () => { openDesktop(a); openLink(a.href!) } }
      : { key: `claude-${a.id}`, icon: 'claude', label: a.label, href: a.href ?? undefined }
