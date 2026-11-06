import { describe, expect, it } from 'vitest'
import { filesOfAgent, insertAt, noteAgentFiles } from './dictation.svelte'
import type { AgentConversation } from './agents.svelte'

describe('dictated text', () => {
  it('goes in at the cursor, spaced from the words around it', () => {
    expect(insertAt('', 0, 0, 'Hello.')).toEqual({ value: 'Hello.', caret: 6 })
    expect(insertAt('Keep the API', 12, 12, 'but add a cursor')).toEqual({ value: 'Keep the API but add a cursor', caret: 29 })
    expect(insertAt('Keep  the API', 5, 5, 'all of')).toEqual({ value: 'Keep all of the API', caret: 11 })
  })

  it('replaces the selection, and leaves punctuation after it alone', () => {
    expect(insertAt('Fix this bug.', 4, 8, 'that')).toEqual({ value: 'Fix that bug.', caret: 8 })
    expect(insertAt('Run the tests.', 13, 13, 'on main')).toEqual({ value: 'Run the tests on main.', caret: 21 })
  })

  it('starts a line after a line break without a space', () => {
    expect(insertAt('First line\n', 11, 11, 'second')).toEqual({ value: 'First line\nsecond', caret: 17 })
  })
})

describe('words to expect', () => {
  it('are the files the agent read or wrote, newest first, 40 at most', () => {
    const tool = (kind: 'read' | 'write' | 'command', what: string) => ({ type: 'tool' as const, tool: 'x', kind, what, state: 'ok' as const, output: null, diff: [], added: 0, removed: 0, at: null, ms: null })
    const turn = (n: number, entries: ReturnType<typeof tool>[]) => ({ n, prompt: '', at: null, last: null, entries, tokens: 0, cost: null, model: null })
    const many = Array.from({ length: 50 }, (_, i) => tool('read', `src/f${i}.ts`))
    const convo = { turns: [turn(1, many), turn(2, [tool('read', 'src/a/lib.rs'), tool('command', 'cargo test'), tool('write', 'web/App.svelte')])], total: 2, partial: false } as AgentConversation
    noteAgentFiles('a1', convo)
    const files = filesOfAgent('a1')
    expect(files.slice(0, 3)).toEqual(['App.svelte', 'lib.rs', 'f49.ts'])
    expect(files).toHaveLength(40)
    noteAgentFiles('a1', null)
    expect(filesOfAgent('a1')).toEqual([])
  })
})
