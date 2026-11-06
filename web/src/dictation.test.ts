import { describe, expect, it } from 'vitest'
import { insertAt } from './dictation.svelte'

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
