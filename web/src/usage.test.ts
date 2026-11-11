import { describe, expect, it } from 'vitest'
import { outdated } from './usage'
import type { ClaudeUsage } from './types'

const at = (iso: string): ClaudeUsage => ({ five_hour: null, seven_day: null, others: [], fetched_at: iso })

describe('outdated', () => {
  const now = new Date(2026, 10, 11, 15, 0).getTime()
  it('is silent for a fresh reading', () => {
    expect(outdated(at(new Date(2026, 10, 11, 14, 40).toISOString()), now)).toBeNull()
    expect(outdated(null, now)).toBeNull()
  })
  it("names today's time, another day's with its day", () => {
    expect(outdated(at(new Date(2026, 10, 11, 14, 13).toISOString()), now)).toMatch(/(14|02).13/)
    const before = outdated(at(new Date(2026, 10, 9, 14, 13).toISOString()), now)
    expect(before).toMatch(/(14|02).13/)
    expect(before).toMatch(/9/)
  })
})
