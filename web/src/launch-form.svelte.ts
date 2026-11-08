import { untrack } from 'svelte'
import { hasCore, invoke } from './bridge'
import { attachFiles, mention, planWorktree, slug, type Launch, type LaunchSlot, type WorktreePlan } from './agents.svelte'
import type { Step, StepKind } from './agent-chains.svelte'
import { t } from './i18n.svelte'

/**
 * What the two New agent forms share: the computer's launcher (`components/AgentLauncher.svelte`)
 * and the phone's sheet (`remote/PhoneLauncher.svelte`). Each lays its fields out its own way; the
 * draft it is opened with, the new worktree's plan, the folder's tasks for its Then steps, the
 * files attached to its prompt and the steps it adds are the same. The `watch…` functions and
 * `Attachments` are made while a component is created (they hold effects).
 */

type NewWorktree = NonNullable<Launch['worktree']>

/** Without a title, the prompt's first line (60 characters). */
export const titleOf = (title: string, prompt: string) => title.trim() || (prompt.trim().split('\n')[0] ?? '').slice(0, 60).trim()

/** A worktree's name from the prompt: its first four words, its @mentions left out. */
export const nameOf = (prompt: string) => slug(prompt.replace(/@\S+/g, '').trim().split(/\s+/).slice(0, 4).join(' '))

/**
 * The new worktree asked for: `name` typed (else `fallback`, unless an existing `branch` names it),
 * on `branch` or a new one from `base` (empty: the repository's), in `repo`, `also` in others.
 */
export function newWorktree(w: { name: string; fallback: string; branch: string; base: string; repo: string; also?: string[] }): NewWorktree {
  return { name: w.name ? slug(w.name) : w.branch ? undefined : w.fallback || undefined, branch: w.branch || undefined, repo: w.repo || undefined, base: (!w.branch && w.base.trim()) || undefined, ...(w.also?.length ? { also: w.also } : {}) }
}

/**
 * The slot's draft (`slotOf`: a prop may change), taken once each time an action opens the form with one (its own slot only):
 * `apply` fills the form, untracked (what it reads does not open it again).
 */
export function takeDraft(slotOf: () => LaunchSlot, apply: (d: Launch) => void) {
  $effect(() => {
    const slot = slotOf()
    const d = slot.draft
    if (!d) return
    slot.draft = null
    untrack(() => apply(d))
  })
}

/**
 * What the new worktree will be, as Prism will create it: asked after a short pause while typing,
 * so a name already taken or a branch already checked out shows before launching. `ask`: its
 * title and worktree, `null` when no new worktree is asked (nor in the browser preview).
 */
export function watchPlan(ask: () => { title: string; worktree: NewWorktree } | null) {
  const plan = $state<{ value: WorktreePlan | null }>({ value: null })
  $effect(() => {
    const q = hasCore() ? ask() : null
    if (!q) return void (plan.value = null)
    let gone = false
    const timer = setTimeout(() => planWorktree(q.title, q.worktree).then((p) => !gone && (plan.value = p), () => !gone && (plan.value = null)), 250)
    return () => {
      gone = true
      clearTimeout(timer)
    }
  })
  return plan
}

/** Where an agent will run, for its folder's tasks: a new worktree's repository, or a folder. */
export type TasksWhere = { repo: string | null } | { cwd: string }

/**
 * The tasks its folder declares (a new worktree: its repository's), while `where` says one (`null`:
 * not wanted now): a task step's choices on a phone, suggestions at the desk. `for`: the place they
 * were read for, so a list still loading or another folder's is told apart.
 */
export function watchTasks(where: () => TasksWhere | null) {
  const tasks = $state<{ list: string[]; for: string }>({ list: [], for: '' })
  $effect(() => {
    const at = where()
    if (!at || !hasCore()) return
    const key = JSON.stringify(at)
    let gone = false
    const done = (list: string[]) => {
      if (gone) return
      tasks.list = list
      tasks.for = key
    }
    invoke<string[]>('launch_tasks', at).then(done, () => done([]))
    return () => {
      gone = true
    }
  })
  return tasks
}
/** The tasks were read for that place. */
export const tasksFor = (tasks: { for: string }, where: TasksWhere) => tasks.for === JSON.stringify(where)
/** The folder's test task, failing that its first. */
export const testTask = (tasks: string[]) => tasks.find((tk) => /\btest\b/.test(tk)) ?? tasks[0] ?? ''

/** A new Then step of that kind, as both forms add it; a task step runs `task`. */
export function newStep(kind: StepKind, task = ''): Step {
  switch (kind) {
    case 'task':
      return { kind, when: 'done', command: task }
    case 'prompt':
      return { kind, when: 'done', text: '' }
    case 'mr':
      return { kind, when: 'ok' }
    case 'agent':
      return { kind, when: 'ask', text: 'Review the code of MR !{mr}.' }
    default:
      return { kind, when: 'done' }
  }
}

/** A file attached to the prompt: its path on the computer (`@path` in the prompt), an image's preview. */
export interface Attached {
  path: string
  name: string
  url: string | null
  size: number
}

/**
 * Files attached to a launch's prompt: saved where agents read them, their `@path` added to the
 * prompt; an image previewed until it is removed, the form launched or closed.
 */
export class Attachments {
  list = $state<Attached[]>([])
  busy = $state(false)

  constructor() {
    // Closed: the previews let go.
    $effect(() => () => this.release())
  }

  /** Saved on the computer: the prompt with their `@path`s. Throws when they could not be. */
  async add(files: File[], prompt: string): Promise<string> {
    if (!files.length) return prompt
    this.busy = true
    try {
      const paths = await attachFiles(files, null)
      paths.forEach((path, i) => this.list.push({ path, name: files[i].name || t('pasted image'), url: files[i].type.startsWith('image/') ? URL.createObjectURL(files[i]) : null, size: files[i].size }))
      return `${prompt.trimEnd()} ${paths.map(mention).join(' ')} `.trimStart()
    } finally {
      this.busy = false
    }
  }

  /** The `i`th taken off: the prompt without its `@path`. */
  remove(i: number, prompt: string): string {
    const a = this.list[i]
    if (a.url) URL.revokeObjectURL(a.url)
    this.list.splice(i, 1)
    return prompt.replace(mention(a.path), '').replace(/ {2,}/g, ' ')
  }

  /** All let go (launched, or another draft): the files stay on the computer. */
  clear() {
    this.release()
    this.list = []
  }

  private release() {
    for (const a of this.list) if (a.url) URL.revokeObjectURL(a.url)
  }
}
