import { describe, expect, it } from 'vitest'

/**
 * In a quoted attribute (`title="… {x}\n …"`), what is outside `{}` is HTML text: `\n` stays a
 * backslash and an n, shown as is in the tooltip. A line break goes inside an expression
 * (`title={`${x}\n…`}`).
 */
function literalEscapes(src: string): string[] {
  const markup = src.replace(/<(script|style)\b[\s\S]*?<\/\1>/g, (m) => '\n'.repeat(m.split('\n').length - 1))
  const found: string[] = []
  for (const m of markup.matchAll(/\s([\w:-]+)="/g)) {
    let depth = 0
    let text = ''
    for (let i = m.index! + m[0].length; i < markup.length; i++) {
      const c = markup[i]
      if (depth === 0 && c === '"') break
      if (c === '{') depth++
      else if (c === '}') depth--
      else if (depth === 0) text += c
    }
    if (/\\[nt]/.test(text)) found.push(`line ${markup.slice(0, m.index).split('\n').length}: ${m[1]}="${text.trim().slice(0, 60)}"`)
  }
  return found
}

/** Every component of the window, as text. */
const SOURCES = import.meta.glob<string>('./**/*.svelte', { query: '?raw', import: 'default', eager: true })

describe('attributes', () => {
  it('finds a literal \\n outside an expression', () => {
    expect(literalEscapes('<a title="{x}\\n{y}: go">')).toHaveLength(1)
    expect(literalEscapes('<a title={`${x}\\n${y}`} data-x="{a ? `\\n${b}` : \'\'}">')).toEqual([])
  })

  it('has none in the components', () => {
    const bad = Object.entries(SOURCES).flatMap(([file, src]) => literalEscapes(src).map((l) => `${file} ${l}`))
    expect(Object.keys(SOURCES).length).toBeGreaterThan(50)
    expect(bad).toEqual([])
  })
})
