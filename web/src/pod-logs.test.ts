import { describe, expect, it, vi } from 'vitest'

vi.mock('./bridge', () => ({ hasCore: false, isDesktop: () => false, invoke: vi.fn() }))

const { join, joinAt } = await import('./pod-logs.svelte')

describe('join', () => {
  it('adds the new end of a tail read again', () => {
    expect(join('a\nb\nc\nd\n', 'c\nd\ne\n')).toBe('a\nb\nc\nd\ne\n')
  })
  it('keeps the log when nothing came', () => {
    expect(join('a\nb\nc\n', 'b\nc\n')).toBe('a\nb\nc\n')
  })
  it('gives up when the tail starts nowhere (the container restarted)', () => {
    expect(join('a\nb\nc\n', 'x\ny\n')).toBeNull()
  })
  it('joins as when the whole log was split, reading only its end', () => {
    // The former join, every line split: the reference.
    const whole = (log: string, tail: string) => {
      const old = log.split('\n')
      const next = tail.split('\n')
      if (old.at(-1) === '') old.pop()
      if (next.at(-1) === '') next.pop()
      if (!next.length) return log
      for (let k = Math.max(0, old.length - next.length); k < old.length; k++) {
        let same = true
        for (let j = 0; k + j < old.length && same; j++) same = old[k + j] === next[j]
        if (same) return [...old.slice(0, k), ...next].join('\n') + (tail.endsWith('\n') ? '\n' : '')
      }
      return null
    }
    let seed = 7
    const rnd = (n: number) => (seed = (seed * 1103515245 + 12345) % 2 ** 31) % n
    for (let run = 0; run < 400; run++) {
      const lines = Array.from({ length: 1 + rnd(30) }, () => 'x'.repeat(rnd(4)) + 'ab'[rnd(2)])
      const log = lines.join('\n') + (rnd(2) ? '\n' : '')
      const from = rnd(lines.length + 2)
      const tail = [...lines.slice(from), ...Array.from({ length: rnd(4) }, () => 'n' + rnd(3))].join('\n') + (rnd(2) ? '\n' : '')
      expect(join(log, tail), JSON.stringify({ log, tail })).toBe(whole(log, tail))
      const at = joinAt(log, tail)
      if (at) expect(at.log.slice(0, at.same)).toBe(log.slice(0, at.same))
    }
  })
})
