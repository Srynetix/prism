import { describe, expect, it } from 'vitest'
import type { Snapshot } from './types'

// The tracker's module follows the page's address (filter.svelte.ts): none under Node.
const page = globalThis as { location?: unknown; addEventListener?: unknown }
page.location ??= { hash: '' }
page.addEventListener ??= () => {}
const { knownMr, mrScopeOf, refOfUrl, refsIn, withMrs } = await import('./linkify')
const { live } = await import('./live.svelte')

const snap = {
  conventions: { ticket_prefix: 'SHOP' },
  ticket_base: 'https://tracker.example.org/browse/',
  merge_requests: [
    { iid: 1288, repo: null, url: 'https://gitlab.example.org/shop/app/-/merge_requests/1288' },
    { iid: 214, repo: 'helm', url: 'https://gitlab.example.org/shop/helm/-/merge_requests/214' },
  ],
  sprint: { tickets: [{ id: 'SHOP-433', task_id: '86c1', repo: null, url: 'https://app.clickup.com/t/86c1' }] },
  pickup: null,
  time_tasks: null,
  next_sprint: null,
  outside_sprint: { tickets: [] },
} as unknown as Snapshot
live.snapshot = snap

describe('refsIn', () => {
  it('finds tickets and MRs named in a line, not inside a URL', () => {
    const line = 'Fixed SHOP-433 in !1291, see https://tracker.example.org/browse/SHOP-401 and shop-12.'
    expect(refsIn(line).map((r) => [r.text, r.ref])).toEqual([
      ['SHOP-433', { ticket: 'SHOP-433' }],
      ['!1291', { mr: { iid: 1291, repo: null } }],
      ['shop-12', { ticket: 'SHOP-12' }],
    ])
    expect(line.slice(refsIn(line)[1].start, refsIn(line)[1].end)).toBe('!1291')
  })
})

describe('refOfUrl', () => {
  it("an MR of a project Prism reads, whatever its tab, in its repository", () => {
    expect(refOfUrl('https://gitlab.example.org/shop/app/-/merge_requests/1301/diffs?view=inline', snap)).toEqual({ mr: { iid: 1301, repo: null } })
    expect(refOfUrl('https://gitlab.example.org/shop/helm/-/merge_requests/215', snap)).toEqual({ mr: { iid: 215, repo: 'helm' } })
  })
  it('a ticket Prism shows, or one under the tracker’s base', () => {
    expect(refOfUrl('https://app.clickup.com/t/86c1/', snap)).toEqual({ ticket: 'SHOP-433' })
    expect(refOfUrl('https://tracker.example.org/browse/shop-401', snap)).toEqual({ ticket: 'SHOP-401' })
  })
  it('anything else stays a plain link', () => {
    expect(refOfUrl('https://gitlab.example.org/other/-/merge_requests/3', snap)).toBeNull()
    expect(refOfUrl('https://example.org/SHOP-401', snap)).toBeNull()
    expect(refOfUrl('https://docs.example.org/', snap)).toBeNull()
    // A local server, the agent's or a colleague's: the browser.
    expect(refOfUrl('http://portfolio.localhost:3000/SHOP-401', snap)).toBeNull()
    expect(refOfUrl('http://localhost:5173/', snap)).toBeNull()
  })
})

describe('withMrs', () => {
  it('makes the MRs an agent names their own spans, never in code or a link', () => {
    const spans = withMrs([{ text: 'Merged in !1291 and !1302, see ', bold: true }, { text: '!7', code: true }, { text: 'here', href: 'https://x.org/!8' }, { text: 'nothing' }])
    expect(spans).toEqual([
      { text: 'Merged in ', bold: true }, { text: '!1291', bold: true, mr: [{ ref: { iid: 1291, repo: null } }] }, { text: ' and ', bold: true }, { text: '!1302', bold: true, mr: [{ ref: { iid: 1302, repo: null } }] }, { text: ', see ', bold: true },
      { text: '!7', code: true }, { text: 'here', href: 'https://x.org/!8' }, { text: 'nothing' },
    ])
    expect(withMrs([{ text: 'Hello!123' }])).toEqual([{ text: 'Hello!123' }])
  })
})

