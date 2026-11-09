/**
 * Overlaps between agents and worktrees (`Snapshot.overlaps`, found by the Rust core): who is on
 * each side, which ones show (`[agents.overlaps] level`, those ignored), who adjusts, and the
 * prompt that asks it to.
 */
import type { Agent, LineRange, MergeRequest, Overlap, OverlapFile, Snapshot } from './types'
import type { AgentDiff } from './bindings/AgentDiff'
import { pref, setPref } from './layout.svelte'
import { mrForSession } from './rules'
import { repoName } from './repositories'
import { t } from './i18n.svelte'
import { baseName } from './platform'

/** The folders an agent works in: its worktree (or folder), those it adopted, those of its other repositories. */
export const agentPaths = (a: Agent) => [a.worktree ?? a.cwd, ...a.adopted, ...(a.also ?? [])]

/** The agent working in `path`: a live one first, the most recent. */
export function agentAt(s: Snapshot, path: string): Agent | undefined {
  const here = (s.agents ?? []).filter((a) => agentPaths(a).includes(path))
  const rank = (a: Agent) => (a.state === 'gone' ? 0 : 1)
  return here.sort((x, y) => rank(y) - rank(x) || y.created_at.localeCompare(x.created_at))[0]
}

export type Side = 'a' | 'b'
export const other = (side: Side): Side => (side === 'a' ? 'b' : 'a')
export const pathOf = (o: Overlap, side: Side) => (side === 'a' ? o.a : o.b)
export const branchOf = (o: Overlap, side: Side) => (side === 'a' ? o.a_branch : o.b_branch)

/** A side's name: its agent's title, otherwise its branch, otherwise its folder's name. */
export function sideName(s: Snapshot, o: Overlap, side: Side): string {
  const path = pathOf(o, side)
  return agentAt(s, path)?.title || branchOf(o, side) || baseName(path) || path
}

/** An overlap's id, the same while its two sides are. */
export const overlapId = (o: Pick<Overlap, 'repo' | 'a' | 'b'>) => `${o.repo}|${o.a}|${o.b}`

const IGNORED = 'overlaps.ignored'
const ignoredMap = () => pref<Record<string, string>>(IGNORED, {})
/** Ignored from its panel, until its hunks change (its key moves). */
export const isIgnored = (o: Overlap) => ignoredMap()[overlapId(o)] === o.key
export function ignore(s: Snapshot, o: Overlap) {
  // Those gone or changed since are dropped as this one is added.
  const now = new Map((s.overlaps?.pairs ?? []).map((p) => [overlapId(p), p.key]))
  const kept = Object.fromEntries(Object.entries(ignoredMap()).filter(([id, key]) => now.get(id) === key))
  setPref(IGNORED, { ...kept, [overlapId(o)]: o.key })
}

const settings = (s: Snapshot) => s.agents_settings?.overlaps
/** Those shown as chips: from `level`, not ignored. */
export function visibleOverlaps(s: Snapshot): Overlap[] {
  const cfg = settings(s)
  if (cfg && !cfg.enabled) return []
  const linesOnly = cfg?.level === 'lines'
  return (s.overlaps?.pairs ?? []).filter((o) => !isIgnored(o) && (!linesOnly || o.level === 'lines'))
}

/** An overlap as one of its sides sees it: the other side, by name. */
export interface Seen {
  o: Overlap
  side: Side
  otherName: string
}

/**
 * The overlaps a set of folders (an agent's, a worktree's) is in. A merged side (`a` of a merged
 * one) has nothing left to do: only the side that has to rebase sees it.
 */
export function overlapsAt(s: Snapshot, paths: string[]): Seen[] {
  const out: Seen[] = []
  for (const o of visibleOverlaps(s)) {
    for (const side of ['a', 'b'] as Side[]) {
      if (o.merged && side === 'a') continue
      if (paths.includes(pathOf(o, side))) out.push({ o, side, otherName: sideName(s, o, other(side)) })
    }
  }
  return out
}

export const overlapsOfAgent = (s: Snapshot, a: Agent) => overlapsAt(s, agentPaths(a))

/** The highest level among them: the chip's tone. */
export const topLevel = (seen: Seen[]) => (seen.some((x) => x.o.level === 'lines') ? 'lines' : 'file')

/** A side's open MR (or its last one). */
export function sideMr(s: Snapshot, o: Overlap, side: Side): MergeRequest | undefined {
  return mrForSession(s, { worktree: pathOf(o, side), branch: branchOf(o, side) ?? null })
}

/**
 * Who adjusts by default: the side without an MR (the other is already in review), otherwise the
 * one whose MR is the youngest; after a merge, the one left to rebase.
 */
export function whoAdjusts(s: Snapshot, o: Overlap): Side {
  if (o.merged) return 'b'
  const [ma, mb] = [sideMr(s, o, 'a'), sideMr(s, o, 'b')]
  if (!ma !== !mb) return ma ? 'b' : 'a'
  if (ma && mb) return (ma.created_at ?? '') > (mb.created_at ?? '') ? 'a' : 'b'
  return 'b'
}

