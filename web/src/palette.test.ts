import { describe, expect, it } from 'vitest'
import { search, type Entry } from './palette-search'

const entry = (over: Partial<Entry> & Pick<Entry, 'id' | 'kind' | 'title'>): Entry => ({ run: () => {}, ...over })

const ALL: Entry[] = [
  entry({ id: 'page:home', kind: 'page', title: 'Home' }),
  entry({ id: 'page:ci', kind: 'page', title: 'CI' }),
  entry({ id: 'block:ci', kind: 'block', title: 'CI health' }),
  entry({ id: 'mr:!1957', kind: 'mr', title: '!1957 feat: CSV export', words: 'agent/acme-4688 alice', waiting: true }),
  entry({ id: 'ticket:ACME-4688', kind: 'ticket', title: 'ACME-4688 Export the Data block as CSV' }),
  entry({ id: 'agent:a1', kind: 'agent', title: 'Export CSV', words: 'ACME-4688', waiting: true }),
  entry({ id: 'cmd:note', kind: 'command', title: 'note', usage: '<text>', required: true }),
  entry({ id: 'cmd:rescan', kind: 'command', title: 'rescan' }),
]

describe('palette search', () => {
  it('shows what waits and the pages when nothing is typed', () => {
    const groups = search('', ALL)
    expect(groups.map((g) => g.title)).toEqual(['Waiting for you', 'Pages'])
    expect(groups[0].rows.map((r) => r.entry.id)).toEqual(['mr:!1957', 'agent:a1'])
  })

  it('groups the matches, best first, on the title and the hidden words', () => {
    const groups = search('4688', ALL)
    expect(groups[0].title).toBe('Tickets')
    expect(groups.flatMap((g) => g.rows.map((r) => r.entry.id))).toEqual(expect.arrayContaining(['ticket:ACME-4688', 'agent:a1', 'mr:!1957']))
    expect(search('ci', ALL)[0].rows[0].entry.id).toBe('page:ci')
    expect(search('zzz', ALL)).toEqual([])
  })

  it('runs a command with the words after its name', () => {
    const [run] = search('note buy milk', ALL)
    expect(run.title).toBe('Run')
    expect(run.rows[0]).toMatchObject({ entry: { id: 'cmd:note' }, words: 'buy milk' })
    // A command without words is only matched.
    expect(search('rescan now', ALL).flatMap((g) => g.rows).some((r) => r.words)).toBe(false)
  })
})
