import { describe, expect, it } from 'vitest'
import { adjustPrompt, alertedOverlaps, hunksOf, overlapsAt, visibleOverlaps, whoAdjusts } from './overlaps'
import type { Overlap, Snapshot } from './types'
import type { AgentDiff } from './bindings/AgentDiff'

const pair = (level: 'lines' | 'file', merged = false): Overlap => ({
  repo: 'shop', base: 'main', a: '/w/a', b: '/w/b', a_branch: 'agent/a', b_branch: 'agent/b', level, merged, key: 'k',
  files: [{ path: 'bar.rs', level, a: [{ start: 92, len: 6 }], b: [{ start: 95, len: 1 }] }],
})
const snap = (pairs: Overlap[], cfg: Partial<{ level: string; alert: string }> = {}) =>
  ({
    overlaps: { pairs, other_base: [] },
    agents: [{ id: 'x', title: 'Agent A', worktree: '/w/a', cwd: '/w/a', adopted: [], state: 'running', created_at: '2026-11-05T10:00:00Z' }],
    merge_requests: [{ iid: 7, repo: null, source_branch: 'agent/a', state: 'opened', created_at: '2026-11-05T09:00:00Z' }],
    worktrees: [{ repo: 'shop', path: '/w/a', branch: 'agent/a' }, { repo: 'shop', path: '/w/b', branch: 'agent/b' }],
    repositories: [{ name: 'shop', primary: true }],
    agents_settings: { overlaps: { enabled: true, level: 'file', alert: 'lines', ignore: [], ...cfg } },
  }) as unknown as Snapshot

describe('overlaps', () => {
  it('shows chips from the level, alerts from the alert setting', () => {
    const s = snap([pair('lines'), { ...pair('file'), b: '/w/c' }])
    expect(visibleOverlaps(s)).toHaveLength(2)
    expect(alertedOverlaps(s)).toHaveLength(1)
    expect(visibleOverlaps(snap([pair('file')], { level: 'lines' }))).toHaveLength(0)
    expect(alertedOverlaps(snap([pair('file')], { alert: 'file' }))).toHaveLength(1)
    expect(alertedOverlaps(snap([pair('lines')], { alert: 'off' }))).toHaveLength(0)
  })

  it('names the other side; a merged side sees nothing', () => {
    const s = snap([pair('lines')])
    expect(overlapsAt(s, ['/w/b'])[0].otherName).toBe('Agent A')
    expect(overlapsAt(snap([pair('lines', true)]), ['/w/a'])).toHaveLength(0)
  })

  it('asks the side without an MR to adjust', () => {
    expect(whoAdjusts(snap([pair('lines')]), pair('lines'))).toBe('b')
  })

  it('keeps the hunks that meet the ranges, and joins them to the prompt', () => {
    const diff = { files: [{ old_path: 'bar.rs', new_path: 'bar.rs', diff: '@@ -10,2 +10,2 @@\n-a\n+b\n@@ -90,9 +90,9 @@\n-c\n+d\n' }] } as unknown as AgentDiff
    const o = pair('lines')
    expect(hunksOf(diff, o.files[0], 'a')).toEqual(['@@ -90,9 +90,9 @@\n-c\n+d'])
    const p = adjustPrompt(snap([o]), o, 'b', diff, 'fr')
    expect(p).toContain('« Agent A »')
    expect(p).toContain('`bar.rs`, lignes 92–97')
    expect(p).toContain('@@ -90,9')
  })
})
