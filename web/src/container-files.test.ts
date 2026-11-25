import { describe, expect, it, vi } from 'vitest'

vi.mock('./bridge', () => ({ invoke: vi.fn(() => Promise.resolve(null)), isDesktop: () => false }))
vi.mock('./live.svelte', () => ({ live: { snapshot: { environments: { envs: [{ context: 'gke_prod', env: 'prod' }, { context: 'gke_staging', env: 'staging' }] } } } }))

const { crumbs, downloadName, isProdContext, joinPath, parentPath, replacedBy } = await import('./container-files.svelte')

describe('container paths', () => {
  it('goes up, joins, and names each step of a path', () => {
    expect(parentPath('/var/log/')).toBe('/var')
    expect(parentPath('/var')).toBe('/')
    expect(parentPath('/')).toBe('/')
    expect(joinPath('/', 'etc')).toBe('/etc')
    expect(joinPath('/app/', 'main.py')).toBe('/app/main.py')
    expect(crumbs('/var/log')).toEqual([{ name: '/', path: '/' }, { name: 'var', path: '/var' }, { name: 'log', path: '/var/log' }])
  })

  it('saves a folder as a .tar.gz, a file under its own name', () => {
    expect(downloadName('/app/config.yml', false)).toBe('config.yml')
    expect(downloadName('/var/log/', true)).toBe('log.tar.gz')
    expect(downloadName('/', true)).toBe('root.tar.gz')
  })

  it('says which files a send replaces', () => {
    const listing = { path: '/app', truncated: false, entries: [{ name: 'a.txt', dir: false, link: null, size: 1, modified: null }, { name: 'src', dir: true, link: null, size: null, modified: null }] }
    expect(replacedBy(['a.txt', 'b.txt', 'src'], listing)).toEqual(['a.txt'])
    expect(replacedBy(['a.txt'], null)).toEqual([])
  })

  it('knows a production context', () => {
    expect(isProdContext('gke_prod')).toBe(true)
    expect(isProdContext('gke_staging')).toBe(false)
    expect(isProdContext('kind-local')).toBe(false)
  })
})
