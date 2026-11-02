import { describe, expect, it } from 'vitest'
import { carry, commentable, compose, lineDiff, passages, quoteOf, spanOf, type TextComment } from './comments'

const PLAN = '# Fix\n\n## Steps\n1. **Fix the operator** in `calc.py:2`.\n2. Add a test in test_calc.py.\n\n   With two cases.\n3. Verify.\n\nOut of scope: XLSX.'

describe('passages', () => {
  it('cuts a plan into headings, paragraphs and each item of a list, numbers kept', () => {
    const ps = passages(PLAN)
    expect(ps.map((p) => p.source)).toEqual(['# Fix', '## Steps', '1. **Fix the operator** in `calc.py:2`.', '2. Add a test in test_calc.py.\n\n   With two cases.', '3. Verify.', 'Out of scope: XLSX.'])
    const third = ps[4].block
    expect(third.kind === 'list' && third.items[0].number).toBe(3)
  })
})

describe('compose', () => {
  const c = (quote: string, body: string): TextComment => ({ id: quote, quote, body, status: 'draft' })
  it('quotes each passage, in the text’s order, then the general words', () => {
    const ps = passages(PLAN)
    const text = compose('plan', [c('3. Verify.', 'Run pytest.'), c('1. **Fix the operator** in `calc.py:2`.', 'Yes.\nAnd a comment.')], 'Keep it short.', ps, true)
    expect(text).toBe('Comments on your plan:\n\n1. On “Fix the operator in calc.py:2.”:\n   Yes.\n   And a comment.\n\n2. On “Verify.”:\n   Run pytest.\n\nAlso: Keep it short.\n\nRevise the plan.')
    expect(compose('answer', [], 'Only this.')).toBe('Only this.')
  })
  it('quotes a comment on several passages by its first and its last', () => {
    const text = compose('plan', [{ ...c('1. **Fix the operator** in `calc.py:2`.', 'Both together.'), end: '3. Verify.' }], '', passages(PLAN))
    expect(text).toBe('Comments on your plan:\n\n1. From “Fix the operator in calc.py:2.” to “Verify.”:\n   Both together.')
  })
  it('cuts a long quote', () => {
    expect(quoteOf('- ' + 'word '.repeat(40), 20)).toBe('word word word word…')
  })
})

describe('carry', () => {
  it('keeps as a draft a comment whose passage did not change; one whose passage changed is addressed', () => {
    const sent: TextComment[] = [
      { id: 'a', quote: '3. Verify.', body: 'Run pytest.', status: 'sent', call: 'p1' },
      { id: 'b', quote: '2. Add a test in test_calc.py.\n\n   With two cases.', body: 'In tests/.', status: 'sent', call: 'p1' },
      { id: 'c', quote: '# Fix', body: 'later', status: 'draft' },
    ]
    const next = passages(PLAN.replace('in test_calc.py', 'in tests/test_calc.py'))
    expect(carry(sent, next, 'p2').map((x) => [x.id, x.status])).toEqual([['a', 'draft'], ['b', 'addressed'], ['c', 'draft']])
    expect(carry(sent, next, 'p1')).toEqual(sent)
  })
  it('addresses a comment on several passages once any of its ends changed', () => {
    const run: TextComment = { id: 'r', quote: '2. Add a test in test_calc.py.\n\n   With two cases.', end: '3. Verify.', body: 'x', status: 'sent', call: 'p1' }
    expect(carry([run], passages(PLAN), 'p2')[0].status).toBe('draft')
    expect(carry([run], passages(PLAN.replace('in test_calc.py', 'in tests/test_calc.py')), 'p2')[0].status).toBe('addressed')
  })
})

describe('spanOf', () => {
  it('finds the first and last passages of a comment, the last after the first', () => {
    const ps = passages(PLAN)
    const first = ps.findIndex((p) => p.source.startsWith('1.'))
    const last = ps.findIndex((p) => p.source === '3. Verify.')
    expect(spanOf(ps, { quote: ps[first].source, end: '3. Verify.' })).toEqual([first, last])
    expect(spanOf(ps, { quote: '3. Verify.' })).toEqual([last, last])
    expect(spanOf(ps, { quote: '3. Verify.', end: ps[first].source })).toBeNull()
    expect(spanOf(ps, { quote: 'gone' })).toBeNull()
  })
})

describe('lineDiff', () => {
  it('keeps the common lines, the others removed or added', () => {
    expect(lineDiff('a\nb\nc', 'a\nB\nc\nd')).toEqual([
      { kind: 'same', text: 'a' }, { kind: 'del', text: 'b' }, { kind: 'add', text: 'B' }, { kind: 'same', text: 'c' }, { kind: 'add', text: 'd' },
    ])
  })
})

it('commentable: long, or several passages', () => {
  expect(commentable('Done.')).toBe(false)
  expect(commentable(PLAN)).toBe(true)
})
