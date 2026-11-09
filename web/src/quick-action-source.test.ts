import { describe, expect, it } from 'vitest'
import { templated, uniqueName } from './quick-action-source'

describe('quick action from a session', () => {
  it('writes the repository and its base as variables, whole words only', () => {
    expect(templated('Update the main branch of prism-pack, then merge origin/main into `main`.', 'prism-pack', 'main')).toBe('Update the {base} branch of {repo}, then merge origin/{base} into `{base}`.')
    expect(templated('Mets à jour la branche main de shop', 'shop', 'main')).toBe('Mets à jour la branche {base} de {repo}')
    expect(templated('refactor the main function of app.py in app', 'app', 'main')).toBe('refactor the main function of app.py in {repo}')
    expect(templated('the domain of prism-packer', 'prism-pack', 'main')).toBe('the domain of prism-packer')
    expect(templated('already {repo} on {base}', 'repo', 'base')).toBe('already {repo} on {base}')
    expect(templated('no repo', '', '')).toBe('no repo')
  })

  it('names it apart from the quick actions there are', () => {
    expect(uniqueName('Upstream + Trivy', [])).toBe('Upstream + Trivy')
    expect(uniqueName('Upstream + Trivy', ['Upstream + Trivy', 'Upstream + Trivy 2'])).toBe('Upstream + Trivy 3')
    expect(uniqueName('  ', [])).toBe('Quick action')
  })
})