describe("an agent's `!12` in the repositories it works in", () => {
  const repo = (name: string, more: object = {}) => ({ name, primary: false, forge: null, mr_web: `https://gitlab.example.org/shop/${name}/-/merge_requests/`, ...more })
  const agent = (id: string, cwd: string, also: string[] = []) => ({ id, cwd, worktree: null, also })
  const s = {
    ...snap,
    repositories: [
      repo('app', { primary: true }),
      repo('helm'),
      repo('site', { forge: 'github', mr_web: 'https://github.com/me/site/pull/' }),
      repo('infra', { mr_web: undefined }),
    ],
    repos: [{ name: 'app', path: '/c/app' }, { name: 'helm', path: '/c/helm' }, { name: 'site', path: '/c/site' }, { name: 'infra', path: '/c/infra' }],
    worktrees: [],
    sessions: [{ id: 'd1', folder: 'charts', worktree: null, repo: 'helm' }, { id: 'd2', folder: 'x', worktree: null }],
    agents: [agent('a-app', '/c/app'), agent('a-helm', '/c/helm.worktrees/fix'), agent('a-site', '/c/site'), agent('a-infra', '/c/infra'), agent('a-out', '/tmp/x'), agent('a-two', '/c/helm', ['/c/app.worktrees/fix']), agent('a-gh', '/c/site', ['/c/helm'])],
  } as unknown as Snapshot
  const scope = (id: string) => mrScopeOf(id, s)
  const refs = (line: string, id: string) => refsIn(line, scope(id)).map((r) => [r.text, r.ref])
  const at = (name: string, iid: number) => `https://gitlab.example.org/shop/${name}/-/merge_requests/${iid}`

  it('in a folder of no repository, the primary one’s, as before', () => {
    expect(scope('a-out')).toBeNull()
    expect(scope('d2')).toBeNull()
    expect(refsIn('see !1288').map((r) => r.ref)).toEqual([{ mr: { iid: 1288, repo: null } }])
  })

  it('in the primary repository, its MR: the panel when Prism knows it, else the forge', () => {
    expect(refs('see !1288', 'a-app')).toEqual([['!1288', { mrs: [{ ref: { iid: 1288, repo: null }, url: at('app', 1288) }] }]])
    expect(knownMr({ ref: { iid: 1288, repo: null } }, s)?.iid).toBe(1288)
    expect(knownMr({ ref: { iid: 9, repo: null } }, s)).toBeUndefined()
  })

  it("another repository's, from an agent or a Claude Desktop session", () => {
    expect(refs('see !214 and !215', 'a-helm')).toEqual([
      ['!214', { mrs: [{ ref: { iid: 214, repo: 'helm' }, url: at('helm', 214) }] }],
      ['!215', { mrs: [{ ref: { iid: 215, repo: 'helm' }, url: at('helm', 215) }] }],
    ])
    expect(knownMr({ ref: { iid: 214, repo: 'helm' } }, s)?.iid).toBe(214)
    expect(refs('!9', 'd1')).toEqual([['!9', { mrs: [{ ref: { iid: 9, repo: 'helm' }, url: at('helm', 9) }] }]])
  })

  it('a repository whose MRs Prism does not read: no link, never the primary one', () => {
    expect(scope('a-infra')).toEqual([])
    expect(refs('fixed in !1288, SHOP-433', 'a-infra')).toEqual([['SHOP-433', { ticket: 'SHOP-433' }]])
  })

  it('a repository on GitHub writes `#12`, even in a GitLab profile', () => {
    expect(refs('see #31 and !31', 'a-site')).toEqual([['#31', { mrs: [{ ref: { iid: 31, repo: 'site', forge: 'github' }, url: 'https://github.com/me/site/pull/31' }] }]])
    expect(refs('#4 !214', 'a-gh').map(([t]) => t)).toEqual(['#4', '!214'])
  })

  it('several repositories: each, even when Prism knows the MR in one only', () => {
    expect(refs('!214', 'a-two')).toEqual([['!214', { mrs: [{ ref: { iid: 214, repo: 'helm' }, url: at('helm', 214) }, { ref: { iid: 214, repo: null }, url: at('app', 214) }] }]])
  })

  it('the same scope while it stays the same: the Chat is not redrawn for nothing', () => {
    expect(scope('a-two')).toBe(scope('a-two'))
  })

  it('in the Chat, as spans of their own', () => {
    expect(withMrs([{ text: 'in !215' }], scope('a-helm'))).toEqual([{ text: 'in ' }, { text: '!215', mr: [{ ref: { iid: 215, repo: 'helm' }, url: at('helm', 215) }] }])
  })
})
