import { describe, expect, it } from 'vitest'
import { actualOf, actualRows, actualsCsv, agentsByTicket, estimateOf, medianOf, pct, shortModel, tierOf, tokensShort, tone, totals, trends, typeOf, unlinked, type ActualRow } from './actuals'
import type { AgentCost } from './bindings/AgentCost'
import type { Ticket, TimeEntry } from './types'

const H = 3_600_000
const agent = (id: string, over: Partial<AgentCost> = {}): AgentCost => ({ id, ticket: null, branch: null, title: id, kind: 'Implement', provider: 'claude', model: 'claude-opus-5-5', started: '2026-10-20T09:00:00Z', last: '2026-10-20T10:00:00Z', active_ms: 30 * 60_000, tokens: 1_000, cost: 1.5, ...over })
const ticket = (id: string, over: Partial<Ticket> = {}): Ticket => ({ id, task_id: id, name: id, status: 'done', closed: true, assignees: [], waiting_on: [], blocking: [], fields: [], time_estimate: null, parent: null, priority: null, task_type: null, task_type_icon: null, list: null, repo: null, description: null, url: '', created_at: null, updated_at: null, ...over })
const entry = (ticket: string, hours: number): TimeEntry => ({ id: `${ticket}${hours}`, ticket, task_name: null, task_url: null, start: '2026-10-20T09:00:00Z', duration_ms: hours * H, description: '' })

describe('a ticket’s actual', () => {
  it('reads the estimate from the tracker, else from an estimate field', () => {
    expect(estimateOf({ time_estimate: 7 * H })).toBe(420)
    expect(estimateOf({ time_estimate: null, fields: [{ name: 'Back', values: [{ label: '3,5', color: null, order: null }], filter: false, estimate: true }] })).toBe(210)
    expect(estimateOf({ time_estimate: null, fields: [{ name: 'Size', values: [{ label: '8', color: null, order: null }], filter: false, estimate: false }] })).toBeNull()
  })

  it('adds the agents’ active time to everyone’s logged time, unless logged only', () => {
    const agents = [agent('a', { active_ms: 58 * 60_000, cost: 3.1 }), agent('b', { active_ms: 42 * 60_000, cost: null })]
    const a = actualOf({ estimate: 420, spentMs: 8.5 * H, mineMs: 6 * H, agents, loggedOnly: false })
    expect(a).toMatchObject({ logged: 510, mine: 360, others: 150, agentMinutes: 100, agentCost: 3.1, tokens: 2_000, actual: 610 })
    expect(pct(a.gap!)).toBe('+45 %')
    expect(actualOf({ estimate: 420, spentMs: 8.5 * H, mineMs: 0, agents, loggedOnly: true }).actual).toBe(510)
    // The tracker says nothing of everyone's time: mine, and nobody else's known.
    expect(actualOf({ estimate: null, spentMs: null, mineMs: 2 * H, agents: [], loggedOnly: false })).toMatchObject({ logged: 120, others: null, agentCost: null, gap: null })
  })

  it('links an agent by its ticket, else by its branch’s MR; the others are unlinked', () => {
    const costs = [agent('a', { ticket: 'ABC-1' }), agent('b', { branch: 'feat/x' }), agent('c', { branch: 'nowhere' })]
    const by = agentsByTicket(costs, [{ source_branch: 'feat/x', ticket: 'ABC-2', tickets: ['ABC-2', 'ABC-3'] }])
    expect(by.get('ABC-1')?.map((a) => a.id)).toEqual(['a'])
    expect(by.get('ABC-3')?.map((a) => a.id)).toEqual(['b'])
    expect(unlinked(by, Date.parse('2026-10-20T00:00:00Z'), Date.parse('2026-10-21T00:00:00Z')).map((a) => a.id)).toEqual(['c'])
    expect(unlinked(by, Date.parse('2026-10-22T00:00:00Z'), Date.parse('2026-10-23T00:00:00Z'))).toEqual([])
  })

  it('shortens tokens and models', () => {
    expect([tokensShort(2_100_000), tokensShort(840_400), tokensShort(950)]).toEqual(['2.1 M', '840 k', '950'])
    expect([shortModel('claude-opus-5-5'), shortModel('claude-sonnet-5'), shortModel('gpt-6.1-sol'), shortModel(null)]).toEqual(['opus', 'sonnet', 'gpt-6.1-sol', ''])
  })

  it('reads a gap', () => {
    expect([tone(0.1), tone(-0.15), tone(0.3), tone(0.45), tone(-0.6), tone(null)]).toEqual(['ok', 'ok', 'warn', 'ko', 'ko', 'none'])
    expect(pct(-0.2)).toBe('−20 %')
  })
})

