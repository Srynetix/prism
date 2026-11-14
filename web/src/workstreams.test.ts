import { describe, expect, it } from 'vitest'
import { projectSummaries, stageOf, ticketsFor, waitingWorkstreams, workstreamsOf } from './workstreams'
import { workstreamTitle } from './project-actions'
import type { Agent, MergeRequest, Snapshot, Worktree } from './types'

const NOW = new Date().toISOString()

const wt = (o: Partial<Worktree> & { path: string; repo?: string }): Worktree => ({
  repo: 'shop', branch: null, ticket: null, base: 'main', behind: 0, ahead: 0, dirty: false, changes: 0, untracked: 0, size_bytes: null,
  unpushed: 0, upstream: null, upstream_gone: false, unpulled: 0, head: 'abc', last_commit: NOW, is_main: false, prunable: false, locked: false, ...o,
})
const agent = (o: Partial<Agent> & { id: string }): Agent =>
  ({ provider: 'claude', title: o.id, category: null, tags: [], cwd: '/code/shop', worktree: null, worktree_note: null, adopted: [], branch: null, ticket: null, ticket_linked: false, mr: null, prompt: null, launch: null, created_at: NOW, state: 'running', exit_code: null, status: 'running', needs_action: null, suggestion: null, gate: null, last_activity: NOW, context: null, looping: null, planned: false, usage: null, ...o }) as Agent
const mr = (o: Partial<MergeRequest> & { iid: number; source_branch: string }): MergeRequest =>
  ({ repo: null, project: 'g/shop', title: `MR ${o.iid}`, url: '', state: 'opened', draft: false, target_branch: 'main', ticket: null, tickets: [], author: 'me', author_name: 'Me', assignees: [], reviewers: [], approved_by: [], changes_requested_by: [], approvals_left: 0, pipeline: 'success', merge_status: '', conflicts: false, needs_rebase: false, merge_error: null, auto_merge: false, labels: [], head_pipeline: null, open_threads: 0, size: null, created_at: NOW, updated_at: NOW, merged_at: null, ...o }) as MergeRequest

function snapshot(o: { worktrees?: Worktree[]; agents?: Agent[]; mrs?: MergeRequest[]; dirty?: boolean; repos?: string[] } = {}): Snapshot {
  const names = o.repos ?? ['shop']
  return {
    worktrees: o.worktrees ?? [],
    agents: o.agents ?? [],
    sessions: [],
    archived_sessions: [],
    merge_requests: o.mrs ?? [],
    pipelines: [],
    left: [],
    repos: names.map((name, i) => ({ name, path: `/code/${name}`, project: `g/${name}`, branch: 'main', base: 'main', behind: 0, ahead: 0, dirty: name === 'shop' && !!o.dirty, is_main: i === 0, error: null, forge: null })),
    repositories: names.map((name, i) => ({ name, project: `g/${name}`, primary: i === 0, clone: true, clone_missing: false, worktrees: name === 'shop', merge_queue: false, ticket_required: false, pipelines: false, agent: null, forge: null, workspace: name === 'docs' ? 'Personal' : 'Work' })),
  } as unknown as Snapshot
}

describe('a workstream', () => {
  it('is in progress while an agent runs, in review with its MR open, to clean up once merged', () => {
    expect(stageOf({ agents: [agent({ id: 'a' })], mr: undefined, clone: false })).toBe('active')
    expect(stageOf({ agents: [agent({ id: 'a', state: 'exited' })], mr: mr({ iid: 1, source_branch: 'x' }), clone: false })).toBe('review')
    expect(stageOf({ agents: [], mr: mr({ iid: 1, source_branch: 'x', state: 'merged' }), clone: false })).toBe('cleanup')
    expect(stageOf({ agents: [], mr: undefined, clone: false, upstreamGone: true })).toBe('cleanup')
    expect(stageOf({ agents: [], mr: undefined, clone: false })).toBe('resume')
  })
  it('in the clone is never to clean up', () => {
    expect(stageOf({ agents: [], mr: undefined, clone: true, upstreamGone: true })).toBe('resume')
  })
})

