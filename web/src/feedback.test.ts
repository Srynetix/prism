import { describe, expect, it } from 'vitest'
import { names } from './feedback.svelte'

describe('a release naming a request', () => {
  it('finds #n, not a longer number nor an entity', () => {
    expect(names('feat: split the Agents view (#214)', 214)).toBe(true)
    expect(names('#214 at the start', 214)).toBe(true)
    expect(names('closes #2140', 214)).toBe(false)
    expect(names('&#214; is not an issue', 214)).toBe(false)
    expect(names('', 214)).toBe(false)
  })
})
