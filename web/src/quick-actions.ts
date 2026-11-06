import { live } from './live.svelte'
import { agentsSettings, launchNow, openLauncher, type Launch } from './agents.svelte'
import { hasWorktrees } from './repositories'
import { placeIn } from './projects'
import { toast } from './toast.svelte'
import { used } from './feature-use.svelte'
import { t } from './i18n.svelte'
import type { QuickAction } from './bindings/QuickAction'

/**
 * Quick actions (`[[agents.quick_actions]]`): the same job asked again and again of an agent on a
 * repository (bring its main branch up to date, scan it), launched in one click from its project in
 * the Agents view, the palette and the phone. ⇧-click on the computer opens the launcher instead,
 * prefilled, to change it before it goes.
 */

/** The quick actions offered on a repository: those that name it, or no repository at all. */
export function quickActionsOn(repo: string, all: QuickAction[] = agentsSettings().quick_actions ?? []): QuickAction[] {
  return all.filter((a) => a.name.trim() && a.prompt.trim() && (!a.repos.length || a.repos.includes(repo)))
}

/** Those offered on it now: none without a local clone (or in error), where none could run. */
export function quickActionsHere(repo: string): QuickAction[] {
  const r = live.snapshot?.repos?.find((x) => x.name === repo)
  return r?.path && !r.error ? quickActionsOn(repo) : []
}

/** Kept in sight among the projects: a repository a quick action names (one on every repository does not). */
export const namedByQuickAction = (repo: string) => quickActionsHere(repo).some((a) => a.repos.includes(repo))

/** Its prompt on that repository: `{repo}` its name, `{base}` its default branch. */
export const quickPrompt = (a: QuickAction, repo: { name: string; base: string }) => a.prompt.replaceAll('{repo}', repo.name).replaceAll('{base}', repo.base || 'main')

/**
 * Its launch on that repository: a new worktree when it asks for one and the repository makes
 * them, otherwise the clone as it is; its model and effort when it names them. Its mode always said (none: `default`, the CLI's own), so the
 * launcher of a ⇧-click proposes the one a click launches with. `null` without a clone.
 */
export function quickLaunch(a: QuickAction, repo: { name: string; path: string; base: string } | undefined, worktrees: boolean): Launch | null {
  if (!repo?.path) return null
  return {
    title: a.name,
    prompt: quickPrompt(a, repo),
    from: a.name,
    ...placeIn(repo, a.worktree && worktrees),
    permission_mode: a.mode || 'default',
    ...(a.kind ? { category: a.kind } : {}),
    ...(a.model ? { model: a.model } : {}),
    ...(a.effort ? { effort: a.effort } : {}),
  }
}

/** Launched at once on the repository; `edit`: the launcher, prefilled, instead. */
export async function runQuickAction(a: QuickAction, repo: string, edit = false) {
  const r = live.snapshot?.repos?.find((x) => x.name === repo)
  const launch = quickLaunch(a, r, hasWorktrees(repo))
  if (!launch) return toast(t('{repo}: no local clone', { repo }))
  used('quick-action')
  if (edit) return openLauncher(launch, null, null, repo)
  await launchNow(launch)
}