describe('workstreamsOf', () => {
  it('gathers a worktree, its agents and its MR; the clone only with an agent or changes', () => {
    const s = snapshot({
      worktrees: [wt({ path: '/code/shop', is_main: true, branch: 'main' }), wt({ path: '/code/shop.worktrees/tva', branch: 'tva' })],
      agents: [agent({ id: 'a1', title: 'TVA au panier', worktree: '/code/shop.worktrees/tva', cwd: '/code/shop.worktrees/tva', status: 'needs_you' })],
      mrs: [mr({ iid: 412, source_branch: 'tva' })],
    })
    const list = workstreamsOf(s, 'shop')
    expect(list).toHaveLength(1)
    expect(list[0]).toMatchObject({ title: 'TVA au panier', branch: 'tva', stage: 'active', waiting: true, clone: false })
    expect(list[0].mr?.iid).toBe(412)
  })
  it('makes an agent in the clone a workstream on main', () => {
    const s = snapshot({ worktrees: [wt({ path: '/code/shop', is_main: true, branch: 'main' })], agents: [agent({ id: 'a2', title: 'Fix', cwd: '/code/shop/src' })] })
    const [w] = workstreamsOf(s, 'shop')
    expect(w).toMatchObject({ clone: true, branch: 'main', stage: 'active' })
    expect(w.agents.map((a) => a.id)).toEqual(['a2'])
  })
  it('shows a clone whose worktrees are not listed while it has changes', () => {
    expect(workstreamsOf(snapshot({ dirty: true }), 'shop')).toMatchObject([{ clone: true, dirty: true, changes: 0 }])
    expect(workstreamsOf(snapshot(), 'shop')).toHaveLength(0)
  })
  it('leaves out the worktrees hidden and those whose folder is gone', () => {
    const s = snapshot({ worktrees: [wt({ path: '/w/a' }), wt({ path: '/w/b', prunable: true })] })
    expect(workstreamsOf(s, 'shop').map((w) => w.path)).toEqual(['/w/a'])
    expect(workstreamsOf(s, 'shop', { hidden: (i) => i.w.path === '/w/a' })).toEqual([])
  })
})

describe('projectSummaries', () => {
  it('groups by working folder, those waiting first, those at rest last', () => {
    const s = snapshot({
      repos: ['shop', 'api', 'docs'],
      worktrees: [wt({ path: '/code/shop.worktrees/x', branch: 'x' })],
      agents: [agent({ id: 'b', cwd: '/code/api', status: 'needs_you' })],
    })
    const list = projectSummaries(s)
    expect(list.map((p) => p.name)).toEqual(['api', 'shop', 'docs'])
    expect(list[0]).toMatchObject({ waiting: 1, workspace: 'Work' })
    expect(list[2]).toMatchObject({ workspace: 'Personal', resting: true })
  })
})

describe('waitingWorkstreams', () => {
  it('counts the workstreams waiting for me as the page does, one per folder', () => {
    const s = snapshot({
      repos: ['shop', 'api'],
      worktrees: [wt({ path: '/code/shop', is_main: true, branch: 'main' }), wt({ path: '/code/shop.worktrees/tva', branch: 'tva' }), wt({ path: '/code/shop.worktrees/old', branch: 'old', prunable: true })],
      agents: [
        agent({ id: 'a', worktree: '/code/shop.worktrees/tva', cwd: '/code/shop.worktrees/tva', status: 'needs_you' }),
        agent({ id: 'b', worktree: '/code/shop.worktrees/tva', cwd: '/code/shop.worktrees/tva', status: 'needs_you' }),
        agent({ id: 'c', cwd: '/code/shop', status: 'needs_you' }),
        agent({ id: 'd', cwd: '/code/api', status: 'needs_you', read: true } as never),
        agent({ id: 'e', worktree: '/code/shop.worktrees/old', cwd: '/code/shop.worktrees/old', status: 'needs_you' }),
      ],
    })
    expect(waitingWorkstreams(s)).toBe(projectSummaries(s).reduce((n, p) => n + p.waiting, 0))
    expect(waitingWorkstreams(s)).toBe(2)
    expect(waitingWorkstreams(s, (w) => w.path.endsWith('tva'))).toBe(1)
  })
})

describe('ticketsFor', () => {
  it('offers the open tickets of the repository not already worked on, the next one first', () => {
    const t = (id: string, o: object = {}) => ({ id, name: id, status: 'to do', closed: false, repo: null, assignees: [], ...o })
    const s = { ...snapshot(), sprint: { tickets: [t('A'), t('B'), t('C', { closed: true }), t('D', { repo: 'docs' })], statuses: [] } } as unknown as Snapshot
    const taken = workstreamsOf(s, 'shop')
    expect(ticketsFor(s, 'shop', taken).map((x) => x.id)).toEqual(['A', 'B'])
    expect(ticketsFor(s, 'shop', taken, t('B') as never).map((x) => x.id)).toEqual(['B', 'A'])
    expect(ticketsFor(s, 'shop', [{ ticketId: 'A' } as never]).map((x) => x.id)).toEqual(['B'])
  })
})

describe('workstreamTitle', () => {
  it('is the prompt’s first line, cut at a word', () => {
    expect(workstreamTitle('\n  Fix the VAT  \nand more')).toBe('Fix the VAT')
    const long = workstreamTitle('Recompute the VAT on every cart line and add the multi-rate cases to the tests')
    expect(long.endsWith('…')).toBe(true)
    expect(long.length).toBeLessThanOrEqual(61)
  })
})
