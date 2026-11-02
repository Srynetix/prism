import { BLOCK_HELP } from './block-help'
import { SCHEMA } from './settings-schema'
import { SHORTCUTS } from './shortcuts'
import { ALL_BLOCKS, BLOCK_LABELS, type BlockId } from './views'
import { BLOCK_DEFS } from './blocks'
import { keys as forPlatform } from './platform'
import { lang, t } from './i18n.svelte'

/**
 * In-app Help's content (opening it: `help.svelte.ts`): the guides, Markdown files of `help/` (`NN-id.md`, in that order), and three
 * references built from the app itself (blocks, shortcuts, settings), so always up to date.
 * A guide opens with a short header:
 *
 *     ---
 *     title: Merge requests and review
 *     group: Everyday work
 *     blocks: mrs            (the "?" of these blocks leads to the guide)
 *     related: ci, agents
 *     ---
 *     Lead paragraph…
 *
 * Its `prism:` links act in the app (`runAction`). Its images sit in `help/images/` and are given
 * by a relative path, `![The pages wizard](images/wizard.png){width=600}`: bundled with the app
 * (the window's CSP allows no external image). Guides are written the macOS way (`⌘S`,
 * Finder): `read` adapts them to the system, for display and search alike.
 * Apart, so that a page which shows no help (the phone's) never downloads the guides: what reads
 * them there imports this module when it needs it.
 */
export interface Guide {
  id: string
  title: string
  group: string
  blocks: BlockId[]
  related: string[]
  lead: string
  body: string
  /** URLs of its images, by the relative path its text gives them (`images/x.png`). */
  images: Record<string, string>
  /** Lowercase text, for the search. */
  text: string
}

export const GROUPS = ['Start', 'Everyday work', 'Watch the system', 'Make it yours', 'Reference']

/**
 * A guide in the window's language: `help/<lang>/NN-id.md` (the same header, its `group` kept in
 * English, the name `GROUPS` gives it), a block's `guide.translations[lang]`; failing that, the English one.
 */
const TRANSLATED: Record<string, string> = Object.fromEntries(
  Object.entries(import.meta.glob('./help/*/*.md', { query: '?raw', import: 'default', eager: true }) as Record<string, string>)
    .flatMap(([file, raw]) => {
      const m = /^\.\/help\/(\w+)\/(.+)$/.exec(file)
      return m && m[1] === lang() ? [[`./help/${m[2]}`, raw]] : []
    }),
)
const RAW: Record<string, string> = {
  ...(import.meta.glob('./help/*.md', { query: '?raw', import: 'default', eager: true }) as Record<string, string>),
  // The guides of the blocks with a source of their own, in the same order (`NN-id.md`).
  ...Object.fromEntries(BLOCK_DEFS.flatMap((b) => (b.guide ? [[`./help/${b.guide.file}`, b.guide.translations?.[lang()] ?? b.guide.text]] : []))),
  ...TRANSLATED,
}

/** The images of `help/images/`, by the path the guides give them (`images/x.png`). */
const IMAGES: Record<string, string> = Object.fromEntries(
  Object.entries(import.meta.glob('./help/images/*.{png,jpg,jpeg,gif,svg,webp}', { query: '?url', import: 'default', eager: true }) as Record<string, string>)
    .map(([file, url]) => [file.replace('./help/', ''), url]),
)
/** A block's guide also sees its own images. */
const imagesOf = (file: string) => ({ ...IMAGES, ...BLOCK_DEFS.find((b) => file === `./help/${b.guide?.file}`)?.guide?.images })

function read(file: string, raw: string): Guide {
  const id = /\/\d+-([\w-]+)\.md$/.exec(file)?.[1] ?? file
  const src = forPlatform(raw.replace(/\r\n?/g, '\n'))
  const m = /^---\n([\s\S]*?)\n---\n?([\s\S]*)$/.exec(src)
  const head = new Map<string, string>()
  for (const line of (m?.[1] ?? '').split('\n')) {
    const kv = /^(\w+):\s*(.*)$/.exec(line)
    if (kv) head.set(kv[1], kv[2].trim())
  }
  const list = (k: string) => (head.get(k) ?? '').split(',').map((s) => s.trim()).filter(Boolean)
  const content = (m?.[2] ?? src).trim()
  // The lead: the first paragraph; the body, the rest.
  const cut = content.search(/\n\s*\n/)
  const lead = cut < 0 ? content : content.slice(0, cut).trim()
  const body = cut < 0 ? '' : content.slice(cut).trim()
  const title = head.get('title') ?? id
  return { id, title, group: head.get('group') ?? 'Start', blocks: list('blocks') as BlockId[], related: list('related'), lead, body, images: imagesOf(file), text: `${title}\n${content}`.toLowerCase() }
}

/** The references, built from the app's own lists. */
const REFERENCES: Guide[] = [
  {
    id: 'ref-blocks', title: t('All blocks'), group: 'Reference', blocks: [], related: ['pages'],
    lead: t('Every block Prism can show, what it is for and where its data comes from. Add one to a page with Edit layout, then + Block.'),
    body: '', images: {},
    text: ['all blocks', t('All blocks'), ...ALL_BLOCKS.map((b) => `${t(BLOCK_LABELS[b])} ${t(BLOCK_HELP[b]?.purpose ?? '')} ${BLOCK_HELP[b]?.points.map((p) => t(p)).join(' ') ?? ''}`)].join('\n').toLowerCase(),
  },
  {
    id: 'ref-keys', title: t('All shortcuts'), group: 'Reference', blocks: [], related: ['keyboard'],
    lead: t('Every keyboard shortcut — the same list as the ? overlay.'),
    body: '', images: {},
    text: ['all shortcuts keyboard', t('All shortcuts'), ...SHORTCUTS.flatMap((s) => [s.title, ...s.keys.map(([k, w]) => `${k} ${w}`)])].join('\n').toLowerCase(),
  },
  {
    id: 'ref-settings', title: t('All settings'), group: 'Reference', blocks: [], related: ['settings'],
    lead: t('Every setting of config.toml, with what it does — the same text as the Settings view. A click opens its section there.'),
    body: '', images: {},
    text: ['all settings config.toml', t('All settings'), ...SCHEMA.flatMap((s) => [t(s.label), s.table, s.intro ? t(s.intro) : '', ...s.fields.map((f) => `${f.key} ${t(f.label)} ${f.hint ? t(f.hint) : ''}`)])].join('\n').toLowerCase(),
  },
]

export const guides: Guide[] = [
  ...Object.entries(RAW)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([file, raw]) => read(file, raw)),
  ...REFERENCES,
]
export const guideById = (id: string) => guides.find((g) => g.id === id)
/** The guide explaining a block: its "?" leads there. */
export const guideFor = (block: BlockId) => guides.find((g) => g.blocks.includes(block))

/** Occurrences of the query in each guide, those that have some. */
export function search(query: string): Map<string, number> {
  const q = query.trim().toLowerCase()
  const out = new Map<string, number>()
  if (!q) return out
  for (const g of guides) {
    const n = g.text.split(q).length - 1
    if (n) out.set(g.id, n)
  }
  return out
}
