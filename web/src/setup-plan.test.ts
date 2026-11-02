import { describe, expect, it } from 'vitest'
import { pickAgents, pickForge, pickRepos, pickTracker, planPages, repoRows, workspaceDirs, type SetupChoices } from './setup-plan'
import type { SetupDetection } from './bindings/SetupDetection'
import type { RepoFound } from './bindings/RepoFound'

const repo = (name: string, forge: 'gitlab' | 'github' | null, touched = 0, root = '~/Development'): RepoFound => ({
  name, path: `${root}/${name}`, root, project: forge ? `team/${name}` : null, host: forge ? `${forge}.com` : null, forge, touched,
})

const detection = (over: Partial<SetupDetection> = {}): SetupDetection => ({
  forges: [
    { provider: 'gitlab', installed: true, host: 'gitlab.com', login: null },
    { provider: 'github', installed: true, host: 'github.com', login: null },
  ],
  agents: [
    { provider: 'claude', path: '~/.local/bin/claude', version: '2.1.0' },
    { provider: 'command-code', path: null, version: null },
    { provider: 'opencode', path: null, version: null },
    { provider: 'codex', path: '/opt/homebrew/bin/codex', version: '0.162.1' },
  ],
  tmux: 'tmux 3.5a',
  folders: ['~/Development'],
  repos: [repo('api', 'gitlab', 3), repo('blog', 'github', 2), repo('web', 'gitlab', 1)],
  kubectl: false,
  kube_contexts: [],
  trackers: { clickup_env: false, jira_env: false, obsidian_vaults: [] },
  ...over,
})

const all: SetupChoices = { forge: 'gitlab', signedIn: true, agents: ['claude'], tmux: true, repos: 2, tracker: 'clickup', calendar: true, kube: true }

describe('the first page', () => {
  const views = [
    { name: 'Dashboard', columns: ['todo sessions notes', 'mrs', 'sprint agenda'] },
    { name: 'Tickets', columns: ['refinement', 'pickup'] },
    { name: 'Infra', columns: ['pods', '---', 'envs argocd'] },
  ]
  it('keeps every block whose source is there', () => {
    expect(planPages(views, all).pages).toEqual(views)
  })
  it('leaves out the blocks whose source is missing, and the pages they emptied', () => {
    const { pages, blocks } = planPages(views, { ...all, tracker: 'none', kube: false, signedIn: false })
    expect(pages).toEqual([{ name: 'Dashboard', columns: ['todo sessions notes', 'agenda'] }])
    expect(blocks.filter((b) => b.missing).map((b) => [b.id, b.missing])).toEqual([
      ['mrs', 'not-signed-in'], ['sprint', 'no-tracker'], ['refinement', 'no-tracker'], ['pickup', 'no-tracker'], ['pods', 'no-kube'], ['envs', 'no-kube'], ['argocd', 'no-kube'],
    ])
  })
  it('drops a band left empty, never the page’s first nor last line', () => {
    expect(planPages([{ name: 'X', columns: ['pods', '---', 'notes', '---', 'envs'] }], { ...all, kube: false }).pages).toEqual([{ name: 'X', columns: ['notes'] }])
  })
  it('says why the agents are out', () => {
    expect(planPages([{ name: 'A', columns: ['sessions'] }], { ...all, agents: [] }).blocks[0].missing).toBe('no-agent')
    expect(planPages([{ name: 'A', columns: ['sessions'] }], { ...all, tmux: false }).blocks[0].missing).toBe('no-tmux')
    expect(planPages([{ name: 'A', columns: ['worktrees'] }], { ...all, repos: 0 }).blocks[0].missing).toBe('no-repo')
  })
})

describe('pre-filled from the machine', () => {
  it('starts from the forge signed in, the one most repositories are on when both are', () => {
    expect(pickForge(detection())).toBe('gitlab')
    expect(pickForge(detection({ forges: [{ provider: 'gitlab', installed: true, host: 'gitlab.com', login: null }, { provider: 'github', installed: true, host: 'github.com', login: 'me' }] }))).toBe('github')
    expect(pickForge(detection({ forges: [{ provider: 'gitlab', installed: false, host: 'gitlab.com', login: null }, { provider: 'github', installed: false, host: 'github.com', login: null }] }))).toBe('none')
    expect(pickForge(detection({ repos: [] }))).toBe('none')
  })
  it('ticks the agent CLIs found, Claude Code first', () => {
    expect(pickAgents(detection())).toEqual(['claude', 'codex'])
  })
  it('ticks the repositories of the forge, but those already there', () => {
    expect(pickRepos(detection(), 'gitlab')).toEqual(['~/Development/api', '~/Development/web'])
    expect(pickRepos(detection(), 'gitlab', ['~/Development/api'])).toEqual(['~/Development/web'])
    expect(pickRepos(detection(), 'none')).toHaveLength(3)
  })
  it('picks the tracker within reach', () => {
    expect(pickTracker(detection())).toBe('none')
    expect(pickTracker(detection({ trackers: { clickup_env: false, jira_env: false, obsidian_vaults: ['~/Notes'] } }))).toBe('obsidian')
    expect(pickTracker(detection({ trackers: { clickup_env: true, jira_env: true, obsidian_vaults: [] } }))).toBe('clickup')
  })
})

describe('what the wizard writes', () => {
  it('names the repositories after their folder, once each', () => {
    const rows = repoRows([repo('api', 'gitlab'), repo('blog', 'github'), repo('Api', null, 0, '~/src')], 'gitlab', ['web'])
    expect(rows).toEqual([
      { name: 'api', path: '~/Development/api', project: 'team/api', merge_requests: true, worktrees: true },
      { name: 'blog', path: '~/Development/blog', project: 'team/blog', merge_requests: true, worktrees: true, forge: 'github' },
      { name: 'api-2', path: '~/src/Api', worktrees: true },
    ])
  })
  it('reads no MR without a forge', () => {
    expect(repoRows([repo('api', 'gitlab')], 'none', [])[0].merge_requests).toBe(false)
  })
  it('adds the folders holding them as working folders', () => {
    expect(workspaceDirs([repo('api', 'gitlab'), repo('web', 'gitlab'), repo('x', null, 0, '~/src')])).toEqual(['~/Development', '~/src'])
  })
})
