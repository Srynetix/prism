import { describe, expect, it, vi } from 'vitest'

const live = { snapshot: null as Record<string, unknown> | null }
vi.mock('./live.svelte', () => ({ live }))

const { compactBands, fitBands, viewRows, views } = await import('./views')

describe('viewRows', () => {
  it('writes the edited page’s blocks and every page’s id, the others’ blocks untouched', () => {
    live.snapshot = { views: [
      { name: 'Dashboard', id: 'dashboard', columns: ['todo sessions', 'mrs'] },
      { name: 'Data', color: 'coral', columns: [] },
      { name: 'Data', columns: ['unknown-block'] },
      { name: ' ', columns: [] },
    ] }
    const ids = views().map((v) => v.id)
    expect(ids).toEqual(['dashboard', 'data', 'data-2'])
    expect(viewRows({ data: [['bqcost'], [], ['data']] })).toEqual([
      { name: 'Dashboard', id: 'dashboard', columns: ['todo sessions', 'mrs'] },
      { name: 'Data', id: 'data', color: 'coral', columns: ['bqcost', 'data'] },
      { name: 'Data', id: 'data-2', columns: ['unknown-block'] },
      { name: ' ', columns: [] },
    ])
  })

  it('keeps a page’s identity once moved: its id is written, not taken from its place', () => {
    live.snapshot = { views: viewRows({}).reverse() }
    expect(views().map((v) => v.id)).toEqual(['data-2', 'data', 'dashboard'])
  })

  it('writes the empty Dashboard when config.toml has no page', () => {
    live.snapshot = { views: [] }
    expect(viewRows({ dashboard: [['todo']] })).toEqual([{ name: 'Dashboard', id: 'dashboard', icon: 'dashboard', columns: ['todo'] }])
  })
})

describe('bands', () => {
  it('reads `---` as the start of a band below, empty bands skipped, one band without', () => {
    live.snapshot = { views: [
      { name: 'Dashboard', columns: ['agenda', '---', 'todo sessions', 'mrs', '----', '---'] },
      { name: 'Data', columns: ['---', 'bqcost', 'data'] },
    ] }
    const [dash, data] = views()
    expect(dash.columns).toEqual([['agenda'], ['todo', 'sessions'], ['mrs']])
    expect(dash.bands).toEqual([1, 2])
    expect(dash.sizes.bands).toEqual([1, 1])
    expect(data.bands).toBeUndefined()
  })

  it('writes `---` between bands, only when there are several, empty columns and bands left out', () => {
    live.snapshot = { views: [{ name: 'Dashboard', id: 'dashboard', columns: ['todo', 'mrs'] }] }
    expect(viewRows({ dashboard: [['agenda'], [], ['todo'], ['mrs']] }, { dashboard: [1, 1, 2] })[0].columns).toEqual(['agenda', '---', 'todo', 'mrs'])
    expect(viewRows({ dashboard: [['todo'], ['mrs']] })[0].columns).toEqual(['todo', 'mrs'])
  })

  it('fits the bands to the columns: the extra ones to the last band', () => {
    expect(fitBands(undefined, 3)).toEqual([3])
    expect(fitBands([1, 1], 4)).toEqual([1, 3])
    expect(fitBands([2, 2], 3)).toEqual([2, 1])
    expect(fitBands([3, 2], 2)).toEqual([2])
  })

  it('compacts: empty columns out, a band left with none goes', () => {
    expect(compactBands([['agenda'], [], ['todo'], []], [1, 1, 2])).toEqual({ columns: [['agenda'], ['todo']], bands: [1, 1], keep: [0, 2] })
  })
})
