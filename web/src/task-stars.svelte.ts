import { pref, setPref } from './layout.svelte'

/**
 * The tasks starred as part of a repository's stack (Tasks panel), by the agents' folder they
 * start from (`Agent.cwd`: every worktree of a repository shares it): what the Dev servers block
 * starts. A star keeps its command line, so the stack starts without reading the task files.
 */
export interface StarredTask {
  file: string
  name: string
  command: string
}
const STARS = 'agents.tasks.starred'
export const starredTasks = $state<Record<string, StarredTask[]>>(pref(STARS, {}))
export const isStarred = (repo: string, file: string, name: string) => (starredTasks[repo] ?? []).some((t) => t.file === file && t.name === name)
export function toggleStar(repo: string, task: StarredTask) {
  const list = starredTasks[repo] ?? []
  const next = list.some((t) => t.file === task.file && t.name === task.name) ? list.filter((t) => !(t.file === task.file && t.name === task.name)) : [...list, task]
  if (next.length) starredTasks[repo] = next
  else delete starredTasks[repo]
  setPref(STARS, $state.snapshot(starredTasks))
}
