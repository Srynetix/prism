import { describe, expect, it } from 'vitest'
import { cleanLines, cleanText, isFrame } from './tty-clean'
import { parse } from '../ansi'

describe('tty-clean', () => {
  it('drops what the CLI draws, keeps its words', () => {
    const screen = [
      '╭──────────────────────────╮',
      '│ > add the CSV export     │',
      '╰──────────────────────────╯',
      '',
      '',
      '⏺ Looking first.',
      '✻ Whirring… (34s · ↓ 1.2k tokens · esc to interrupt)',
      '✻ Whirring… (35s · ↓ 1.3k tokens · esc to interrupt)',
      '───────────────────────────',
      '  ⏵⏵ accept edits on (shift+tab to cycle)        Context left: 61%',
    ]
    expect(cleanText(screen)).toEqual(['> add the CSV export', '', '⏺ Looking first.'])
  })

  it('keeps a shell’s lines, a dash in a word or a table’s pipe included', () => {
    expect(isFrame('---')).toBe(false)
    expect(cleanText(['$ ls -la', 'a | b', 'a', 'a'])).toEqual(['$ ls -la', 'a | b', 'a'])
  })

  it('keeps the colours of what it keeps', () => {
    const lines = cleanLines(parse('\x1b[32m│\x1b[0m \x1b[1mbold\x1b[0m text \x1b[32m│\x1b[0m\n\x1b[2m────\x1b[0m\n\x1b[31mred\x1b[0m'))
    expect(lines.map((l) => l.segs.map((s) => s.text).join(''))).toEqual(['bold text', 'red'])
    expect(lines[0].segs[0]).toMatchObject({ text: 'bold', bold: true })
  })
})
