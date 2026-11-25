import { describe, expect, it, vi } from 'vitest'

vi.mock('./bridge', () => ({ invoke: vi.fn(() => Promise.resolve(null)), isDesktop: () => false }))
vi.mock('./live.svelte', () => ({
  live: {
    snapshot: {
      stages: [{ role: 'production', name: 'production', env: 'clients' }],
      environments: { envs: [{ context: 'cluster-a', env: 'prod' }, { context: 'gke_staging', env: 'staging' }, { context: 'cluster-b', env: 'prd' }, { context: 'cluster-c', env: 'clients' }] },
    },
  },
}))

const { crumbs, downloadName, isProdContext, joinPath, parentPath, replacedBy, containerFiles, startEdit, saveEdit, editDirty, compareConflict, overwriteConflict, takeTheirs } = await import('./container-files.svelte')
const { invoke } = await import('./bridge')
const mocked = vi.mocked(invoke)

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
    expect(isProdContext('cluster-a')).toBe(true)
    expect(isProdContext('cluster-b')).toBe(true)
    // The production stage's environment, whatever its name.
    expect(isProdContext('cluster-c')).toBe(true)
    // A Pods block's context no environment names, prod by its own name.
    expect(isProdContext('gke_acme-production_europe-west9-a_prod')).toBe(true)
    expect(isProdContext('gke_staging')).toBe(false)
    expect(isProdContext('kind-local')).toBe(false)
    expect(isProdContext('products-dev')).toBe(false)
  })
})

describe('editing a container\'s file', () => {
  const target = { kind: 'docker' as const, id: 'abc' }
  function opened(prod = false) {
    Object.assign(containerFiles, { open: true, target, write: true, prod, edit: null, note: null, file: { path: '/app/a.conf', text: { text: 'one\n', binary: false, truncated: false, hash: 'h1' }, loading: false, error: null } })
    startEdit()
    containerFiles.edit!.draft = 'two\n'
  }

  it('saves, checking the file read is still there', async () => {
    opened()
    expect(editDirty()).toBe(true)
    mocked.mockResolvedValueOnce({ kind: 'saved', hash: 'h2' })
    await saveEdit()
    expect(mocked).toHaveBeenLastCalledWith('container_file_save', { target, path: '/app/a.conf', text: 'two\n', expected: 'h1' })
    expect(containerFiles.edit).toBe(null)
    expect(containerFiles.file?.text).toEqual({ text: 'two\n', binary: false, truncated: false, hash: 'h2' })
  })

  it('shows production\'s changes first, then saves', async () => {
    opened(true)
    mocked.mockClear()
    await saveEdit()
    expect(mocked).not.toHaveBeenCalled()
    expect(containerFiles.edit?.review).toBe(true)
    mocked.mockResolvedValueOnce({ kind: 'saved', hash: 'h2' })
    await saveEdit()
    expect(mocked).toHaveBeenCalledTimes(1)
  })

  it('stops at a file changed meanwhile: compared, taken, or overwritten', async () => {
    opened()
    mocked.mockResolvedValueOnce({ kind: 'changed', text: 'theirs\n', hash: 'h3' })
    await saveEdit()
    expect(containerFiles.edit?.conflict).toEqual({ text: 'theirs\n', hash: 'h3' })
    // Saving again does nothing until the conflict is settled.
    mocked.mockClear()
    await saveEdit()
    expect(mocked).not.toHaveBeenCalled()

    compareConflict()
    expect(containerFiles.edit).toMatchObject({ original: 'theirs\n', hash: 'h3', draft: 'two\n', comparing: true, conflict: null })

    containerFiles.edit!.conflict = { text: 'again\n', hash: 'h4' }
    takeTheirs()
    expect(containerFiles.edit).toMatchObject({ original: 'again\n', draft: 'again\n', hash: 'h4', comparing: false })
    expect(editDirty()).toBe(false)

    containerFiles.edit!.draft = 'mine\n'
    containerFiles.edit!.conflict = { text: 'later\n', hash: 'h5' }
    mocked.mockResolvedValueOnce({ kind: 'saved', hash: 'h6' })
    overwriteConflict()
    await vi.waitFor(() => expect(containerFiles.edit).toBe(null))
    // Still checked, against the version seen: a third change would stop it.
    expect(mocked).toHaveBeenLastCalledWith('container_file_save', { target, path: '/app/a.conf', text: 'mine\n', expected: 'h5' })
  })

  it('is not offered for a part of a file', () => {
    Object.assign(containerFiles, { write: true, edit: null, file: { path: '/big.log', text: { text: 'x', binary: false, truncated: true, hash: null }, loading: false, error: null } })
    startEdit()
    expect(containerFiles.edit).toBe(null)
  })
})
