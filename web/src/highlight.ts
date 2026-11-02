import type { HighlighterCore, ThemedToken } from 'shiki/core'
import type { DiffLine } from './mr-review.svelte'

/**
 * Syntax highlighting of MR diffs, by Shiki (VS Code's grammars) with its JavaScript
 * regular expression engine (no WebAssembly). Everything is loaded on demand, when
 * a diff is opened: the engine, then the grammar of each language encountered. The theme
 * is only CSS variables (`--shiki-token-*`, app.css): the colors follow Prism's
 * theme. Tokens are rendered as Svelte elements, without injected HTML.
 */

/** Languages highlighted, by Shiki grammar. */
const GRAMMARS = {
  python: () => import('shiki/langs/python.mjs'),
  typescript: () => import('shiki/langs/typescript.mjs'),
  tsx: () => import('shiki/langs/tsx.mjs'),
  javascript: () => import('shiki/langs/javascript.mjs'),
  json: () => import('shiki/langs/json.mjs'),
  jsonc: () => import('shiki/langs/jsonc.mjs'),
  yaml: () => import('shiki/langs/yaml.mjs'),
  markdown: () => import('shiki/langs/markdown.mjs'),
  mdx: () => import('shiki/langs/mdx.mjs'),
  html: () => import('shiki/langs/html.mjs'),
  css: () => import('shiki/langs/css.mjs'),
  shellscript: () => import('shiki/langs/shellscript.mjs'),
  toml: () => import('shiki/langs/toml.mjs'),
  ini: () => import('shiki/langs/ini.mjs'),
  docker: () => import('shiki/langs/docker.mjs'),
  sql: () => import('shiki/langs/sql.mjs'),
  svelte: () => import('shiki/langs/svelte.mjs'),
  rust: () => import('shiki/langs/rust.mjs'),
} as const
type Lang = keyof typeof GRAMMARS

const EXTENSIONS: Record<string, Lang> = {
  py: 'python', pyi: 'python',
  ts: 'typescript', mts: 'typescript', cts: 'typescript', tsx: 'tsx', jsx: 'tsx',
  js: 'javascript', mjs: 'javascript', cjs: 'javascript',
  json: 'json', jsonc: 'jsonc', yml: 'yaml', yaml: 'yaml',
  md: 'markdown', mdx: 'mdx', html: 'html', css: 'css',
  sh: 'shellscript', bash: 'shellscript', bats: 'shellscript',
  toml: 'toml', ini: 'ini', cfg: 'ini', sql: 'sql', svelte: 'svelte', rs: 'rust',
}

/** Beyond this, a file stays plain text: highlighting would cost more than it brings. */
const MAX_LINES = 2000
/** Beyond this, a diff's hunks are highlighted without the file above them. */
const MAX_CONTEXT = 3000
/** Lines of the file above a hunk read in one go, the window free between two (tens of ms each). */
const CONTEXT_SLICE = 100

export function langOf(path: string): Lang | null {
  const name = path.split('/').pop() ?? ''
  if (/^Dockerfile(\..+)?$|\.dockerfile$/i.test(name)) return 'docker'
  const ext = name.includes('.') ? name.split('.').pop()!.toLowerCase() : ''
  return EXTENSIONS[ext] ?? null
}

export interface Token {
  text: string
  color?: string
  italic?: boolean
  bold?: boolean
}

let highlighter: Promise<HighlighterCore> | null = null
const loaded = new Map<Lang, Promise<void>>()

async function core(): Promise<HighlighterCore> {
  highlighter ??= (async () => {
    const [{ createHighlighterCore, createCssVariablesTheme }, { createJavaScriptRegexEngine }] = await Promise.all([
      import('shiki/core'),
      import('shiki/engine/javascript'),
    ])
    return createHighlighterCore({
      themes: [createCssVariablesTheme({ name: 'prism', variablePrefix: '--shiki-', fontStyle: true })],
      langs: [],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    })
  })()
  return highlighter
}

async function ready(lang: Lang): Promise<HighlighterCore> {
  const h = await core()
  let p = loaded.get(lang)
  if (!p) {
    p = GRAMMARS[lang]().then((m) => h.loadLanguage(m.default))
    p.catch(() => loaded.delete(lang))
    loaded.set(lang, p)
  }
  await p
  return h
}

const toToken = (t: ThemedToken): Token => ({
  text: t.content,
  color: t.color,
  // Shiki FontStyle: 1 italic, 2 bold.
  italic: !!(t.fontStyle && t.fontStyle & 1),
  bold: !!(t.fontStyle && t.fontStyle & 2),
})

/**
 * Tokens of each line of a diff (null for a hunk header, or without highlighting). Each
 * hunk is highlighted twice, old side (unchanged and removed lines) and new side
 * (unchanged and added), so that each line keeps its context: a line in the middle
 * of a multi-line string or comment is highlighted as such.
 *
 * With `whole`, the new version's lines, each hunk starts from the grammar's state at
 * the line above it in the file: a hunk that opens inside a docstring or a comment
 * begun higher up reads its closing quotes as such, not as an opening.
 */
export async function highlightDiff(path: string, lines: DiffLine[], whole: string[] | null = null): Promise<(Token[] | null)[] | null> {
  const lang = langOf(path)
  if (!lang || lines.length > MAX_LINES) return null
  if (whole && whole.length > MAX_CONTEXT) whole = null
  const h = await ready(lang)
  const out: (Token[] | null)[] = lines.map(() => null)
  type State = ReturnType<HighlighterCore['getLastGrammarState']>
  let state: State | undefined
  let read = 0
  /** The grammar's state after the file's first `upTo` lines, from where the last call left it. */
  const advance = async (upTo: number) => {
    while (whole && read < upTo) {
      const to = Math.min(upTo, read + CONTEXT_SLICE)
      state = h.getLastGrammarState(whole.slice(read, to).join('\n'), { lang, theme: 'prism', grammarState: state })
      read = to
      if (read < upTo) await new Promise((r) => setTimeout(r, 0))
    }
  }
  const tokenize = (idx: number[]) => {
    if (!idx.length) return
    const rows = h.codeToTokensBase(idx.map((i) => lines[i].text).join('\n'), { lang, theme: 'prism', grammarState: state })
    idx.forEach((i, n) => (out[i] ??= (rows[n] ?? []).map(toToken)))
  }
  let start = 0
  for (let i = 0; i <= lines.length; i++) {
    if (i < lines.length && lines[i].kind !== 'hunk') continue
    const hunk = Array.from({ length: i - start }, (_, n) => start + n)
    const first = hunk.find((n) => lines[n].new !== null)
    if (first !== undefined) await advance(lines[first].new! - 1)
    // New side first: an unchanged line takes its colors from there.
    tokenize(hunk.filter((n) => lines[n].kind !== 'del'))
    tokenize(hunk.filter((n) => lines[n].kind === 'del' || lines[n].kind === 'ctx'))
    start = i + 1
  }
  return out
}
