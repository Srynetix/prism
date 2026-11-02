import { describe, expect, it } from 'vitest'
import { ask, quote, refOf } from './ask'

const p = { path: 'src/cart/checkout.ts', from: 43, to: 46, text: 'const a = 1\nconst b = `x`\n' }

describe('ask', () => {
  it('names the lines, in the prompt language', () => {
    expect(refOf(p)).toBe('src/cart/checkout.ts:43-46')
    expect(refOf({ ...p, to: 43 })).toBe('src/cart/checkout.ts:43')
    expect(quote(p, 'en').split('\n')[0]).toBe('In `src/cart/checkout.ts`, lines 43–46:')
    expect(quote({ ...p, to: 43 }, 'fr').split('\n')[0]).toBe('Dans `src/cart/checkout.ts`, ligne 43 :')
  })
  it('fences the lines with a fence they cannot close', () => {
    expect(quote({ ...p, text: 'a ``` b' }, 'en')).toContain('````ts\na ``` b\n````')
    expect(ask(p, ' Why? ', 'en').endsWith('```\n\nWhy?')).toBe(true)
  })
})
