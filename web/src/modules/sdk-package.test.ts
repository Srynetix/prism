import { describe, expect, it } from 'vitest'
import index from '../../../sdk/web/index.d.ts?raw'
import words from '../../../sdk/web/words.d.ts?raw'
import exportsJson from '../../../sdk/web/exports.json?raw'

/**
 * `@prism/sdk` (sdk/web), the package a module is built with, says what this window gives: its
 * generated files (the names of sdk.ts and sdk-words.ts among them) by
 * `node web/scripts/module-sdk.mjs --check` (`just check`), here the names its declarations type.
 */
describe('the module SDK’s package', () => {
  const exports = JSON.parse(exportsJson) as { specifiers: Record<string, string[]> }

  it('declares the types of every name it gives, and no other', () => {
    const declared = (text: string) =>
      [
        ...[...text.matchAll(/^export declare (?:function|const) (\w+)/gm)].map((m) => m[1]),
        // `export { t, tc, tn } from './words'`: values re-exported (types go by `export type`).
        ...[...text.matchAll(/^export \{([^}]*)\} from/gm)].flatMap((m) => m[1].split(',').map((n) => n.trim())),
      ].sort()
    expect(declared(index)).toEqual([...exports.specifiers['@prism/sdk']].sort())
    expect(declared(words)).toEqual([...exports.specifiers['@prism/sdk/words']].sort())
  })
})
