import { describe, expect, it } from 'vitest'
import { LogLines, parse } from './ansi'

const segs = (s: string) => parse(s)[0].segs.map(({ text, fg, color, bold }) => ({ text, ...(fg ? { fg } : {}), ...(color ? { color } : {}), ...(bold ? { bold } : {}) }))

describe('parse', () => {
  it('the 16 colors as classes, bold, resets', () => {
    expect(segs('\x1b[1;31merror\x1b[0m done')).toEqual([{ text: 'error', fg: 'c31', bold: true }, { text: ' done' }])
  })
  it('256 colors (a terminal’s history): the first 16 as classes, the rest as CSS', () => {
    expect(segs('\x1b[38;5;231m●\x1b[39m \x1b[38;5;2mok\x1b[38;5;242m 3s')).toEqual([
      { text: '●', color: 'rgb(255, 255, 255)' },
      { text: ' ' },
      { text: 'ok', fg: 'c32' },
      { text: ' 3s', color: 'rgb(108, 108, 108)' },
    ])
  })
  it('true color, and a background skipped without eating the next codes', () => {
    expect(segs('\x1b[38;2;255;126;219mpink\x1b[48;5;22;1m bold')).toEqual([{ text: 'pink', color: 'rgb(255, 126, 219)' }, { text: ' bold', color: 'rgb(255, 126, 219)', bold: true }])
  })
})

describe('LogLines', () => {
  it('parses as parse does, a line read again being the same object', () => {
    const log = 'one\n\x1b[31mred\nstill red\x1b[0m\nplain\n'
    const ll = new LogLines()
    const first = ll.parse(log)
    expect(first).toEqual(parse(log))
    const grown = ll.parse(log + 'more\n')
    expect(grown).toEqual(parse(log + 'more\n'))
    expect(grown.slice(0, first.length).every((l, i) => l === first[i])).toBe(true)
    expect(grown.map((_, i) => ll.key(i))).toEqual([0, 1, 2, 3, 4])
  })

  it('follows its lines when the log slides, colors carried from the line before', () => {
    const ll = new LogLines()
    const a = ll.parse('a\nb\nc\nd\n')
    const b = ll.parse('c\nd\ne\n')
    expect(b[0]).toBe(a[2])
    expect([ll.key(0), ll.key(1), ll.key(2)]).toEqual([2, 3, 4])
    // The same text after other colors is another line.
    const colored = ll.parse('\x1b[32mc\nd\n')
    expect(colored[1].segs[0].fg).toBe('c32')
    expect(colored[1]).not.toBe(b[1])
  })

  it('reads only what a growing log added, as a whole parse would, colors and a line half written included', () => {
    const ll = new LogLines()
    let log = 'start\n\x1b[33mwarm'
    const seen = [ll.parse(log)]
    for (const more of ['ing\x1b[0m up\n', 'a\n\x1b[31mred', ' still\nend red\x1b[0m\n\n', '', 'plain\n', 'half']) {
      const from = log
      log += more
      const lines = ll.parse(log, { from, same: from.length })
      expect(lines).toEqual(parse(log))
      seen.push(lines)
    }
    // A line kept is the same object, its key the same.
    expect(seen.at(-1)![0]).toBe(seen[1][0])
    expect(seen.at(-1)!.map((_, i) => ll.key(i))).toEqual(seen.at(-1)!.map((_, i) => i))
  })

  it('reads it all again when what it is said to come from is not what it read', () => {
    const ll = new LogLines()
    ll.parse('a\nb\n')
    expect(ll.parse('x\ny\nz\n', { from: 'other\n', same: 6 })).toEqual(parse('x\ny\nz\n'))
    expect(ll.parse('x\nQ\nz\nw\n', { from: 'x\ny\nz\n', same: 2 })).toEqual(parse('x\nQ\nz\nw\n'))
  })
})
