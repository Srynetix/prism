import type { Extension } from '@codemirror/state'
import { baseName } from '../platform'

/**
 * The language of a file for its editor, by its name: each one loaded on demand, when a file of
 * it is opened. Lezer grammars where CodeMirror has one, its legacy modes otherwise.
 */
type Loader = () => Promise<Extension>

const legacy = (load: () => Promise<{ [k: string]: unknown }>, name: string): Loader => async () => {
  const [{ StreamLanguage }, mode] = await Promise.all([import('@codemirror/language'), load()])
  return StreamLanguage.define(mode[name] as Parameters<typeof StreamLanguage.define>[0])
}

const js = (typescript: boolean, jsx: boolean): Loader => async () => (await import('@codemirror/lang-javascript')).javascript({ typescript, jsx })

const LOADERS: Record<string, Loader> = {
  ts: js(true, false), mts: js(true, false), cts: js(true, false), tsx: js(true, true),
  js: js(false, false), mjs: js(false, false), cjs: js(false, false), jsx: js(false, true),
  py: async () => (await import('@codemirror/lang-python')).python(), pyi: async () => (await import('@codemirror/lang-python')).python(),
  rs: async () => (await import('@codemirror/lang-rust')).rust(),
  json: async () => (await import('@codemirror/lang-json')).json(), jsonc: async () => (await import('@codemirror/lang-json')).json(),
  yml: async () => (await import('@codemirror/lang-yaml')).yaml(), yaml: async () => (await import('@codemirror/lang-yaml')).yaml(),
  md: async () => (await import('@codemirror/lang-markdown')).markdown(), mdx: async () => (await import('@codemirror/lang-markdown')).markdown(), markdown: async () => (await import('@codemirror/lang-markdown')).markdown(),
  html: async () => (await import('@codemirror/lang-html')).html(), htm: async () => (await import('@codemirror/lang-html')).html(),
  svelte: async () => (await import('@codemirror/lang-html')).html(), vue: async () => (await import('@codemirror/lang-html')).html(),
  css: async () => (await import('@codemirror/lang-css')).css(), scss: async () => (await import('@codemirror/lang-css')).css(), less: async () => (await import('@codemirror/lang-css')).css(),
  sql: async () => (await import('@codemirror/lang-sql')).sql(),
  toml: legacy(() => import('@codemirror/legacy-modes/mode/toml'), 'toml'),
  sh: legacy(() => import('@codemirror/legacy-modes/mode/shell'), 'shell'), bash: legacy(() => import('@codemirror/legacy-modes/mode/shell'), 'shell'),
  zsh: legacy(() => import('@codemirror/legacy-modes/mode/shell'), 'shell'), bats: legacy(() => import('@codemirror/legacy-modes/mode/shell'), 'shell'),
  env: legacy(() => import('@codemirror/legacy-modes/mode/shell'), 'shell'),
  ini: legacy(() => import('@codemirror/legacy-modes/mode/properties'), 'properties'), cfg: legacy(() => import('@codemirror/legacy-modes/mode/properties'), 'properties'),
  conf: legacy(() => import('@codemirror/legacy-modes/mode/properties'), 'properties'), properties: legacy(() => import('@codemirror/legacy-modes/mode/properties'), 'properties'),
  go: legacy(() => import('@codemirror/legacy-modes/mode/go'), 'go'),
  java: legacy(() => import('@codemirror/legacy-modes/mode/clike'), 'java'), kt: legacy(() => import('@codemirror/legacy-modes/mode/clike'), 'kotlin'),
  c: legacy(() => import('@codemirror/legacy-modes/mode/clike'), 'c'), h: legacy(() => import('@codemirror/legacy-modes/mode/clike'), 'c'),
  cpp: legacy(() => import('@codemirror/legacy-modes/mode/clike'), 'cpp'), hpp: legacy(() => import('@codemirror/legacy-modes/mode/clike'), 'cpp'),
  diff: legacy(() => import('@codemirror/legacy-modes/mode/diff'), 'diff'), patch: legacy(() => import('@codemirror/legacy-modes/mode/diff'), 'diff'),
  dockerfile: legacy(() => import('@codemirror/legacy-modes/mode/dockerfile'), 'dockerFile'),
}

/** What the status line names it. */
const NAMES: Record<string, string> = {
  ts: 'TypeScript', mts: 'TypeScript', cts: 'TypeScript', tsx: 'TSX', js: 'JavaScript', mjs: 'JavaScript', cjs: 'JavaScript', jsx: 'JSX',
  py: 'Python', pyi: 'Python', rs: 'Rust', json: 'JSON', jsonc: 'JSON', yml: 'YAML', yaml: 'YAML', md: 'Markdown', mdx: 'MDX', markdown: 'Markdown',
  html: 'HTML', htm: 'HTML', svelte: 'Svelte', vue: 'Vue', css: 'CSS', scss: 'SCSS', less: 'Less', sql: 'SQL', toml: 'TOML',
  sh: 'Shell', bash: 'Shell', zsh: 'Shell', bats: 'Shell', env: 'Env', ini: 'INI', cfg: 'INI', conf: 'Config', properties: 'Properties',
  go: 'Go', java: 'Java', kt: 'Kotlin', c: 'C', h: 'C', cpp: 'C++', hpp: 'C++', diff: 'Diff', patch: 'Diff', dockerfile: 'Dockerfile',
}

/** The key of `path`'s language: its extension, or the name of a file without one (`Dockerfile`, `.env`). */
function keyOf(path: string): string {
  const name = baseName(path).toLowerCase()
  if (/^dockerfile(\..+)?$|\.dockerfile$/.test(name)) return 'dockerfile'
  if (/^\.env(\..+)?$/.test(name)) return 'env'
  if (/^(makefile|justfile)$/.test(name)) return 'sh'
  return name.includes('.') ? name.split('.').pop()! : ''
}

export const languageName = (path: string): string => NAMES[keyOf(path)] ?? 'Text'

/** `path`'s language, loaded; `null`: plain text. */
export async function languageOf(path: string): Promise<Extension | null> {
  const load = LOADERS[keyOf(path)]
  return load ? load().catch(() => null) : null
}
