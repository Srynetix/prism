import { describe, expect, it } from 'vitest'
import { bounds, change, ofTool, period, sum, tokens, toolsOf } from './period'
import type { TokenDay } from './types'

const day = (date: string, input: number, model = 'claude-opus-5-5', project = 'shop'): TokenDay => ({
  date, input, output: 0, cache_read: 0, cache_write: 0, replies: 1, cost: input / 1000, unpriced: 0, models: [[model, input]], projects: [[project, input]], tools: [],
})
// Thursday 15 October 2026.
const today = new Date(2026, 9, 15, 10)

describe('token periods', () => {
  it('a week runs Monday to Sunday, a month by the calendar', () => {
    const w = bounds('week', 0, today)
    expect([w.from.getDate(), w.to.getDate()]).toEqual([12, 18])
    expect(bounds('week', 1, today).from.getDate()).toBe(5)
    const m = bounds('month', 1, today)
    expect([m.from.getMonth(), m.from.getDate(), m.to.getDate()]).toEqual([8, 1, 30])
  })

  it('sums the period, every day listed, and the one before', () => {
    const all = [day('2026-10-06', 50), day('2026-10-12', 10, 'claude-sonnet-5-5'), day('2026-10-14', 30)]
    const p = period(all, 'week', 0, today)
    expect(p.days.map((d) => d.tokens)).toEqual([10, 0, 30, 0, 0, 0, 0])
    expect(p.days.map((d) => d.future)).toEqual([false, false, false, false, true, true, true])
    expect(p.totals.tokens).toBe(40)
    expect(p.totals.models).toEqual([['claude-opus-5-5', 30], ['claude-sonnet-5-5', 10]])
    // Days without a tool breakdown (kept before the other tools): Claude Code's.
    expect(p.totals.tools).toEqual([['claude', 40]])
    expect(sum([{ ...day('2026-10-13', 30), tools: [['codex', 20], ['claude', 10]] }, day('2026-10-14', 5)]).tools).toEqual([['codex', 20], ['claude', 15]])
    // Under way (Thursday): against Monday to Thursday of the week before, not all of it.
    expect(p.partial).toBe(true)
    expect(p.before.tokens).toBe(50)
    // Saturday the 10th is after Thursday: left out of the comparison.
    const whole = period([...all, day('2026-10-08', 40), day('2026-10-10', 999)], 'week', 0, today)
    expect(whole.before.tokens).toBe(90)
    expect(change(whole.totals.tokens, whole.before.tokens)).toBe('−56 %')
    expect(period(all, 'week', 1, today).partial).toBe(false)
  })

  it('writes token counts short', () => {
    expect([tokens(830), tokens(12_400), tokens(3_420_000), tokens(1_200_000_000)]).toEqual(['830', '12k', '3.4M', '1.2B'])
  })

  it('filters by tool: its own part, a kept day of one tool whole, a mixed one by share', () => {
    const codex = { ...day('2026-10-13', 30, 'gpt-5-codex', 'api'), cost: 0, unpriced: 30, tools: [['codex', 30]] as [string, number][] }
    const split: TokenDay = { ...day('2026-10-13', 130), tools: [['claude', 100], ['codex', 30]], by_tool: { claude: day('2026-10-13', 100), codex } }
    const old = day('2026-10-12', 50)
    const mixed: TokenDay = { ...day('2026-10-14', 100), tools: [['claude', 75], ['codex', 25]] }
    const all = [old, split, mixed]
    expect(toolsOf(all)).toEqual(['claude', 'codex'])
    expect(ofTool(all, 'codex').map((d) => [d.date, d.input, d.models.length])).toEqual([['2026-10-13', 30, 1], ['2026-10-14', 25, 0]])
    expect(ofTool(all, 'claude').map((d) => d.input)).toEqual([50, 100, 75])
    expect(ofTool(all, 'opencode')).toEqual([])
  })
})
