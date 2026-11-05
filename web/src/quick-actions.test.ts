import { describe, expect, it } from 'vitest'
import { quickActionsOn, quickLaunch } from './quick-actions'
import type { QuickAction } from './bindings/QuickAction'

const action = (o: Partial<QuickAction> = {}): QuickAction => ({ name: 'Upstream + Trivy', prompt: 'Update {base} of {repo} from upstream, then run Trivy.', repos: [], worktree: false, mode: null, kind: null, ...o })
const api = { name: 'api', path: '/code/api', base: 'develop' }

describe('quickActionsOn', () => {
  it('offers those naming the repository or none, with a name and a prompt', () => {
    const all = [action({ name: 'Everywhere' }), action({ name: 'Api', repos: ['api'] }), action({ name: 'Web', repos: ['web'] }), action({ name: ' ' }), action({ name: 'Empty', prompt: '' })]
    expect(quickActionsOn('api', all).map((a) => a.name)).toEqual(['Everywhere', 'Api'])
  })
})

describe('quickLaunch', () => {
  it('runs in the clone as it is, its prompt filled in', () => {
    expect(quickLaunch(action(), api, true)).toEqual({ title: 'Upstream + Trivy', prompt: 'Update develop of api from upstream, then run Trivy.', from: 'Upstream + Trivy', cwd: '/code/api', permission_mode: 'default' })
  })

  it('takes a new worktree only where the repository makes them', () => {
    expect(quickLaunch(action({ worktree: true }), api, true)).toMatchObject({ worktree: { repo: 'api' } })
    expect(quickLaunch(action({ worktree: true }), api, false)).toMatchObject({ cwd: '/code/api' })
  })

  it('passes its mode and kind, none the CLI’s own: default, not the launcher’s', () => {
    expect(quickLaunch(action({ mode: 'acceptEdits', kind: 'Ops' }), api, false)).toMatchObject({ permission_mode: 'acceptEdits', category: 'Ops' })
    expect(quickLaunch(action({ mode: 'default' }), api, false)).toMatchObject({ permission_mode: 'default' })
    expect(quickLaunch(action(), api, false)).toMatchObject({ permission_mode: 'default' })
  })

  it('needs a clone', () => {
    expect(quickLaunch(action(), undefined, false)).toBeNull()
    expect(quickLaunch(action(), { ...api, path: '' }, false)).toBeNull()
  })
})
