import { beforeAll, describe, expect, it } from 'vitest'
import { highlightDiff } from './highlight'
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
