/// <reference types="node" />
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { beforeAll, describe, expect, it } from 'vitest'
import { DICTIONARIES, loadDictionary } from './i18n'
import { TREE } from './settings-tree'

beforeAll(() => loadDictionary('fr'))

const repo = fileURLToPath(new URL('../..', import.meta.url))
/** Where Prism names a place of Settings: the app, its help, its back end, its docs. */
const ROOTS = ['web/src', 'src-tauri/crates', 'docs', 'examples', 'packs', 'README.md', 'config.example.toml']
const SKIP = new Set(['node_modules', 'target', 'dist', 'mockups', 'bindings'])

function files(path: string, out: string[] = []): string[] {
  let isDir: boolean
  try {
    isDir = statSync(path).isDirectory()
  } catch {
    return out
  }
  if (!isDir) {
    if (/\.(ts|svelte|md|rs|toml)$/.test(path) && !path.endsWith('settings-crumbs.test.ts')) out.push(path)
    return out
  }
  for (const f of readdirSync(path)) if (!SKIP.has(f)) files(join(path, f), out)
  return out
}

const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
/** `label` at the start of `text`, a whole word: the rest after it, or null. */
function after(text: string, label: string): string | null {
  const m = new RegExp(`^${esc(label)}(?![\\p{L}\\d])`, 'u').exec(text)
  return m ? text.slice(m[0].length) : null
}

/**
 * A crumb « Settings › Domain › Page » names a domain of settings-tree.ts and one of its pages
 * (in French « Réglages › … », with their French labels): checked are those two only, what follows
 * the page (a provider, a field) and a crumb naming the domain alone are left be. The system's own
 * settings (« System Settings › Privacy », Windows' « Settings › Apps », ClickUp › Settings) are not
 * Prism's.
 */
describe('settings crumbs', () => {
  it('name a domain and one of its pages of the tree', () => {
    const fr = (s: string) => DICTIONARIES.fr[s] ?? s
    const domains = (lang: 'en' | 'fr') =>
      TREE.map((d) => ({ label: lang === 'fr' ? fr(d.label) : d.label, pages: (d.children ?? []).map((p) => (lang === 'fr' ? fr(p.label) : p.label)) }))
        .sort((a, b) => b.label.length - a.label.length)
    const tree = { Settings: domains('en'), Réglages: domains('fr') }
    const OS = /^(Windows Update|Apps|Accounts)\b/
    const wrong: string[] = []
    for (const path of ROOTS.flatMap((r) => files(join(repo, r)))) {
      readFileSync(path, 'utf8').split('\n').forEach((line, i) => {
        for (const m of line.matchAll(/(?<!System |› |\w)(Settings|Réglages) › /g)) {
          const rest = line.slice(m.index + m[0].length)
          if (/^[{$]/.test(rest) || OS.test(rest)) continue
          let ok = false
          for (const d of tree[m[1] as 'Settings' | 'Réglages']) {
            const tail = after(rest, d.label)
            if (tail === null) continue
            ok = !tail.startsWith(' › ') || d.pages.some((p) => after(tail.slice(3), p) !== null)
            break
          }
          if (!ok) wrong.push(`${path.slice(repo.length)}:${i + 1}: ${m[0]}${rest.slice(0, 40)}`)
        }
      })
    }
    expect(wrong).toEqual([])
    // Every source of the repository read: past 5 s while the other suites run.
  }, 30_000)
})
