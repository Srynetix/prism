import { describe, expect, it } from 'vitest'
import { SORTS, allRepos, repoNotes, type Note } from './notes.svelte'
import { agentRepo } from './repositories'
import type { Snapshot } from './types'

const note = (text: string, updated_at: string, status: Note['status'] = null): Note => ({ id: text, text, created_at: updated_at, updated_at, kind: 'sticky', status })
const NOTES = [
  note('#infra rotate the keys', '2026-10-10T09:00:00Z'),
  note('call Ana', '2026-10-14T09:00:00Z', 'todo'),
  note('Budget 10', '2026-10-12T09:00:00Z', 'doing'),
  note('budget 9 #perso', '2026-10-11T09:00:00Z'),
]
const order = (by: keyof typeof SORTS) => [...NOTES].sort(SORTS[by].compare).map((n) => n.text)

describe('note orders', () => {
  it('by name: tags left out, case ignored, numbers as numbers', () => {
    expect(order('name')).toEqual(['budget 9 #perso', 'Budget 10', 'call Ana', '#infra rotate the keys'])
  })
  it('by date: last changed first', () => {
    expect(order('date')).toEqual(['call Ana', 'Budget 10', 'budget 9 #perso', '#infra rotate the keys'])
  })
  it('by status: in progress, to do, then plain notes', () => {
    expect(order('status').slice(0, 2)).toEqual(['Budget 10', 'call Ana'])
  })
  it('by tag: first tag, untagged last', () => {
    expect(order('tag').slice(0, 2)).toEqual(['#infra rotate the keys', 'budget 9 #perso'])
  })
})

describe('notes of a repository', () => {
  const of = (text: string, repo: string | null, extra: Partial<Note> = {}): Note => ({ ...note(text, '2026-10-10T09:00:00Z'), repo, ...extra })
  const LIST = [
    of('plain', 'app'),
    of('later', 'app', { kind: 'reminder', remind_at: '2026-10-20T09:00:00Z' }),
    of('soon', 'app', { kind: 'reminder', remind_at: '2026-10-19T09:00:00Z' }),
    of('finished', 'app', { status: 'done' }),
    of('rung', 'app', { kind: 'reminder', done_at: '2026-10-11T09:00:00Z' }),
    of('elsewhere', 'infra', { status: 'todo' }),
    of('none', null),
  ]
  it('lists the open ones, reminders first by time', () => {
    expect(repoNotes('app', LIST).map((n) => n.text)).toEqual(['soon', 'later', 'plain'])
  })
  it('counts the open notes of each repository, by name', () => {
    expect(allRepos(LIST)).toEqual([{ repo: 'app', count: 3 }, { repo: 'infra', count: 1 }])
  })
})

describe('agent repository', () => {
  const snap = {
    worktrees: [{ repo: 'api', path: '/w/elsewhere/feat' }],
    repos: [{ name: 'app', path: '/dev/app' }, { name: 'sub', path: '/dev/app/sub' }],
  } as unknown as Snapshot
  const at = (cwd: string, worktree: string | null = null) => agentRepo({ cwd, worktree }, snap)
  it('takes its worktree’s repository, then the deepest clone around its folder', () => {
    expect(at('/w/elsewhere/feat', '/w/elsewhere/feat')).toBe('api')
    expect(at('/dev/app/src')).toBe('app')
    expect(at('/dev/app/sub/x')).toBe('sub')
    expect(at('/dev/app.worktrees/fix')).toBe('app')
    expect(at('/dev/application')).toBeNull()
  })
})
