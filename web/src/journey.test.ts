import { afterEach, describe, expect, it } from 'vitest'
import { filterJourneys, journal, journeyOf, journeys, stepStates } from './journey'
import { live } from './live.svelte'
import type { Agent, MergeRequest, Snapshot, Ticket } from './types'

const ME = { id: 1, name: 'Denis B', initials: 'DB', color: null, avatar: null }
const SAM = { id: 2, name: 'Sam Lee', initials: 'SL', color: null, avatar: null }

const ticket = (id: string, over: Partial<Ticket> = {}): Ticket => ({
  id, task_id: id.toLowerCase(), parent: null, priority: null, task_type: null, task_type_icon: null, list: null, repo: null, description: null,
  time_estimate: null, name: `Ticket ${id}`, status: 'in progress', closed: false, assignees: [ME], url: '', created_at: '2026-10-20T08:00:00Z',
  updated_at: '2026-10-28T08:00:00Z', fields: [], waiting_on: [], blocking: [], ...over,
}) as Ticket

const mr = (iid: number, over: Partial<MergeRequest> = {}): MergeRequest => ({
  iid, repo: null, project: null, title: `MR ${iid}`, url: '', state: 'opened', draft: false, source_branch: `b${iid}`, target_branch: 'main', ticket: null, tickets: [],
  author: 'me', author_name: 'Denis B', assignees: [], reviewers: ['alex'], approved_by: [], changes_requested_by: [], approvals_left: 1, pipeline: 'success',
  merge_status: 'mergeable', conflicts: false, needs_rebase: false, merge_error: null, auto_merge: false, labels: [], head_pipeline: null, open_threads: 0,
  size: null, created_at: '2026-10-27T08:00:00Z', updated_at: '2026-10-29T08:00:00Z', merged_at: null, ...over,
}) as MergeRequest

const forTicket = (id: string) => ({ ticket: id, tickets: [id] })

const agent = (id: string, over: Partial<Agent> = {}): Agent => ({
  id, provider: 'claude', title: `${id} · Implement`, category: null, tags: [], cwd: '/w', worktree: null, worktree_note: null, adopted: [], branch: null,
  ticket: null, ticket_linked: false, mr: null, prompt: null, launch: null, created_at: '2026-10-27T07:00:00Z', state: 'running', exit_code: null,
  status: 'running', needs_action: null, suggestion: null, last_activity: '2026-10-27T07:40:00Z', context: null, ...over,
}) as Agent

const snap = (tickets: Ticket[], merge_requests: MergeRequest[] = [], agents: Agent[] = []) =>
  ({
    me: 'me', tracker_me: ME, merge_requests, agents, worktrees: [], merge_queue_label: null,
    repositories: [{ name: 'shop', primary: true, merge_queue: false, worktrees: true }],
    sprint: { name: 'Sprint 44', tickets, statuses: [], boards: [] }, pickup: { boards: [] }, outside_sprint: { statuses: [], tickets: [], list_statuses: {} },
  }) as unknown as Snapshot

afterEach(() => {
  live.snapshot = null
})

describe('journeyOf', () => {
  it('puts a review asked of me first, with its author', () => {
    const m = mr(1301, { ...forTicket('PRJ-418'), author: 'sam', author_name: 'Sam Lee', reviewers: ['me'] })
    const s = snap([], [m])
    const j = journeyOf(s, null, [], m)
    expect(j).toMatchObject({ role: 'reviewer', move: 'me', urgency: 0, step: 'review', why: 'Sam waits for your review' })
  })

  it('says how many threads wait on my MR', () => {
    const t = ticket('PRJ-412')
    const j = journeyOf(snap([t], [mr(1294, { ...forTicket('PRJ-412'), open_threads: 2 })]), t, [])
    expect(j).toMatchObject({ role: 'author', move: 'me', urgency: 0, step: 'review', why: '2 unresolved threads' })
  })

  it('marks a red CI at the MR step, with the stage that failed', () => {
    const t = ticket('PRJ-405')
    const head = { id: 9, stages: [{ name: 'test', status: 'failed', failed: 1 }] }
    const j = journeyOf(snap([t], [mr(1288, { ...forTicket('PRJ-405'), pipeline: 'failed', head_pipeline: head as never })]), t, [])
    expect(j).toMatchObject({ step: 'mr', failed: true, urgency: 1, move: 'me', why: 'CI failed: test' })
    expect(stepStates(j)).toEqual(['done', 'done', 'ko', 'todo', 'todo', 'todo', 'todo'])
  })

  it('waits on others while the reviewers have not approved', () => {
    const t = ticket('PRJ-410')
    const j = journeyOf(snap([t], [mr(1290, forTicket('PRJ-410'))]), t, [])
    expect(j).toMatchObject({ step: 'review', move: 'others', urgency: 2, why: 'Waiting for review' })
  })

  it('goes to validation once merged', () => {
    const t = ticket('PRJ-399', { status: 'review' })
    const j = journeyOf(snap([t], [mr(1280, { ...forTicket('PRJ-399'), state: 'merged', merged_at: '2026-10-28T10:00:00Z' })]), t, [])
    expect(j.step).toBe('validate')
    expect(j.why).toBe('!1280 merged, move the ticket on')
    expect(stepStates(j)).toEqual(['done', 'done', 'done', 'done', 'done', 'now', 'todo'])
  })

  it('reads a ticket to do as not started, an agent at work as the agent’s', () => {
    const t = ticket('PRJ-421', { status: 'to do' })
    expect(journeyOf(snap([t]), t, [])).toMatchObject({ step: 'dev', urgency: 3, move: 'me', why: 'Refined, not started' })
    const a = agent('a1', { ticket: 'PRJ-421' })
    expect(journeyOf(snap([t], [], [a]), t, [])).toMatchObject({ step: 'dev', urgency: 2, move: 'others', why: 'Agent “a1 · Implement” at work' })
    const asking = agent('a2', { ticket: 'PRJ-421', status: 'needs_you' })
    expect(journeyOf(snap([t], [], [asking]), t, [])).toMatchObject({ urgency: 0, move: 'me' })
  })

  it('sees a ticket before the first development status as being refined', () => {
    live.snapshot = { conventions: { workflow_statuses: ['backlog', 'to do', 'in progress', 'review', 'complete'], before_dev_statuses: ['to do'], in_progress_statuses: ['in progress'], review_statuses: ['review'], after_merge_statuses: ['complete'], done_statuses: ['complete'], validation_statuses: [] } } as unknown as Snapshot
    const t = ticket('PRJ-430', { status: 'backlog' })
    expect(journeyOf(snap([t]), t, [])).toMatchObject({ step: 'refined', urgency: 3 })
  })
})

