import { t } from '../../i18n.svelte'
import { live } from '../../live.svelte'
import { BLOCK_DEFS } from '../../blocks'
import { valueOf } from '../../settings.svelte'
import type { RepoBlock } from '../../bindings/RepoBlock'
import type { ForgeProvider } from '../../bindings/ForgeProvider'
import { profileForge } from '../../forge'

/**
 * The blocks that read repositories (`Snapshot.repo_blocks`: CI health, Renovate, Vulnerabilities,
 * a pack's), as Settings edits them: the repositories each one names in its key, pending edits
 * included, otherwise the one bare MR references name.
 */
export const repoBlocks = (): RepoBlock[] => live.snapshot?.repo_blocks ?? []

/** The names a block's key gives, as edited: one, several, or none. */
export function namedBy(b: RepoBlock): string[] {
  const v = valueOf<unknown>(b.key, null)
  if (Array.isArray(v)) return v.filter((n): n is string => typeof n === 'string' && !!n.trim())
  return typeof v === 'string' && v.trim() ? [v.trim()] : []
}

/** The repositories a block reads: those its key names, otherwise `bare` (the primary one). */
export const readsOf = (b: RepoBlock, bare: string) => (namedBy(b).length ? namedBy(b) : bare ? [bare] : [])

/** `~/x` for `/Users/me/x`: a working folder and a clone compare the same way. */
export const tilde = (p: string) => p.trim().replace(/\/+$/, '').replace(/^(\/Users|\/home)\/[^/]+/, '~')

type Row = Record<string, unknown>
const str = (v: unknown) => (typeof v === 'string' ? v : '')
/** The working folder (`[[workspaces]]`, as edited) a repository's clone is in. */
export function workspaceOf(r: Row): Row | undefined {
  const path = str(r.path)
  if (!path) return undefined
  return (valueOf<Row[] | null>(['workspaces'], null) ?? []).find((w) => !!str(w.dir).trim() && (tilde(path) === tilde(str(w.dir)) || tilde(path).startsWith(`${tilde(str(w.dir))}/`)))
}
/** A repository's forge: its own, otherwise its working folder's, otherwise the profile's. */
export function forgeOfRow(r: Row): ForgeProvider {
  const own = r.forge ?? workspaceOf(r)?.forge
  return own === 'github' || own === 'gitlab' ? own : profileForge()
}

/** The blocks that read `name` (their ids). */
export const readersOf = (name: string, bare: string) => repoBlocks().filter((b) => readsOf(b, bare).includes(name)).map((b) => b.id)

const CORE: Record<string, string> = { ci_health: 'CI health', renovate: 'Renovate', vulnerabilities: 'Vulnerabilities' }
/** A block's name: the core's, or a pack's block's label (by its id, or its source). */
export function repoBlockLabel(id: string): string {
  if (CORE[id]) return t(CORE[id])
  const def = BLOCK_DEFS.find((d) => d.source === id) ?? BLOCK_DEFS.find((d) => d.id === id)
  return def ? t(def.label) : id
}
