import { t } from './i18n.svelte'
import type { FieldSpec } from './settings-schema'
import { fieldsOf, pathOf, SECTION, type Section } from './settings-sections'
import { chainOf, PARENT, TREE, isProvider, type TreeNode } from './settings-tree'

/**
 * The Settings search: it lists settings, not pages. Each key of each page is an entry (its label,
 * key, table, hint, choices and synonyms); a query matches the entries where every word is found,
 * accents and case aside, in one of them, or as letters in order close together (« wrkt » finds
 * Worktrees). A word in brackets (`[forge]`) keeps the entries of that table. The entries come
 * best first: the label before the key, the key before the hint.
 */
export interface Entry {
  /** `section:key`: a key shown in two sections (`[forge]`'s GitHub keys) is two entries. */
  id: string
  path: string[]
  /** The dotted key, as config.toml writes it. */
  key: string
  field: FieldSpec
  section: Section
  /** The page showing it: a provider's parent page. */
  page: TreeNode
  /** Its provider (Forge › GitLab), if it is one's. */
  provider?: TreeNode
  /** Domain › page (› provider) › section, as the result shows it. */
  crumb: string[]
  /** What the words are looked for in, by weight: label and synonyms, key and table, the rest. */
  label: string
  keyText: string
  rest: string
}

/** Lower case, without accents nor typographic quotes: « Préférences » is « preferences ». */
export const norm = (s: string) =>
  s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[’‘]/g, "'")
    .toLowerCase()

/**
 * `word`'s letters in order in `text`, within twice its length: « wrkt » in « worktrees », not
 * scattered over a whole sentence. Only for words of 4 letters or more.
 */
export function near(word: string, text: string): boolean {
  if (word.length < 4) return false
  const span = word.length * 2
  for (let start = text.indexOf(word[0]); start >= 0; start = text.indexOf(word[0], start + 1)) {
    let j = start
    let ok = true
    for (const c of word.slice(1)) {
      j = text.indexOf(c, j + 1)
      if (j < 0 || j - start >= span) {
        ok = false
        break
      }
    }
    if (ok) return true
  }
  return false
}

/** The words of a query, and the tables it keeps (`[forge]`, `[[repos]]`). */
export function parse(query: string): { words: string[]; tables: string[] } {
  const parts = norm(query).split(/\s+/).filter(Boolean)
  const tables = parts.filter((p) => p.startsWith('[')).map((p) => p.replace(/[[\]]/g, '')).filter(Boolean)
  return { words: parts.filter((p) => !p.startsWith('[')), tables }
}

const startsWord = (text: string, word: string) => text.startsWith(word) || new RegExp(`[\\s._\\-/›(]${word.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`).test(text)

/**
 * How well `e` answers the words (0: not at all), its value (`value`, already as text) looked in too.
 * Each word must be found somewhere; the label and the synonyms weigh most, then the key and the
 * table, then the hint, the choices and the value, then letters close together.
 */
export function score(e: Entry, q: { words: string[]; tables: string[] }, value = ''): number {
  if (q.tables.length && !q.tables.some((tb) => norm(e.section.table).includes(tb) || e.key.startsWith(`${tb}.`) || e.key === tb)) return 0
  if (!q.words.length) return q.tables.length ? 1 : 0
  const v = norm(value)
  let total = 0
  for (const w of q.words) {
    const s = e.label.includes(w)
      ? (startsWord(e.label, w) ? 12 : 8)
      : e.keyText.includes(w)
        ? 6
        : e.rest.includes(w) || v.includes(w)
          ? 3
          : near(w, e.label) || near(w, e.keyText)
            ? 1
            : 0
    if (!s) return 0
    total += s
  }
  return total
}

/** A section's fields as entries, on their page. */
function entriesOf(page: TreeNode, provider: TreeNode | undefined, s: Section): Entry[] {
  const base = pathOf(s)
  if (!base) return []
  // The section's name when the page has others, and it is not the page's own.
  const own = provider ?? page
  const crumb = [...chainOf(page).map((n) => n.label), ...(provider ? [provider.label] : []), ...(s.label !== own.label && s.id !== own.sections?.[0] ? [s.label] : [])]
  return fieldsOf(s).map((f) => {
    const path = [...base, f.key]
    const syn = f.syn ? `${f.syn}` : ''
    const options = Array.isArray(f.options) ? f.options.join(' ') : ''
    const labels = f.labels ? Object.values(f.labels).join(' ') : ''
    return {
      id: `${s.id}:${f.key}`,
      path,
      key: path.join('.'),
      field: f,
      section: s,
      page,
      provider,
      crumb,
      label: norm(`${f.label} ${syn}`),
      keyText: norm(`${path.join('.')} ${f.key.replace(/_/g, ' ')} ${s.table}`),
      rest: norm(`${f.hint ?? ''} ${options} ${labels} ${f.placeholder ?? ''} ${crumb.join(' ')}`),
    }
  })
}

/** Every key of every page, providers' included, in the tree's order. */
export function buildIndex(tree: TreeNode[] = TREE): Entry[] {
  const out: Entry[] = []
  const seen = new Set<string>()
  const add = (page: TreeNode, provider: TreeNode | undefined, n: TreeNode) => {
    for (const id of n.sections ?? []) {
      const s = SECTION.get(id)
      if (!s) continue
      for (const e of entriesOf(page, provider, s)) {
        if (seen.has(e.id)) continue
        seen.add(e.id)
        out.push(e)
      }
    }
  }
  for (const root of tree)
    for (const page of root.children ?? []) {
      add(page, undefined, page)
      for (const p of (page.children ?? []).filter(isProvider)) add(page, p, p)
    }
  return out
}

/** The entries answering `query`, best first; `valueText` gives an entry's value as text. */
export function search(index: Entry[], query: string, valueText: (e: Entry) => string = () => ''): Entry[] {
  const q = parse(query)
  if (!q.words.length && !q.tables.length) return []
  return index
    .map((e, i) => ({ e, i, s: score(e, q, valueText(e)) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.e)
}

/** The pages whose name answers the query: shown as chips above the results. */
export function pagesFor(query: string, tree: TreeNode[] = TREE): TreeNode[] {
  const q = parse(query)
  if (!q.words.length) return []
  const pages = tree.flatMap((r) => (r.children ?? []).flatMap((p) => [p, ...(p.children ?? []).filter(isProvider)]))
  return pages.filter((p) => q.words.every((w) => norm(p.label).includes(w) || near(w, norm(p.label))))
}

/** A value as one short line, for a result or the save bar's diff. */
export function shown(v: unknown): string {
  if (v === undefined || v === null || v === '') return '—'
  if (typeof v === 'boolean') return v ? t('on') : t('off')
  if (Array.isArray(v)) return v.length ? (v.every((x) => typeof x !== 'object') ? v.join(', ') : `${v.length} ×`) : '[]'
  if (typeof v === 'object') return `{ ${Object.keys(v as object).length} }`
  return String(v)
}

/** A provider's parent page, a page's own node otherwise. */
export const pageNode = (n: TreeNode) => (isProvider(n) ? (PARENT.get(n.id) ?? n) : n)
