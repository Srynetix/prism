import { describe, expect, it } from 'vitest'
import { fold, hasToken, mentionMatches, mentionsIn, type Mentionable, type MentionSource } from './mentions.svelte'
import type { Mention } from './bindings/Mention'

const person = (key: string, name: string, handle: string | null = null): Mentionable => ({ key, name, handle }) as Mentionable
const source = (people: Mentionable[], first: string[] = [], me: string | null = null): MentionSource => ({
  people: () => people,
  error: () => '',
  load: () => {},
  first,
  me,
  token: (p) => `@${p.handle ?? p.name}`,
  rich: false,
  picture: () => undefined,
})

describe('mentions', () => {
  it('fold drops the accents and the case', () => {
    expect(fold('Élodie')).toBe('elodie')
    expect(fold('ÇA Va')).toBe('ca va')
  })

  it('offers the thread first, then those starting with the query, then the others; never me', () => {
    const people = [person('zoe', 'Zoé Martin', 'zoe'), person('anna', 'Anna Lee', 'alee'), person('me', 'Moi Même', 'me'), person('jean', 'Jean Annan', 'jann')]
    const names = (q: string, first: string[] = []) => mentionMatches(source(people, first, 'me'), q).map((p) => p.key)
    expect(names('')).toEqual(['anna', 'jean', 'zoe'])
    expect(names('', ['zoe'])).toEqual(['zoe', 'anna', 'jean'])
    // `ann`: Anna starts with it, Jean Annan too (a word of the name), by name.
    expect(names('ann')).toEqual(['anna', 'jean'])
    // Found inside only: after those it starts.
    expect(names('ar')).toEqual(['zoe'])
    expect(names('elo')).toEqual([])
    expect(names('zo')).toEqual(['zoe'])
    expect(mentionMatches(source(people), '', 2)).toHaveLength(2)
  })

  it('reads a token as a whole word only', () => {
    expect(hasToken('hi @Ann, look', '@Ann')).toBe(true)
    expect(hasToken('hi @Annabelle', '@Ann')).toBe(false)
    expect(hasToken('@Ann.', '@Ann')).toBe(true)
    expect(hasToken('cost (x+1)', '(x+1)')).toBe(true)
  })

  it('keeps the mentions still in the text, each once', () => {
    const ann = { id: 1, text: '@Ann' } as unknown as Mention
    const bob = { id: 2, text: '@Bob' } as unknown as Mention
    expect(mentionsIn('hi @Ann and @Ann', [ann, ann, bob])).toEqual([ann])
    expect(mentionsIn('nobody', [ann])).toEqual([])
  })
})
