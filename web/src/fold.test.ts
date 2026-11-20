import { describe, expect, it } from 'vitest'
import { foldOut } from './fold'

describe('foldOut', () => {
  const widths = new Map([['a', 100], ['b', 80], ['c', 60], ['d', 0]])
  const ids = ['a', 'b', 'c', 'd']

  it('folds nothing while everything fits', () => {
    expect(foldOut(ids, widths, 240, 30)).toEqual([])
  })
  it('folds from the right, the ⋯ counted once one goes', () => {
    // c then b go; c fits again in the room b left.
    expect(foldOut(ids, widths, 200, 30)).toEqual(['b'])
    expect(foldOut(ids, widths, 230, 30)).toEqual(['c'])
  })
  it('folds the lowest rank first, never an Infinity one, never one with no width', () => {
    const rank = (id: string) => ({ a: 0, b: Infinity, c: 5 })[id] ?? 0
    expect(foldOut(ids, widths, 200, 30, rank)).toEqual(['a'])
    expect(foldOut(ids, widths, 10, 30, rank)).toEqual(['a', 'c'])
  })
  it('brings back a narrower one when a wider one gone left room', () => {
    // c goes first (rightmost), then a (wide): c fits again in what a left.
    const rank = (id: string) => ({ a: 1, b: 2, c: 0 })[id] ?? 0
    expect(foldOut(['a', 'b', 'c'], widths, 160, 0, rank)).toEqual(['a'])
  })
})
