import { hasCore, invoke } from './bridge'
import { mrArgs, mrKey, type MrRef } from './mr-ref'
import type { MrDetail } from './bindings/MrDetail'
import type { MrDiff } from './bindings/MrDiff'
import type { MrCommit } from './bindings/MrCommit'

/**
 * Review of an MR in its pane: description, discussions, approvals and diff, read on
 * demand (never collected) and kept for the session, per MR; the detail is re-read
 * when the MR moves (`updated_at`) and after each action.
 */
export type { MrNote as Note } from './bindings/MrNote'
export type { Discussion } from './bindings/Discussion'
export type { MrDetail } from './bindings/MrDetail'
export type { ApprovalEvent } from './bindings/ApprovalEvent'
export type { FileDiff } from './bindings/FileDiff'
export type { MrDiff } from './bindings/MrDiff'
export type { MrCommit } from './bindings/MrCommit'

interface Entry<T> {
  data: T | null
  error: string | null
  loading: boolean
  /** The MR's `updated_at` at read time: re-read if the MR has moved since. */
  at: string
}

/** Per MR (`mrKey`). */
export const review = $state({
  detail: {} as Record<string, Entry<MrDetail>>,
  diff: {} as Record<string, Entry<MrDiff>>,
  commits: {} as Record<string, Entry<MrCommit[]>>,
})

/** The browser preview has no access to GitLab. */
export const canReview = hasCore

async function load<T>(store: Record<string, Entry<T>>, command: string, mr: MrRef, at: string, force: boolean) {
  const key = mrKey(mr)
  const cur = store[key]
  if (!canReview() || cur?.loading || (!force && cur && cur.at === at && !cur.error)) return
  store[key] = { data: cur?.data ?? null, error: null, loading: true, at }
  try {
    const data = await invoke<T>(command, mrArgs(mr))
    store[key] = { data, error: null, loading: false, at }
  } catch (e) {
    store[key] = { data: cur?.data ?? null, error: String(e), loading: false, at }
  }
}

export const loadDetail = (mr: MrRef, at: string, force = false) => load(review.detail, 'mr_detail', mr, at, force)
export const loadDiff = (mr: MrRef, at: string, force = false) => load(review.diff, 'mr_diff', mr, at, force)
/** Its commits, newest first. */
export const loadCommits = (mr: MrRef, at: string, force = false) => load(review.commits, 'mr_commits', mr, at, force)
/** A file of the MR, whole, on its source branch: the Markdown preview of the diff. */
export const mrFile = (mr: MrRef, path: string) => invoke<string>('mr_file', { ...mrArgs(mr), path })
/** An image of the MR's diff as a `data:` URL: on its source branch, or (`old`) on its target branch. */
export const mrImage = (mr: MrRef, path: string, old: boolean) => invoke<string>('mr_image', { ...mrArgs(mr), path, old })

/** Approve, or withdraw my approval; the snapshot follows on the Rust side, the detail is re-read. */
export async function approve(mr: MrRef, on: boolean, at: string) {
  await invoke<void>('approve_mr', { ...mrArgs(mr), approve: on })
  await loadDetail(mr, at, true)
}

/** Request changes, `body` its comment (required on GitHub), or take my request back (GitLab). */
export async function requestChanges(mr: MrRef, on: boolean, body: string, at: string) {
  await invoke<void>('request_changes_mr', { ...mrArgs(mr), on, body })
  await loadDetail(mr, at, true)
}

/** Comment on the MR, or reply to a thread; the detail is re-read to display it. */
export async function comment(mr: MrRef, body: string, at: string, discussion?: string) {
  await invoke<void>('comment_mr', { ...mrArgs(mr), body, discussion: discussion ?? null })
  await loadDetail(mr, at, true)
}

/** Line targeted by a diff comment: both numbers for an unchanged line. */
export interface LineRef {
  old_path: string
  new_path: string
  old_line: number | null
  new_line: number | null
  /** Where it sits in both files (`positionOf`), and on several lines the first one (this one is the last). */
  pos?: [number, number]
  start?: { old_line: number | null; new_line: number | null; pos: [number, number] }
}

/**
 * Where a hunk's line sits in the old file and the new one, as GitLab numbers it for a range: an
 * unchanged line by its two numbers, an added one with the old file's next line, a removed one
 * with the new file's next line.
 */
export function positionOf(lines: DiffLine[], i: number): [number, number] {
  const l = lines[i]
  let o = l.old
  let n = l.new
  for (let k = i - 1; k >= 0 && (o === null || n === null); k--) {
    const p = lines[k]
    if (p.kind === 'hunk') {
      const h = /^@@ -(\d+)(?:,\d+)? \+(\d+)/.exec(p.text)
      o ??= h ? Number(h[1]) : 1
      n ??= h ? Number(h[2]) : 1
      break
    }
    if (o === null && p.old !== null) o = p.old + 1
    if (n === null && p.new !== null) n = p.new + 1
  }
  return [o ?? 1, n ?? 1]
}

/** Two lines of a file's diff in the same hunk, none shown only for context between them: a range a forge takes. */
export function sameHunk(lines: DiffLine[], a: number, b: number): boolean {
  const [lo, hi] = a < b ? [a, b] : [b, a]
  if (lo < 0 || hi >= lines.length) return false
  for (let k = lo; k <= hi; k++) if (lines[k].kind === 'hunk' || lines[k].extra) return false
  return true
}

/** New thread on a diff line; the detail is re-read to display it under the line. */
export async function commentLine(mr: MrRef, line: LineRef, body: string, at: string) {
  await invoke<void>('comment_line', { ...mrArgs(mr), line, body })
  await loadDetail(mr, at, true)
}

/** Resolve a thread, or reopen it. */
export async function resolveThread(mr: MrRef, discussion: string, resolved: boolean, at: string) {
  await invoke<void>('resolve_thread', { ...mrArgs(mr), discussion, resolved })
  await loadDetail(mr, at, true)
}

/** Web address of an MR's project (`…/group/project`), for its uploaded files. */
export const projectWeb = (mrUrl: string) => mrUrl.split('/-/')[0].replace(/\/pull\/\d+$/, '')

/** Line of a unified diff, numbered in the old and the new file. */
export interface DiffLine {
  kind: 'add' | 'del' | 'ctx' | 'hunk'
  text: string
  old: number | null
  new: number | null
  /** An unchanged line outside the diff, shown on demand (diff-rows.ts `expandLines`). */
  extra?: boolean
}

export function parseDiff(diff: string): DiffLine[] {
  const out: DiffLine[] = []
  let o = 0
  let n = 0
  for (const raw of diff.split('\n')) {
    const h = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)$/.exec(raw)
    if (h) {
      o = Number(h[1])
      n = Number(h[2])
      out.push({ kind: 'hunk', text: raw, old: null, new: null })
    } else if (raw.startsWith('+')) out.push({ kind: 'add', text: raw.slice(1), old: null, new: n++ })
    else if (raw.startsWith('-')) out.push({ kind: 'del', text: raw.slice(1), old: o++, new: null })
    else if (raw.startsWith('\\')) continue
    else if (raw !== '' || out.length) out.push({ kind: 'ctx', text: raw.slice(1), old: o++, new: n++ })
  }
  // The last empty line of a diff is only a final newline.
  if (out.at(-1)?.kind === 'ctx' && out.at(-1)?.text === '') out.pop()
  return out
}

export const diffStats = (diff: string) => {
  let add = 0
  let del = 0
  for (const l of diff.split('\n')) {
    if (l.startsWith('+')) add++
    else if (l.startsWith('-')) del++
  }
  return { add, del }
}
