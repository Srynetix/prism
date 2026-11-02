import { describe, expect, it } from 'vitest'
import { projectOf, replaceShortcodes, splitShortcodes } from './emoji.svelte'
import { GITHUB_ONLY, GITLAB_ONLY, SHORTCODES } from './emoji-shortcodes'

const table = (pairs: string) => new Map(pairs.split(' ').map((p) => [p.slice(0, p.indexOf(':')), p.slice(p.indexOf(':') + 1)]))
const common = table(SHORTCODES)
const on = (own: string) => (name: string) => table(own).get(name) ?? common.get(name)
const gitlab = on(GITLAB_ONLY)
const github = on(GITHUB_ONLY)

describe('replaceShortcodes', () => {
  it('draws the names of both forges', () => {
    expect(replaceShortcodes('LGTM :+1: :thumbsup: :tada:', gitlab)).toBe('LGTM 👍 👍 🎉')
    // GitLab's own names (gemojione), skin tones and flags.
    expect(replaceShortcodes(':slight_smile: :thumbsup_tone2: :flag_fr:', gitlab)).toBe('🙂 👍🏼 🇫🇷')
    // GitHub's.
    expect(replaceShortcodes(':slightly_smiling_face: :shipit:', github)).toBe('🙂 :shipit:')
  })

  it('follows the forge where names differ', () => {
    expect(replaceShortcodes(':cricket:', gitlab)).toBe('🏏')
    expect(replaceShortcodes(':cricket:', github)).toBe('🦗')
  })

  it('leaves unknown names, glued ones and times as written', () => {
    expect(replaceShortcodes(':not_an_emoji: a:smile: 10:30:00', gitlab)).toBe(':not_an_emoji: a:smile: 10:30:00')
    expect(replaceShortcodes(':smile::smile:', gitlab)).toBe('😄😄')
  })
})

describe('emojiMatches', async () => {
  const { emojiMatches } = await import('./emoji.svelte')
  // The table loads on the first ask.
  emojiMatches('thu', false)
  await import('./emoji-shortcodes')
  await new Promise((r) => setTimeout(r, 0))

  it('offers names that start with the word first, each emoji once', () => {
    const found = emojiMatches('thu', false)!
    expect(found[0]).toEqual({ name: 'thumbsup', emoji: '👍' })
    expect(found.filter((f) => f.emoji === '👍')).toHaveLength(1)
    expect(found.map((f) => f.name)).toContain('thumbsdown')
  })

  it('finds a word inside a name, and follows the forge', () => {
    expect(emojiMatches('smile', false)!.map((f) => f.name)).toContain('smiley_cat')
    expect(emojiMatches('cricket', false)![0].emoji).toBe('🏏')
    expect(emojiMatches('cricket', true)![0].emoji).toBe('🦗')
  })
})

describe('splitShortcodes', () => {
  const custom = new Map([['party-parrot', 'https://gitlab.example.org/uploads/-/system/custom_emoji/file/1/p.gif'], ['tada', 'https://x/tada.png']])

  it('cuts a custom emoji out as its own piece, the standard ones replaced around it', () => {
    expect(splitShortcodes('Merged :party-parrot: :tada: !', gitlab, custom)).toEqual([
      { text: 'Merged ' },
      { text: ':party-parrot:', emoji: 'https://gitlab.example.org/uploads/-/system/custom_emoji/file/1/p.gif' },
      // A standard name wins over a custom one.
      { text: ' 🎉 !' },
    ])
  })

  it('works before the standard table is loaded, and leaves unknown names', () => {
    expect(splitShortcodes(':party-parrot::nope:', null, custom)).toEqual([{ text: ':party-parrot:', emoji: custom.get('party-parrot') }, { text: ':nope:' }])
    expect(splitShortcodes('plain', gitlab, null)).toEqual([{ text: 'plain' }])
  })

  it('reads the project from the web address', () => {
    expect(projectOf('https://gitlab.example.org/acme/shop/app/')).toBe('acme/shop/app')
  })
})
