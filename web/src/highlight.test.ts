import { beforeAll, describe, expect, it } from 'vitest'
import { highlightDiff, highlightText, langOf } from './highlight'
import { parseDiff } from './mr-review.svelte'

/** A docstring opened above the hunk, closed on its first line. */
const FILE = ['class Command:', '    def run(self):', '        """', '        Explains the run.', '        """', '        name = self.name', '        return name']
const DIFF = ['@@ -5,2 +5,3 @@ class Command:', '         """', '+        name = self.name', '         return name'].join('\n')

const colorOf = (tokens: { text: string; color?: string }[] | null | undefined, text: string) => tokens?.find((t) => t.text.includes(text))?.color

describe('highlightDiff', () => {
  // Shiki and its Python grammar load once, slowly.
  beforeAll(() => highlightDiff('warm.py', []), 30_000)

  it('starts a hunk from the state of the file above it', async () => {
    const lines = parseDiff(DIFF)
    const tokens = (await highlightDiff('command.py', lines, FILE))!
    const plain = (await highlightDiff('command.py', [{ kind: 'ctx', text: '        name = self.name', old: 1, new: 1 }]))!
    // The added line is code, as it reads outside any string.
    expect(colorOf(tokens[2], 'self')).toBe(colorOf(plain[0], 'self'))
  })

  it('without the file, reads the closing quotes as an opening', async () => {
    const lines = parseDiff(DIFF)
    const tokens = (await highlightDiff('command.py', lines))!
    const plain = (await highlightDiff('command.py', [{ kind: 'ctx', text: '        name = self.name', old: 1, new: 1 }]))!
    expect(colorOf(tokens[2], 'self')).not.toBe(colorOf(plain[0], 'self'))
  })
})

describe('highlightText', () => {
  it('knows a container\'s files by name', () => {
    expect(langOf('/etc/nginx/conf.d/site.conf')).toBe('nginx')
    expect(langOf('/app/.env.production')).toBe('dotenv')
    expect(langOf('/var/log/app.log')).toBe('log')
    expect(langOf('/etc/hosts')).toBe(null)
  })

  it('carries a string over its slices, and keeps every line', async () => {
    // A docstring opened on line 1 and closed past the first slice (300 lines).
    const text = ['x = """', ...Array.from({ length: 400 }, () => 'name = self.name'), '"""', 'name = self.name'].join('\n')
    const lines = (await highlightText('a.py', text))!
    expect(lines).toHaveLength(403)
    expect(lines.map((l) => l.map((t) => t.text).join(''))).toEqual(text.split('\n'))
    // Inside the string past the first slice, code after it.
    expect(colorOf(lines[350], 'self')).toBe(colorOf(lines[1], 'self'))
    expect(colorOf(lines[402], 'self')).not.toBe(colorOf(lines[350], 'self'))
  }, 30_000)

  it('leaves an unknown file plain', async () => {
    expect(await highlightText('/etc/hosts', '127.0.0.1 localhost')).toBe(null)
  })
})