describe('the sprint’s actuals', () => {
  it('takes the type from where the setting says', () => {
    const t = { task_type: 'Bug', tags: ['back', 'api'], fields: [{ name: 'Area', values: [{ label: 'Front', color: null, order: null }], filter: true, estimate: false }] }
    expect([typeOf(t, ''), typeOf(t, 'tags'), typeOf(t, 'area'), typeOf(t, 'Nope')]).toEqual(['Bug', 'back', 'Front', null])
  })

  it('rows, totals and the median gap', () => {
    const tickets = [ticket('A', { time_estimate: 7 * H, time_spent: 8.5 * H, tags: ['back'] }), ticket('B', { time_estimate: 3.5 * H, tags: ['front'] }), ticket('C', { tags: ['bug'] })]
    const agents = agentsByTicket([agent('x', { ticket: 'A', active_ms: H, cost: 2 }), agent('y', { ticket: 'C', active_ms: H / 2, cost: 1 })], [])
    const rows = actualRows(tickets, agents, [entry('B', 3), entry('C', 6)], { loggedOnly: false, typeSource: 'tags', sprint: 'S44' })
    expect(rows.map((r) => [r.id, r.type, r.actual, r.gap === null ? null : Math.round(r.gap * 100)])).toEqual([['A', 'back', 570, 36], ['B', 'front', 180, -14], ['C', 'bug', 390, null]])
    const sum = totals(rows)
    expect(sum).toMatchObject({ count: 3, estimated: 630, estimatedActual: 750, actual: 1140, agentMinutes: 90, agentCost: 3, perTicket: 1 })
    expect(Math.round(sum.medianGap! * 100)).toBe(11)
  })

  it('says what the past sprints say, by type and by tier', () => {
    const row = (id: string, type: string, estimate: number, actual: number): ActualRow => ({ id, name: id, type, estimate, logged: actual, agentMinutes: 0, agentCost: null, actual, gap: actual / estimate - 1, sprint: 'S' })
    const rows = [row('1', 'back', 420, 580), row('2', 'back', 420, 600), row('3', 'back', 420, 560), row('4', 'front', 210, 190), row('5', 'front', 210, 200), row('6', 'front', 840, 900)]
    const { byType, byTier } = trends(rows)
    expect(byType.map((t) => [t.type, t.n])).toEqual([['back', 3], ['front', 3]])
    expect(byTier[0]).toMatchObject({ type: 'back', tier: 420, n: 3, medianActual: 580 })
    expect(Math.round(byTier[0].medianGap * 100)).toBe(38)
    expect(byTier.find((t) => t.type === 'front')).toMatchObject({ tier: 210, n: 2 })
    expect(tierOf(200)).toBe(210)
    expect(medianOf([])).toBeNull()
  })

  it('exports CSV, quoting what needs it', () => {
    const csv = actualsCsv([{ id: 'A', name: 'Purge, "expired"', type: null, estimate: 420, logged: 510, agentMinutes: 100, agentCost: 4.8, actual: 610, gap: 0.452, sprint: 'S44' }], ['Sprint', 'Ticket', 'Name', 'Type', 'Estimate (h)', 'Logged (h)', 'Agents (h)', 'Agents ($)', 'Actual (h)', 'Gap (%)'])
    expect(csv.split('\n')[1]).toBe('S44,A,"Purge, ""expired""",,7,8.5,1.67,4.8,10.17,45')
  })

  it('keeps a name a spreadsheet would run as text', () => {
    const csv = actualsCsv([{ id: 'B', name: '=HYPERLINK("https://x")', type: null, estimate: null, logged: 0, agentMinutes: 0, agentCost: null, actual: 0, gap: -0.2, sprint: 'S44' }], ['h'])
    expect(csv.split('\n')[1]).toBe(`S44,B,"'=HYPERLINK(""https://x"")",,,0,0,,0,-20`)
  })
})
