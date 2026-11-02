import { describe, expect, it } from 'vitest'
import { clickedOutside } from './drawers'

/** Just what `clickedOutside` reads of an element: where it is, and the ancestors it matches. */
const el = (matches: string[] = [], dataset: Record<string, string> = {}) =>
  ({ isConnected: true, dataset, closest: (sel: string) => (sel.split(', ').some((s) => matches.includes(s)) ? el([], dataset) : null) }) as unknown as HTMLElement
const drawer = { dataset: { drawer: '1' }, contains: () => false } as unknown as HTMLElement
const click = (target: HTMLElement) => ({ target }) as unknown as PointerEvent

describe('clickedOutside', () => {
  it('closes on a click elsewhere in the window', () => {
    expect(clickedOutside(click(el()), drawer)).toBe(true)
  })

  it('stays open on a choice of an app-drawn select or suggestions list, a menu, a dialog', () => {
    for (const role of ['[role=listbox]', '[role=menu]', '[role=dialog]']) expect(clickedOutside(click(el([role])), drawer)).toBe(false)
  })
})
