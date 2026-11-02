import { describe, expect, it } from 'vitest'
import type { Agent, MergeRequest, Snapshot } from '../../types'

// The modules follow the page's address (filter.svelte.ts): none under Node.
const page = globalThis as { location?: unknown; addEventListener?: unknown }
page.location ??= { hash: '' }
page.addEventListener ??= () => {}
const { errands } = await import('./errands')
const { daypart, season, specials } = await import('./world')
const { NEW_GROWTH, takeShots, TROPHIES } = await import('./growth')

const mr = (iid: number, over: Partial<MergeRequest> = {}): MergeRequest => ({
  iid, repo: null, project: null, title: `MR ${iid}`, url: '', state: 'opened', draft: false, source_branch: `b${iid}`, target_branch: 'main', ticket: null,
  author: 'me', author_name: 'Me', assignees: [], reviewers: [], approved_by: [], changes_requested_by: [], approvals_left: 1, pipeline: 'success',
  merge_status: 'mergeable', conflicts: false, needs_rebase: false, merge_error: null, auto_merge: false, labels: [], head_pipeline: null, open_threads: 0,
  size: null, created_at: '2026-10-20T08:00:00Z', updated_at: '2026-10-20T08:00:00Z', merged_at: null, ...over,
}) as MergeRequest

const agent = (id: string, over: Partial<Agent> = {}) => ({ id, provider: 'claude', title: `Agent ${id}`, cwd: `/src/${id}`, worktree: null, state: 'running', status: 'running', last_activity: '2026-10-23T09:00:00Z', created_at: '2026-10-23T08:00:00Z', ...over }) as Agent

const snap = (merge_requests: MergeRequest[], agents: Agent[] = []) =>
  ({ me: 'me', merge_requests, agents, merge_queue_label: null, repositories: [], repos: [], worktrees: [] }) as unknown as Snapshot

const NOW = Date.parse('2026-10-23T12:00:00Z')

describe('errands', () => {
  it('brings an agent waiting for you first, then the oldest review asked of you, then my MRs to act on', () => {
    const list = errands(snap([
      mr(1, { pipeline: 'failed' }),
      mr(2, { author: 'bob', author_name: 'Bob', reviewers: ['me'], updated_at: '2026-10-23T09:00:00Z' }),
      mr(3, { author: 'sam', author_name: 'Sam', reviewers: ['me'], updated_at: '2026-10-22T09:00:00Z' }),
      mr(4, { approvals_left: 0 }),
    ], [agent('a', { status: 'needs_you' }), agent('b')]), NOW)
    expect(list.map((e) => e.key)).toEqual(['agent:a', 'mr:!3', 'mr:!2', 'mr:!1:CI failed', 'mr:!4:Ready to merge'])
    expect(list[1].text).toContain('Sam')
  })

  it('leaves out what does not wait for you: reviews done, drafts, others’ MRs, agents at work', () => {
    const list = errands(snap([
      mr(5, { author: 'bob', reviewers: ['me'], approved_by: ['me'] }),
      mr(6, { draft: true, pipeline: 'failed' }),
      mr(7, { author: 'bob', pipeline: 'failed' }),
      mr(8, { approvals_left: 1, pipeline: 'success' }),
    ], [agent('c'), agent('d', { state: 'exited', status: 'needs_you' })]), NOW)
    expect(list).toEqual([])
  })
})

describe('its window', () => {
  it('follows the hour and the season', () => {
    expect(daypart(new Date(2026, 9, 23, 7).getTime())).toBe('dawn')
    expect(daypart(new Date(2026, 9, 23, 14).getTime())).toBe('day')
    expect(daypart(new Date(2026, 9, 23, 19).getTime())).toBe('dusk')
    expect(daypart(new Date(2026, 9, 23, 23).getTime())).toBe('night')
    expect(season(new Date(2026, 0, 10).getTime())).toBe('winter')
    expect(season(new Date(2026, 9, 23).getTime())).toBe('autumn')
  })

  it('knows its birthday (a year on, not the day it hatched), the sprint’s last day and Friday afternoon', () => {
    const born = new Date(2025, 9, 23, 10).getTime()
    expect(specials(new Date(2026, 9, 23, 16).getTime(), born, '2026-10-23T17:00:00')).toEqual(['birthday', 'sprint-end', 'no-deploy'])
    expect(specials(new Date(2025, 9, 23, 11).getTime(), born)).toEqual([])
    expect(specials(new Date(2026, 9, 31, 10).getTime(), born)).toEqual(['halloween'])
  })
})

describe('takeShots', () => {
  it('photographs its hatching once, then each stage, each new form and each trophy', () => {
    const g = structuredClone(NEW_GROWTH)
    const born = 1000
    expect(takeShots(g, { stage: 'baby', form: null }, born, [], 2000).map((s) => s.caption)).toEqual(['Hatched!'])
    expect(takeShots(g, { stage: 'baby', form: null }, born, [], 3000)).toEqual([])
    expect(takeShots(g, { stage: 'grown-up', form: 'reviewer' }, born, [TROPHIES[0]], 4000).map((s) => s.caption)).toEqual(['All grown up', 'The Reviewer', TROPHIES[0].label])
    expect(takeShots(g, { stage: 'grown-up', form: 'reviewer' }, born, [], 5000)).toEqual([])
    expect(g.album.map((s) => s.at)).toEqual([1000, 4000, 4000, 4000])
  })

  it('photographs an older pet as it is now, after its hatching', () => {
    const g = structuredClone(NEW_GROWTH)
    expect(takeShots(g, { stage: 'young', form: null }, 1000, [], 9000).map((s) => [s.caption, s.stage])).toEqual([['Hatched!', 'baby'], ['Young now', 'young']])
  })
})
