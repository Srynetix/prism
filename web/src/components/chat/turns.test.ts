import { describe, expect, it } from 'vitest'
import type { ConversationTool, ConversationTurn } from '../../agents.svelte'
import { matches, promptImages, proposalCard, segments, summarize, sumSteps } from './turns'

const tool = (t: Partial<ConversationTool>) => ({ type: 'tool' as const, tool: 'Edit', kind: 'write' as const, what: null, state: 'ok' as const, output: null, diff: [], added: 0, removed: 0, at: null, ms: null, ...t })

const turn: ConversationTurn = {
  n: 2,
  prompt: 'Round per line on credit notes',
  at: '2026-10-17T10:00:00Z',
  last: '2026-10-17T10:22:00Z',
  tokens: 0,
  cost: null,
  model: null,
  entries: [
    tool({ what: 'src/vat.ts', added: 3 }),
    tool({ tool: 'Bash', kind: 'command', what: 'npm test', state: 'failed' }),
    tool({ what: 'src/vat.ts', added: 1, removed: 2 }),
    tool({ tool: 'Bash', kind: 'command', what: 'npm test' }),
    { type: 'text', text: '## **Done**\n\nAll **green** now.' },
  ],
}

describe('turns', () => {
  it('sums a turn up', () => {
    expect(summarize(turn)).toEqual({ files: [{ path: 'src/vat.ts', added: 4, removed: 2 }], commands: 2, failed: 1, ms: 22 * 60_000, last: 'Done' })
  })

  it('finds a turn by its prompt, its words or its tools', () => {
    expect(matches(turn, 'credit')).toBe(true)
    expect(matches(turn, 'GREEN vat.ts')).toBe(true)
    expect(matches(turn, 'credit pipeline')).toBe(false)
    expect(matches(turn, '  ')).toBe(true)
  })

  it('folds the steps between its words, a question apart', () => {
    const ask = tool({ tool: 'AskUserQuestion', kind: 'other', ask: { questions: [] } as never })
    const think = { type: 'thinking' as const, text: '', ms: null }
    const segs = segments([think, ...turn.entries.slice(0, 2), ask, turn.entries[3], turn.entries[4]])
    expect(segs.map((s) => (s.type === 'steps' ? `steps ${s.from}:${s.entries.length}` : `one ${s.at}`))).toEqual(['steps 0:3', 'one 3', 'steps 4:1', 'one 5'])
  })

  it('keeps a subagent out of the fold', () => {
    const agent = tool({ tool: 'Agent', kind: 'agent' })
    const segs = segments([turn.entries[0], agent, turn.entries[1], turn.entries[2]])
    expect(segs.map((s) => (s.type === 'steps' ? `steps ${s.from}:${s.entries.length}` : `one ${s.at}`))).toEqual(['steps 0:1', 'one 1', 'steps 2:2'])
  })

  it('sums its steps up, each file once', () => {
    expect(sumSteps(turn.entries.slice(0, 4))).toEqual({ steps: 4, read: 0, edited: 1, added: 4, removed: 2, commands: 2, others: 0, failed: 1 })
  })
})

describe('promptImages', () => {
  it('takes the images out of the words', () => {
    expect(promptImages('@.prism/attach/shot.png It overflows, see @"/w/attach/My shot.jpg".')).toEqual({ text: 'It overflows, see .', images: ['.prism/attach/shot.png', '/w/attach/My shot.jpg'] })
  })
  it('leaves other mentions and addresses alone', () => {
    expect(promptImages('Read @src/a.ts and mail me@x.png')).toEqual({ text: 'Read @src/a.ts and mail me@x.png', images: [] })
  })
  it('takes images one space apart, as the Chat box writes them', () => {
    expect(promptImages('Look @.prism/attach/a.png @.prism/attach/b.jpg ')).toEqual({ text: 'Look', images: ['.prism/attach/a.png', '.prism/attach/b.jpg'] })
  })
  it('keeps punctuation after a bare path out of it', () => {
    expect(promptImages('(see @a.png) then @b.png.')).toEqual({ text: '(see ) then .', images: ['a.png', 'b.png'] })
  })
})

describe('a proposal of a change of Prism', () => {
  const propose = (t: Partial<ConversationTool>) => tool({ tool: 'mcp__prism__prism_propose_config', kind: 'other', propose: true, ...t })

  it('is a card while it waits and once answered, a step when invalid or handed on', () => {
    const waiting = propose({ state: 'running' })
    const applied = propose({ output: '{"status":"applied"}' })
    const refused = propose({ output: '{"status":"refused","reason":"not that repo"}' })
    const invalid = propose({ output: '{"status":"invalid","errors":["x"]}' })
    const handed = propose({ output: '{"status":"waiting"}' })
    for (const e of [waiting, applied, refused]) expect(proposalCard(e)).toBe(true)
    for (const e of [invalid, handed, tool({})]) expect(proposalCard(e)).toBe(false)
    // Never folded with the steps around it.
    const segs = segments([tool({}), waiting, tool({}), invalid])
    expect(segs.map((s) => s.type)).toEqual(['steps', 'one', 'steps'])
  })
})