describe('journeys', () => {
  it('orders: waited on, failed, the flow, not started; done tickets left out', () => {
    const tickets = [ticket('A-1', { status: 'to do' }), ticket('A-2'), ticket('A-3'), ticket('A-4', { status: 'complete' }), ticket('A-5', { assignees: [SAM] })]
    const mrs = [
      mr(2, forTicket('A-2')),
      mr(3, { ...forTicket('A-3'), pipeline: 'failed' }),
      mr(9, { ...forTicket('A-9'), author: 'sam', author_name: 'Sam Lee', reviewers: ['me'] }),
    ]
    const rows = journeys(snap(tickets, mrs))
    expect(rows.map((j) => j.id)).toEqual(['!9', 'A-3', 'A-2', 'A-1'])
    expect(filterJourneys(rows, 'others').map((j) => j.id)).toEqual(['A-2'])
    expect(filterJourneys(rows, 'me').map((j) => j.id)).toEqual(['!9', 'A-3', 'A-1'])
  })

  it('adds a ticket of someone else that my running agent works on, and my MR without a ticket', () => {
    const other = ticket('B-1', { assignees: [SAM] })
    const rows = journeys(snap([other], [mr(5)], [agent('x', { ticket: 'B-1' })]))
    expect(rows.map((j) => [j.id, j.role])).toEqual([['B-1', 'assignee'], ['!5', 'author']])
  })
})

describe('journal', () => {
  it('groups the ticket’s events by stage, the steps ahead dashed', () => {
    const t = ticket('PRJ-412', { time_estimate: 7 * 3_600_000 })
    const m = mr(1294, { ...forTicket('PRJ-412'), open_threads: 2 })
    const s = snap([t], [m], [agent('a1', { ticket: 'PRJ-412', state: 'exited', branch: 'prj-412-export', worktree: '/w/prj-412' })])
    const stages = journal(s, t, journeyOf(s, t, []), 'Sprint 44')
    expect(stages.map((g) => g.stage)).toEqual(['refinement', 'development', 'review', 'delivery'])
    expect(stages[0].events[0]).toMatchObject({ text: 'Created', detail: 'estimated 7h00 · in Sprint 44', tone: 'ok' })
    expect(stages[1].events.map((e) => [e.text, e.tone])).toEqual([['Agent “a1 · Implement”', 'ok'], ['!1294 opened', 'ok']])
    expect(stages[2].events.map((e) => e.tone)).toEqual(['ok', 'now', 'now'])
    expect(stages[3].events.map((e) => [e.text, e.tone])).toEqual([['Merge', 'todo'], ['Validation', 'todo'], ['Done', 'todo']])
  })

  it('dates the merge and ends on done', () => {
    const t = ticket('PRJ-300', { status: 'complete' })
    const m = mr(1200, { ...forTicket('PRJ-300'), state: 'merged', merged_at: '2026-10-25T10:00:00Z', approved_by: ['alex'] })
    const s = snap([t], [m])
    const delivery = journal(s, t, journeyOf(s, t, [])).find((g) => g.stage === 'delivery')!
    expect(delivery.events.map((e) => [e.text, e.tone, e.at])).toEqual([['!1200 merged into main', 'ok', '2026-10-25T10:00:00Z'], ['Done (complete)', 'ok', '2026-10-28T08:00:00Z']])
  })

  it('lists the environments rolled out since the merge, the others ahead', () => {
    const t = ticket('PRJ-399', { status: 'review' })
    const m = mr(1280, { ...forTicket('PRJ-399'), state: 'merged', merged_at: '2026-10-28T10:00:00Z' })
    const env = (name: string, at: string) => ({ env: name, context: '', cluster: '', namespace: name, error: null, workloads: [{ updated_at: at }] })
    const s = { ...snap([t], [m]), environments: { envs: [env('prod', '2026-10-20T10:00:00Z'), env('staging', '2026-10-29T09:00:00Z')], clusters: [] } } as unknown as Snapshot
    const delivery = journal(s, t, journeyOf(s, t, [])).find((g) => g.stage === 'delivery')!
    expect(delivery.events.map((e) => [e.key, e.tone])).toEqual([['merged:!1280', 'ok'], ['env:staging', 'ok'], ['validate', 'now'], ['env:prod', 'todo'], ['todo:done', 'todo']])
  })

  it('reads the panel’s ticket from the reviewer’s side when its MR asks for my review', () => {
    const t = ticket('PRJ-418', { assignees: [SAM] })
    const m = mr(1301, { ...forTicket('PRJ-418'), author: 'sam', author_name: 'Sam Lee', reviewers: ['me'] })
    expect(journeyOf(snap([t], [m]), t, [])).toMatchObject({ role: 'reviewer', move: 'me', urgency: 0 })
  })
})
