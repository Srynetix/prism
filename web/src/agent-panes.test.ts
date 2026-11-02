import { describe, expect, it } from 'vitest'
import { followStep } from './agent-panes.svelte'

describe('followStep', () => {
  it('brings who waits into an empty pane, not the focused one', () => {
    expect(followStep(['a', null], new Set(['w']), 0, [], [])).toEqual({ next: ['a', 'w'], followed: ['w'], dismissed: [] })
  })

  it('empties the pane of one it brought once answered, unless focused', () => {
    expect(followStep(['a', 'w'], new Set(), 0, ['w'], []).next).toEqual(['a', null])
    expect(followStep(['a', 'w'], new Set(), 1, ['w'], []).next).toEqual(['a', 'w'])
  })

  it('never brings back one taken out by hand while it still waits', () => {
    expect(followStep(['a', null], new Set(['w']), 0, [], ['w']).next).toEqual(['a', null])
    expect(followStep(['a', null], new Set(), 0, [], ['w']).dismissed).toEqual([])
  })

  it('leaves the last pane to one that waits too', () => {
    expect(followStep(['a', 'v'], new Set(['v', 'w']), 0, [], []).next).toEqual(['a', 'v'])
  })
})