/**
 * `10–14`, `10`, `after line 10`: in the interface's language, or in `lang` for a prompt (in
 * `[agents] prompt_language`, not the interface's).
 */
export function rangeText(r: LineRange, lang?: string): string {
  const whole = r.len >= 4_000_000_000
  if (lang === undefined) {
    if (whole) return t('the whole file')
    if (r.len === 0) return t('after line {n}', { n: r.start })
  } else {
    if (whole) return lang === 'fr' ? 'tout le fichier' : 'the whole file'
    if (r.len === 0) return lang === 'fr' ? `après la ligne ${r.start}` : `after line ${r.start}`
  }
  return r.len === 1 ? `${r.start}` : `${r.start}–${r.start + r.len - 1}`
}

export function rangesText(rs: LineRange[], lang?: string): string {
  return rs.map((r) => rangeText(r, lang)).join(', ')
}

/** The files and lines shared, for a tooltip. */
export function filesSummary(files: OverlapFile[], side: Side): string {
  return files.map((f) => (f.level === 'lines' ? `${f.path} · ${t('lines {r}', { r: rangesText(side === 'a' ? f.a : f.b) })}` : `${f.path} · ${t('other lines')}`)).join('\n')
}

/** The hunks of a diff's file whose base lines meet `ranges` (all of them for a file-level overlap). */
export function hunksOf(diff: AgentDiff | null | undefined, file: OverlapFile, side: Side): string[] {
  const f = diff?.files.find((d) => d.old_path === file.path || d.new_path === file.path)
  if (!f?.diff) return []
  const hunks = f.diff.split(/\n(?=@@ )/).map((h) => h.replace(/\n$/, ''))
  if (file.level === 'file') return hunks
  const ranges = side === 'a' ? file.a : file.b
  return hunks.filter((h) => {
    const m = /^@@ -(\d+)(?:,(\d+))?/.exec(h)
    if (!m) return false
    const start = Number(m[1])
    const end = start + Math.max(Number(m[2] ?? 1), 1) - 1
    return ranges.some((r) => r.start <= end + 1 && start <= r.start + Math.max(r.len, 1))
  })
}

/**
 * The prompt asking side `to` to adjust to the other: what the other changes in their shared
 * files, its hunks joined, in `[agents] prompt_language`.
 */
export function adjustPrompt(s: Snapshot, o: Overlap, to: Side, otherDiff: AgentDiff | null | undefined, lang: string): string {
  const from = other(to)
  const name = sideName(s, o, from)
  const fr = lang === 'fr'
  const lines = o.files.map((f) => {
    const theirs = from === 'a' ? f.a : f.b
    if (f.level === 'lines') return fr ? `- \`${f.path}\`, lignes ${rangesText(theirs, 'fr')}` : `- \`${f.path}\`, lines ${rangesText(theirs, 'en')}`
    return fr ? `- \`${f.path}\` (d’autres lignes)` : `- \`${f.path}\` (other lines)`
  })
  const head = o.merged
    ? fr
      ? `« ${name} » a été mergé et modifiait les mêmes fichiers que toi :`
      : `"${name}" was merged and changed the same files as you:`
    : fr
      ? `L’agent « ${name} » modifie aussi :`
      : `The agent "${name}" also changes:`
  const ask = o.merged
    ? fr
      ? `Rebase-toi sur origin/${o.base} et résous ce qui se recoupe, ou dis-moi si c’est incompatible.`
      : `Rebase on origin/${o.base} and resolve what overlaps, or tell me if it is incompatible.`
    : fr
      ? 'Adapte-toi à ses changements, ou dis-moi si c’est incompatible.'
      : 'Adapt to its changes, or tell me if they are incompatible.'
  const hunks = o.files.flatMap((f) => {
    const hs = hunksOf(otherDiff, f, from)
    return hs.length ? [`--- ${f.path}\n${hs.join('\n')}`] : []
  })
  const diff = hunks.length ? `\n\n${fr ? 'Son diff (fichiers partagés) :' : 'Its diff (shared files):'}\n\n\`\`\`diff\n${hunks.join('\n')}\n\`\`\`` : ''
  return `${head}\n${lines.join('\n')}\n\n${ask}${diff}`
}

/**
 * The overlaps of an MR's branch, when the other side has one too (or was merged): on the MRs'
 * rows, where the order of the merges is decided.
 */
export function overlapsOfMr(s: Snapshot, mr: MergeRequest): Seen[] {
  if (mr.state !== 'opened') return []
  const repo = repoName(mr.repo, s)
  return visibleOverlaps(s).flatMap((o) =>
    (['a', 'b'] as Side[])
      .filter((side) => o.repo === repo && branchOf(o, side) === mr.source_branch && !(o.merged && side === 'a'))
      .filter((side) => o.merged || !!sideMr(s, o, other(side)))
      .map((side) => ({ o, side, otherName: sideName(s, o, other(side)) })),
  )
}
