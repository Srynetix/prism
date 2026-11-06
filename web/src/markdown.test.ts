import { describe, expect, it } from 'vitest'
import { inline, parse, withMentions, type Block } from './markdown'

const lists = (md: string) => parse(md).filter((b): b is Extract<Block, { kind: 'list' }> => b.kind === 'list')
const numbers = (md: string) => lists(md).map((l) => l.items.map((i) => i.number ?? '•'))
const text = (md: string) => lists(md).map((l) => l.items.map((i) => i.spans.map((s) => s.text).join('')))

describe('numbered lists', () => {
  it('keeps a loose list whole, as agents write them: one item, a blank line, the next', () => {
    expect(numbers('1. **First** — a\n\n2. **Second** — b\n\n3. Third')).toEqual([[1, 2, 3]])
  })

  it('counts on `1.` `1.` `1.`, and keeps the author’s numbers otherwise', () => {
    expect(numbers('1. a\n1. b\n1. c')).toEqual([[1, 2, 3]])
    expect(numbers('3. c\n4. d')).toEqual([[3, 4]])
  })

  it('keeps a numbered item’s indented paragraph in it, and its nested bullets as bullets', () => {
    const md = '1. Install\n\n   Run it once.\n   - with pnpm\n2. Start'
    expect(numbers(md)).toEqual([[1, '•', 2]])
    expect(text(md)[0][0]).toBe('Install Run it once.')
  })

  it('ends at a paragraph after a blank line, and a numbered list after it starts at its own number', () => {
    expect(parse('1. a\n\nThen:\n\n2. b').map((b) => b.kind)).toEqual(['list', 'paragraph', 'list'])
    expect(numbers('1. a\n\nThen:\n\n2. b')).toEqual([[1], [2]])
  })

  it('separates a bulleted list from a numbered one', () => {
    expect(numbers('- a\n- b\n\n1. c')).toEqual([['•', '•'], [1]])
  })
})

describe('code blocks', () => {
  const kinds = (md: string) => parse(md).map((b) => b.kind)

  it('keeps a code block indented under a list item in it, whole, blank lines and all', () => {
    const md = '1. Install:\n\n   ```bash\n   pnpm i\n\n   - not an item\n   ```\n\n   Then check.\n2. Run.\n\nAfter.'
    expect(kinds(md)).toEqual(['list', 'paragraph'])
    const [item, next] = lists(md)[0].items
    expect(item.spans.map((s) => s.text).join('')).toBe('Install:')
    expect(item.blocks?.map((b) => b.kind)).toEqual(['code', 'paragraph'])
    expect(item.blocks?.[0]).toMatchObject({ kind: 'code', text: 'pnpm i\n\n- not an item' })
    expect(next.number).toBe(2)
  })

  it('closes a fence only on as many of the same character, alone on its line', () => {
    expect(parse('````md\n```js\nx\n```\n````\nAfter')).toMatchObject([{ kind: 'code', text: '```js\nx\n```' }, { kind: 'paragraph' }])
    expect(parse('~~~\n```\n~~~')).toMatchObject([{ kind: 'code', text: '```' }])
  })

  it('opens no block on backticks closed on the same line', () => {
    expect(kinds('Use ```x``` here\n```foo```\nmore')).toEqual(['paragraph'])
  })
})

describe('code blocks in quotes', () => {
  it('reads a quote holding a code block as blocks, the code whole', () => {
    const [q] = parse('> Run:\n>\n> ```sh\n> ls\n>\n> - not an item\n> ```\n>\n> Done.')
    expect(q).toMatchObject({ kind: 'quote', spans: [] })
    const blocks = q.kind === 'quote' ? q.blocks : undefined
    expect(blocks?.map((b) => b.kind)).toEqual(['paragraph', 'code', 'paragraph'])
    expect(blocks?.[1]).toMatchObject({ text: 'ls\n\n- not an item' })
  })

  it('does the same in a callout, and leaves a quote without code as one run of text', () => {
    expect(parse('> [!TIP]\n> ```\n> x\n> ```')).toMatchObject([{ kind: 'callout', tone: 'tip', blocks: [{ kind: 'code', text: 'x' }] }])
    const [q] = parse('> a\n> - b')
    expect(q.kind === 'quote' && q.blocks).toBeUndefined()
  })
})

describe('tables', () => {
  const rows = (md: string) =>
    parse(md)
      .filter((b): b is Extract<Block, { kind: 'table' }> => b.kind === 'table')
      .map((t) => t.rows.map((r) => r.map((c) => c.map((s) => s.text).join(''))))

  it('keeps an escaped pipe in its cell', () => {
    expect(rows('| a | b |\n|---|---|\n| x \\| y | z |')).toEqual([[['a', 'b'], ['x | y', 'z']]])
  })

  it('keeps a pipe in code in its cell, escaped or not', () => {
    expect(rows('| a | b |\n|---|---|\n| `x | y` | `p \\| q` |')).toEqual([[['a', 'b'], ['x | y', 'p | q']]])
  })

  it('cuts on a pipe after a lone backtick', () => {
    expect(rows('| a | b |\n|---|---|\n| ` x | y |')).toEqual([[['a', 'b'], ['` x', 'y']]])
  })
})

describe('inline code', () => {
  const code = (md: string) => inline(md).filter((s) => s.code).map((s) => s.text)

  it('reads code between runs of as many backticks, the backticks inside it kept', () => {
    expect(code('Use ``a`b`` and `c`')).toEqual(['a`b', 'c'])
    expect(code('A fence: ```` ``` ````')).toEqual(['```'])
    expect(code('Wrap it: `` `x` ``')).toEqual(['`x`'])
  })

  it('leaves a run without its match as text', () => {
    expect(code('``a` b')).toEqual([])
    expect(inline('``a` b').map((s) => s.text).join('')).toBe('``a` b')
  })
})

describe('mentions', () => {
  const chips = (text: string, marks: Parameters<typeof withMentions>[1]) => withMentions(inline(text), marks).filter((s) => s.mention).map((s) => `${s.text}:${s.mention}`)

  it('reads a forge username, mine lit, never an address nor the dot after it', () => {
    expect(chips('@ann and @Bob.b, see a@b.com or @bob.', { me: 'bob' })).toEqual(['@ann:other', '@Bob.b:other', '@bob:me'])
  })

  it('leaves code and links alone', () => {
    expect(chips('`@ann` [@bob](https://x.y) @carl', {})).toEqual(['@carl:other'])
  })

  it('reads only the known tokens of a tracker, the longest first', () => {
    expect(chips('@Ann Two and @Ann, not @Bob', { known: [{ text: '@Ann', mine: true }, { text: '@Ann Two', mine: false }] })).toEqual(['@Ann Two:other', '@Ann:me'])
    expect(chips('@Ann', { known: [] })).toEqual([])
    expect(chips('@Annabelle, @Ann.', { known: [{ text: '@Ann', mine: false }] })).toEqual(['@Ann:other'])
  })
})
