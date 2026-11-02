import { describe, expect, it } from 'vitest'
import { fuzzy } from './fuzzy'

describe('fuzzy', () => {
  it('finds the characters in order, words in any order', () => {
    expect(fuzzy('ACME-4688 Export CSV', '4688')).not.toBeNull()
    expect(fuzzy('Merge requests', 'mrq')).not.toBeNull()
    expect(fuzzy('Merge requests', 'requests merge')).not.toBeNull()
    expect(fuzzy('Merge requests', 'xyz')).toBeNull()
    expect(fuzzy('anything', '')).toEqual({ score: 0, at: [] })
  })

  it('ranks a contiguous match and a word start higher, then the shorter text', () => {
    const score = (t: string, q: string) => fuzzy(t, q)!.score
    expect(score('Rebase onto main', 'reb')).toBeGreaterThan(score('Retry the build', 'reb'))
    expect(score('CI', 'ci')).toBeGreaterThan(score('CI health', 'ci'))
    expect(fuzzy('CI health', 'ch')!.at).toEqual([0, 3])
  })
})
