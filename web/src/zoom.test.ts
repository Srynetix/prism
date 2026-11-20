import { describe, expect, it } from 'vitest'
import { clampZoom, stepZoom, ZOOMS, zoomLabel } from './zoom.svelte'
import { sizeStep } from './shortcuts'

const key = (k: string, code = '', alt = false) => ({ key: k, code, altKey: alt, metaKey: true, ctrlKey: false, shiftKey: false, target: null }) as unknown as KeyboardEvent

describe('window zoom', () => {
  it('steps through the browser’s steps, and stops at the ends', () => {
    expect(stepZoom(1, 1)).toBe(1.1)
    expect(stepZoom(1, -1)).toBe(0.9)
    expect(stepZoom(2, 1)).toBe(2)
    expect(stepZoom(0.67, -1)).toBe(0.67)
    // Between two steps (written by hand): the nearer one that way.
    expect(stepZoom(1.2, 1)).toBe(1.25)
    expect(stepZoom(1.2, -1)).toBe(1.1)
    expect(ZOOMS).toContain(1)
  })

  it('keeps a zoom between half and double', () => {
    expect(clampZoom(1.25)).toBe(1.25)
    expect(clampZoom(9)).toBe(2)
    expect(clampZoom(0.1)).toBe(0.5)
    expect(clampZoom(undefined)).toBe(1)
    expect(clampZoom(Number.NaN)).toBe(1)
    expect(zoomLabel(1.25)).toBe('125%')
  })

  it('reads + − 0 on any layout, ⌥ or not', () => {
    expect(sizeStep(key('='))).toBe(1)
    expect(sizeStep(key('+'))).toBe(1)
    expect(sizeStep(key('-'))).toBe(-1)
    expect(sizeStep(key('0'))).toBe(0)
    // AZERTY: 0 is on the à key.
    expect(sizeStep(key('à', 'Digit0'))).toBe(0)
    // ⌥ on a Mac changes the character.
    expect(sizeStep(key('≠', 'Equal', true))).toBe(1)
    expect(sizeStep(key('—', 'Digit6', true))).toBe(-1)
    expect(sizeStep(key('k', 'KeyK'))).toBe(null)
  })
})
