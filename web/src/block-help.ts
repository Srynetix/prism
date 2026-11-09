import { trackerProvider } from './tracker'
import { mrWord, onGithub } from './forge'
import { actorName, agentCli, agentName } from './agent-cli'
import { agentsSettings } from './agents.svelte'
import type { BlockId } from './views'
import { BLOCK_DEFS, type PluginBlockId } from './blocks'
import { t } from './i18n.svelte'

/**
 * Contextual help for each block: the "?" in its title and the "+ Block" menu show it.
 * `purpose` says what the block is for, `points` what you see there and what you can do,
 * `source` where its data comes from. Written in the window's language (`t`).
 */
export interface BlockHelp {
  purpose: string
  points: string[]
  source: string
}

/** `merge requests` on GitLab, `pull requests` on GitHub. */
const changes = () => (onGithub() ? 'pull requests' : 'merge requests')

/** Where the tracker's blocks read their tickets, by provider. */
const fromTracker = (texts: { clickup: string; jira: string; obsidian: string }) => texts[trackerProvider()]

export const BLOCK_HELP: Record<BlockId, BlockHelp> = {
  ...(Object.fromEntries(BLOCK_DEFS.map((b) => [b.id, b.help])) as Record<PluginBlockId, BlockHelp>),
  todo: {
    purpose: t('What needs you now, most severe first: one line per problem worth acting on.'),
    get points() {
      return [
        onGithub() ? t('Failing pods, broken runners and stuck jobs, failed workflow runs on main or release/*.') : t('Failing pods, broken runners and stuck jobs, failed pipelines on main or release/*.'),
        t('{mr}s and tickets out of sync: {mr} merged but ticket not done, ticket in review without {mr}…', { mr: mrWord() }),
        t('Reviews waiting for you, your {mr}s failing, in conflict, out of draft without a reviewer or with unresolved threads, {agent} sessions waiting for an answer.', { mr: mrWord(), agent: agentName() }),
        t('x marks an alert as seen; it comes back if the situation changes.'),
      ]
    },
    source: t('Computed by Prism’s core from what the other blocks read, the window open or not.'),
  },
  sessions: {
    get purpose() {
      return t('The {cli} agents launched by Prism, and whether one of them is waiting for you.', { cli: agentCli() })
    },
    get points() {
      return [
        t('Waiting for you first (asks, tool), then running, then stopped agents.'),
        t('Badges for the ticket, the {mr}, its CI and how far the branch is behind its base.', { mr: mrWord() }),
        t('A click opens the agent in the Agents view; Claude Desktop sessions are listed there too.'),
      ]
    },
    get source() {
      return t('Prism agents (tmux) and {hooks}; never the conversation text.', { hooks: { claude: t('the Claude Code hooks'), 'command-code': t('Prism’s Command Code mod'), opencode: t('Prism’s OpenCode plugin'), codex: t('the Codex hooks') }[agentsSettings().provider] ?? t('the Claude Code hooks') })
    },
  },
  worktrees: {
    purpose: t('Every worktree of the primary repository: what it is for, who works in it, and which ones to clean up.'),
    get points() {
      return [
        t('Linked ticket and {mr}, the Prism agents and Claude Desktop sessions in it (archived ones too), behind/ahead, changes, unpushed commits, size.', { mr: mrWord() }),
        t('In use, Stale ({mr} merged, branch gone, ticket done, session archived, or idle for [worktrees] stale_days), Folder gone, Idle.', { mr: mrWord() }),
        t('Remove only when nothing would be lost; Remove anyway on request; Clean up for the stale ones; Prune for folders already gone.'),
        t('Never under a running agent or a live Desktop session. Two clicks to confirm.'),
        t('Archive hides one (or the ticked ones), a repository’s main folder too, from the list, its counts and the stale alert, nothing removed; it comes back by itself on a new commit or a change. One whose agents and sessions are all archived is hidden with them, and back with them. The eye icon shows the archived ones.'),
      ]
    },
    get source() {
      return t('git worktree list, status and rev-list in the main repository; {mr}s, tickets and sessions from the other sources.', { mr: mrWord() })
    },
  },
  notes: {
    purpose: t('Free notes, kept on this machine.'),
    points: [
      t('Enter adds a note, Shift+Enter a new line; click to edit, the delete button to remove it.'),
      t('Sticky, of the selected day, or a reminder notified by the system.'),
      t('A note can be a task: its badge goes TODO → DOING → DONE; type TODO or DOING first to set it. Done ones are folded.'),
      t('Search appears beyond 5 notes.'),
    ],
    source: t('Saved as JSON in the app data folder.'),
  },
  mrs: {
    get purpose() {
      return t('The {changes} of the team, and what each one needs next.', { changes: changes() })
    },
    get points() {
      const mr = mrWord()
      return [
        onGithub() ? t('Team, Mine and To review tabs; the checks of each {mr} at a glance.', { mr }) : t('Team, Mine and To review tabs; the pipeline of each {mr} at a glance, stage by stage.', { mr }),
        t('The next step in plain words (CI failed, Review it, Rebase needed, Ready to merge…) and the button that unblocks it.'),
        t('The size of each open {mr} (+added −removed lines); over [alerts] large_mr_lines, it stands out in orange.', { mr }),
        t('A click opens the {mr} drawer: overview, diff review with comments, discussion, approve.', { mr }),
        t('In the diff, a Markdown file reads rendered next to its changes (Diff / Both / Preview), the changed blocks marked.'),
        onGithub() ? t('The checks of main and release/* in the banner (click: logs).') : t('The pipelines of main and release/* in the banner (click: logs).'),
      ]
    },
    get source() {
      return onGithub() ? t('GitHub, through gh, every 15 s (slower when its API quota runs low).') : t('GitLab, through glab, every 15 s.')
    },
  },
  queue: {
    get purpose() {
      return t('The {changes} of one repository waiting to be merged, and whether anything will merge them.', { changes: changes() })
    },
    get points() {
      const mr = mrWord()
      return [
        t('With a merge queue (Settings › Code › Repositories, Queue): the {changes} with its label, oldest first, the order its CI job takes them; ▶ is the next one.', { changes: changes() }),
        onGithub() ? t('Then those with auto-merge on, merged by the forge once their checks pass.') : t('Then those with auto-merge on, merged by the forge once their pipeline passes.'),
        t('Then the approved ones with neither: nothing will merge them (Merge queue or Merge on the row).'),
        t('A {mr} held up in the queue (conflicts, failed rebase, failed CI) is counted as stuck in the header.', { mr }),
        t('With priority labels (Settings › Code › Forge › GitLab › Merge priority prefix, merge-priority::0): the lowest number first, then the {changes} without one, each shown as P0, P1…', { changes: changes() }),
        t('Split by target branch when they go to several: each branch its own order and its own ▶, the main branch first.'),
        t('One repository at a time, chosen above the list (the same on the phone); a click opens the {mr} drawer.', { mr }),
      ]
    },
    get source() {
      return t('The {changes} the Merge requests block reads: no request of its own.', { changes: changes() })
    },
  },
  wip: {
    purpose: t('Your tickets in progress, each with where it stands from refinement to done and the one action that moves it on.'),
    get points() {
      return [
        t('One row per active ticket of yours: the sprint’s assigned to you and not done, those with an open {mr} or a running agent of yours, and the {mr}s asking for your review.', { mr: mrWord() }),
        t('Its seven steps (refined, dev, {mr}, review, merged, validation, done), the current one red when it failed; why it stands there in one line; your part (author, reviewer, assignee).', { mr: mrWord() }),
        t('One button: Review, Review-fix, Fix, Rebase, Merge, Implement, Validate locally… the same actions as elsewhere, the agent ones through the launcher.'),
        t('Someone waiting on you first (a review asked, feedback on your {mr}, an agent’s question), then what failed, then the normal flow, then what has not started.', { mr: mrWord() }),
        t('My move, Waiting on others (a review, the CI, the merge queue, an agent at work, with how long), or All: the same choice on the phone. A click on a row opens its ticket.'),
      ]
    },
    get source() {
      return t('What the Sprint, Merge requests and Agents blocks read: no request of its own.')
    },
  },
  aside: {
    purpose: t('What the agents saw in passing, out of their task — dead code, a stale doc, a missing test — flagged with suggest_task rather than in a sentence of their summary: one line per subject, until you deal with it.'),
    points: [
      t('Launch… opens the launcher prefilled: its prompt headed by where it comes from, a new worktree from the base or from its agent’s branch, its agent’s CLI and effort, its kind; Enter launches it. After this agent runs it once that agent is done (a step of its chain); Ticket makes it a ticket of the tracker.'),
      t('Later keeps it without its badge; Dismiss takes it out, and the reason is told to the agent, which will not propose it again. Two agents seeing the same thing (same repository, files that meet, close titles) make one line, seen by both.'),
      t('“files changed since”: a commit of another branch touched its files, maybe it was done already. Open lines older than stale_days fold under Older; nothing is removed on its own.'),
    ],
    get source() {
      return t('Prism’s MCP server, given to the agents it launches ([agents.aside]), and the spawn_task calls of the Claude Desktop sessions: no request of its own.')
    },
  },
  routines: {
    purpose: t('What the agents launched on their own at fixed times (the routines) left for you to look at, and when they run next.'),
    points: [
      t('To review: one row per run that waits for you, blocked ones first: the routine, its repository, what its agent said in a line, what it produced (its MR, branch, files, cost). Reply or Open brings its agent out of the archive; Archive takes the row out; Rerun runs it again.'),
      t('A run blocked (a permission, a question) or failed (its CLI, too long) is here whatever its delivery; a ready one delivered to the Digest goes there instead. One that changed no file is archived at once: the history of Settings › Routines keeps it.'),
      t('Under the rows, the next times: a click opens Settings › Routines. The same pile heads the Agents view, and the phone’s Home and Agents.'),
    ],
    get source() {
      return t('Prism’s routines ([[agents.routines]]), run by Prism itself while it runs, its window open or not: no request of its own.')
    },
  },
  retro: {
    purpose: t('The current sprint seen from your side, for the retro: what you delivered, where your time went, what you could focus on.'),
    points: [
      t('Your tickets of the sprint: delivered or carried (and why: blocked, never started, red CI), the time estimated in the tracker against the time logged and the time traced, and the cycle from in progress to done.'),
      t('Where your time went (sprint tickets, other tickets, no ticket, meetings) and, day by day, your longest stretch without a meeting and how often you switched between tickets and MRs: from the Day trail’s traces, an estimate.'),
      t('Reviews you gave, agents you launched and what they cost, the points you write for the retro (kept by sprint) and your notes tagged #retro.'),
      t('Seen by Prism: observations from these figures, compared with your own previous sprint only; the team is one line, without ranking.'),
      t('Copy as Markdown for the team’s retro, or Draft my points: an agent turns it into a few points to keep or change.'),
    ],
    source: t('The sprint and its tickets from the tracker, the Day trail’s journal (on the machine), your logged time, your agents and notes. Each sprint’s figures are kept locally, to compare the next one with.'),
  },
  sprint: {
    get purpose() {
      return t('The tickets of the current sprint, with their {mr} and the next step to move them.', { mr: mrWord() })
    },
    get points() {
      return [
        t('Under the title, the bar shows the progress (high and urgent tickets done) and the day of the sprint; EPICs and the tasks of the time boards (time-tracking entries) are not listed.'),
        t('Grouped by status or assignee; priority flag, status and assignees editable in place.'),
        t('The next step and its action: Assign me, Implement with {actor}, Fix the CI, move to validate…', { actor: actorName() }),
        t('Subtasks under their parent; a click on a ticket opens its preview.'),
        t('Outside sprint (in ⋯ when the block is narrow): also show the tickets in progress (doing or further) that are not in the sprint, in a group of their own, with their board and Add to sprint.'),
        t('Current and Next tabs: Next shows the following sprint, its dates, when it starts and all its tickets.'),
      ]
    },
    get source() {
      return fromTracker({
        clickup: t('ClickUp, the current list of the sprint folder every 45 s; the next sprint and the tickets outside the sprint every 3 min.'),
        jira: t('Jira, the active sprint of the [jira] board (a Scrum board), or its open issues (Kanban), every [jira] poll_seconds; the next sprint and the issues outside the sprint every 3 min.'),
        obsidian: t('Obsidian, the open cards of the [[obsidian.boards]] (those in the pick statuses go to To pick), read from the vault when its files change.'),
      })
    },
  },
  pods: {
    purpose: t('Are the pods you follow healthy, and is there room on their nodes?'),
    points: [
      t('Grouped by target (a cluster, Settings › Infra › Kubernetes › Pods), then namespace; a finished pod is hidden.'),
      t('One card per node: CPU and memory used, requests reserved (what keeps a pod Pending), pods.'),
      t('GitLab runners and the CI jobs of their Kubernetes executor are recognized: a job links to its CI job (GitLab only).'),
    ],
    source: t('Kubernetes watch on each target’s namespaces, metrics every 15 s.'),
  },
  mail: {
    purpose: t('Which messages are waiting for you, without leaving Prism?'),
    points: [
      t('The unread messages of your Microsoft 365 inbox, newest first, and the flagged ones (read or not): sender, subject, age.'),
      t('A click opens the message in Outlook on the web; read there, it leaves the list at the next read.'),
      t('Signed out: the block says so; you sign in once from Settings › Work › Mail, in its provider’s page (Microsoft 365 or Gmail).'),
    ],
    source: t('Microsoft Graph every minute, with the delegated Mail.ReadBasic permission ([mail] client_id, flagged, max): your mailbox only, sender, subject, date and states, never a body.'),
  },
  ci: {
    purpose: t('Is the CI getting slower or flakier?'),
    get points() {
      return [
        onGithub()
          ? t('For main, release/* or pull request workflow runs: median duration, success rate, wait for a runner, run count, each with a 14-day trend.')
          : t('For main, release/* or MR pipelines: median duration, success rate, wait for a runner, pipeline count, each with a 14-day trend.'),
        t('Slowest jobs and their change versus the previous week.'),
        t('Flaky jobs (failed, then passed on retry in the same pipeline), with their log and an Analyse button for {actor}.', { actor: actorName() }),
      ]
    },
    get source() {
      return onGithub() ? t('GitHub Actions workflow runs of the last 14 days (pushes and pull requests), read again every 10 min (only the new ones).') : t('GitLab pipelines of the last 14 days, read again every 10 min (only what changed).')
    },
  },
  vulns: {
    purpose: t('Known vulnerabilities in what we ship, as CI would see them today.'),
    get points() {
      return [
        t('Dependencies of its repositories on their main branch (or [vulnerabilities] branches), and the images a block of your team names ([vulnerabilities] images), base images included.'),
        t('HIGH and CRITICAL with a fix; new ones since the previous scan are flagged.'),
        t('Fix CVE opens a {actor} session with the cve skill (in its repository for a base image); findings accepted in a .trivyignore are listed apart.', { actor: actorName() }),
      ]
    },
    source: t('Local Trivy scan, once a day or with the refresh button; images read from the registries.'),
  },
  envs: {
    purpose: t('Are the deployed applications healthy, and running the version we think?'),
    get points() {
      return [
        t('prod, demo and staging tabs: every workload with its ready pods, restarts, waiting reason or last exit (OOMKilled, CrashLoopBackOff…).'),
        t('In prod and demo, the running image is compared to the latest tag: a workload behind is flagged.'),
        t('Investigate opens a {actor} session that diagnoses with kubectl, read-only.', { actor: actorName() }),
      ]
    },
    source: t('Kubernetes, the namespaces listed in [[environments]], read-only every 30 s.'),
  },
  argocd: {
    purpose: t('Is ArgoCD doing its job: every Application in sync with Git, healthy, its syncs going through?'),
    get points() {
      return [
        t('Need a look: a failed sync, a degraded or missing Application, one off Git (drift) or syncing, with why under it: the resources that failed, the hooks, what was changed outside Git.'),
        t('The others as a wall of names by environment; a customer’s instance has its own tab (the groups of the Helm paths).'),
        t('Sync operations of the last 24 hours, whatever started them: an auto-sync, a change of config, a sync by hand.'),
        t('A click opens the ArgoCD drawer on the Application; Investigate opens a {actor} session when it lands in an environment Prism reads.', { actor: actorName() }),
        t('No versions nor promotion: what runs where, and what waits to ship, is a delivery block’s.'),
      ]
    },
    source: t('ArgoCD’s Applications, read-only as Kubernetes objects every 30 s (every 5 s while a sync runs): the instances of [[argocd]], otherwise the argocd namespaces of the default cluster.'),
  },
  renovate: {
    purpose: t('Dependency updates proposed by Renovate.'),
    get points() {
      return [t('The open Renovate {changes} and their state.', { changes: changes() }), t('Run Renovate now, or check its last run.')]
    },
    get source() {
      return onGithub() ? t('GitHub: the Dependency Dashboard issue, and Renovate’s workflow or app.') : t('GitLab: Renovate MRs and its pipeline.')
    },
  },
  refinement: {
    purpose: t('The tickets to prepare before they can be picked, grouped by EPIC.'),
    get points() {
      return [
        t('Tree view: one row per EPIC with the progress of its tasks, its estimated load, its signals and the next action.'),
        t('On top, up to three things to do: an EPIC to split, the ticket asleep the longest, an EPIC only estimates away from ready.'),
        t('Sort by status (the order of the flow) or by last update (most recent first).'),
        t('Refine or Estimate opens a {actor} session with the refine skill.', { actor: actorName() }),
      ]
    },
    get source() {
      return fromTracker({
        clickup: t('ClickUp, the boards used for refinement (a folder’s quarterly lists, or a list), every 3 min.'),
        jira: t('Jira, the [[jira.queues]] used for refinement (a JQL query each), every 3 min.'),
        obsidian: t('Not offered with Obsidian boards.'),
      })
    },
  },
  agenda: {
    purpose: t('Your meetings of the day.'),
    points: [
      t('The current meeting with its progress, a countdown to the next one, Join for video calls.'),
      t('A notification 5 min before each meeting; a click shows its attendees and invitation.'),
    ],
    source: t('The macOS Calendar, read locally (Exchange and Microsoft 365 accounts included); macOS only.'),
  },
  pickup: {
    purpose: t('The tickets ready to be picked up.'),
    points: [
      t('The boards used to pick from and the next sprint, grouped by board in their colour.'),
      t('Same rows as the sprint: next step, Assign me; Add to Sprint from the ⋯ menu.'),
    ],
    get source() {
      return fromTracker({
        clickup: t('ClickUp, the tickets in the pick statuses of the boards used to pick from, and the next sprint, every 3 min.'),
        jira: t('Jira, the [[jira.queues]] used to pick from (a JQL query each), and the next sprint, every 3 min.'),
        obsidian: t('Obsidian, the cards of the [[obsidian.boards]] in the pick statuses.'),
      })
    },
  },

}
