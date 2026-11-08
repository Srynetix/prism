import { describe, expect, it } from 'vitest'
import { nameOf, newStep, newWorktree, tasksFor, testTask, titleOf } from './launch-form.svelte'

describe('launch form', () => {
  it('titles a launch from its prompt’s first line, unless typed', () => {
    expect(titleOf('', '  Fix the scroll\nof the list')).toBe('Fix the scroll')
    expect(titleOf(' Mine ', 'Fix')).toBe('Mine')
    expect(titleOf('', 'x'.repeat(80))).toHaveLength(60)
  })

  it('names a worktree from the prompt’s first four words, mentions left out', () => {
    expect(nameOf('Corrige @src/a.ts le scroll de la liste')).toBe('corrige-le-scroll-de')
  })

  it('asks for a new worktree: typed name, else fallback, unless a branch names it', () => {
    expect(newWorktree({ name: 'My Name', fallback: 'fb', branch: '', base: ' dev ', repo: 'shop' })).toEqual({ name: 'my-name', branch: undefined, repo: 'shop', base: 'dev' })
    expect(newWorktree({ name: '', fallback: 'fb', branch: 'feat/x', base: 'dev', repo: '' })).toEqual({ name: undefined, branch: 'feat/x', repo: undefined, base: undefined })
    expect(newWorktree({ name: '', fallback: 'fb', branch: '', base: '', repo: 'shop', also: ['api'] })).toEqual({ name: 'fb', branch: undefined, repo: 'shop', base: undefined, also: ['api'] })
  })

  it('adds Then steps as both forms do', () => {
    expect(newStep('task', 'just test')).toEqual({ kind: 'task', when: 'done', command: 'just test' })
    expect(newStep('mr')).toEqual({ kind: 'mr', when: 'ok' })
    expect(newStep('agent')).toEqual({ kind: 'agent', when: 'ask', text: 'Review the code of MR !{mr}.' })
  })

  it('picks the test task, failing that the first; tells which place tasks were read for', () => {
    expect(testTask(['build', 'unit test', 'lint'])).toBe('unit test')
    expect(testTask(['build'])).toBe('build')
    expect(testTask([])).toBe('')
    expect(tasksFor({ for: JSON.stringify({ repo: 'shop' }) }, { repo: 'shop' })).toBe(true)
    expect(tasksFor({ for: JSON.stringify({ repo: 'shop' }) }, { cwd: '/x' })).toBe(false)
  })
})
