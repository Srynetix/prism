import { t } from '../../i18n.svelte'
import { isCrash, type ChangeEvent, type ChangeKind } from './changes'

/**
 * Two environments on the same axis (T-008): the same change on both linked (by its Argo CD
 * revision or its merge request, otherwise by the image the same application runs), what went on
 * the first and not yet on the second, and a crash on the first that the second got anyway.
 * Only the journal: nothing read anew.
 */

/** What links a change on A to its arrival on B. */
export type Basis = 'revision' | 'mr' | 'image'

export interface Match {
  /** Its first deployment on A, and on B. */
  a: ChangeEvent
  b: ChangeEvent
  basis: Basis
  /** `api:2.15.0` for an image, the revision or `!412` otherwise. */
  label: string
  /** `b` after `a`, in ms (negative: B had it first). */
  delay: number
  /** A crash of the same application on A after `a` (`crashAfter`). */
  crash: ChangeEvent | null
}

export interface Waiting {
  /** An application's newest change on A that B has not got. */
  e: ChangeEvent
  crash: ChangeEvent | null
  /** Its older changes on A, B has not got either, newest first. */
  earlier: ChangeEvent[]
}

export interface Comparison {
  matches: Match[]
  /** One per application, newest first. */
  waiting: Waiting[]
}

const at = (e: ChangeEvent) => Date.parse(e.at)
const DEPLOYS: ReadonlySet<ChangeKind> = new Set(['sync', 'rollout', 'merged'])
const MIN = 60_000
/** A crash is put on a change at most this long after it. */
const CRASH_WINDOW = 24 * 60 * MIN

/** A name without its environment (`orders-api-prod` → `orders-api`). */
function strip(name: string, env: string | undefined): string {
  const e = env?.toLowerCase()
  if (!e) return name
  const n = name.toLowerCase()
  for (const sep of ['-', '_', '.']) {
    if (n.endsWith(sep + e)) return name.slice(0, -(e.length + 1))
    if (n.startsWith(e + sep)) return name.slice(e.length + 1)
  }
  return name
}

/** An application's names, the environment taken off, and its Deployments; lowercase. */
const namesCache = new WeakMap<ChangeEvent, Set<string>>()
function names(e: ChangeEvent): Set<string> {
  let out = namesCache.get(e)
  if (out) return out
  out = new Set(e.workloads.map((w) => w.toLowerCase()))
  if (e.app) {
    out.add(e.app.toLowerCase())
    out.add(strip(e.app, e.env).toLowerCase())
  }
  namesCache.set(e, out)
  return out
}

/** An application's name without its environment (`orders-api-prod` → `orders-api`). */
export const appOf = (e: ChangeEvent): string => strip(e.app ?? e.workloads[0] ?? e.title, e.env)

/** The same application on two environments. */
export function sameApp(x: ChangeEvent, y: ChangeEvent): boolean {
  const a = names(x)
  for (const n of names(y)) if (a.has(n)) return true
  return false
}

/** Two short revisions of the same commit (one may be longer). */
const sameRevision = (x?: string, y?: string) => {
  if (!x || !y) return false
  const [a, b] = [x.toLowerCase(), y.toLowerCase()]
  return Math.min(a.length, b.length) >= 7 && (a.startsWith(b) || b.startsWith(a))
}

/**
 * The application's own images, as `name` and `tag`: `registry:5000/team/api:2.15.0` → `api`,
 * `2.15.0`. An image without a tag, `latest`, or another one (a sidecar) tells nothing.
 */
function ownImages(e: ChangeEvent): { name: string; tag: string; image: string }[] {
  const mine = names(e)
  return e.images.flatMap((image) => {
    const colon = image.lastIndexOf(':')
    if (colon < 0 || image.slice(colon + 1).includes('/')) return []
    const tag = image.slice(colon + 1)
    const repo = image.slice(0, colon)
    const name = strip(repo.slice(repo.lastIndexOf('/') + 1), e.env).toLowerCase()
    return tag && tag !== 'latest' && mine.has(name) ? [{ name, tag, image }] : []
  })
}
const sharedImage = (x: ChangeEvent, y: ChangeEvent) => {
  const ys = ownImages(y)
  for (const i of ownImages(x)) {
    const j = ys.find((j) => j.name === i.name && j.tag === i.tag)
    if (j) return j.image
  }
  return null
}

/** How `x` (on A) and `y` (on B) are the same change: revision or MR first, the image otherwise. */
function link(x: ChangeEvent, y: ChangeEvent, images: boolean): { basis: Basis; label: string } | null {
  if (x.kind !== y.kind) return null
  if (x.kind === 'merged') return x.mr && y.mr && x.mr.iid === y.mr.iid && x.mr.repo === y.mr.repo ? { basis: 'mr', label: `!${x.mr.iid}` } : null
  if (!sameApp(x, y)) return null
  if (sameRevision(x.revision, y.revision)) return { basis: 'revision', label: x.revision! }
  if (!images) return null
  const image = sharedImage(x, y)
  return image ? { basis: 'image', label: image } : null
}

/** Events by each of their names (an MR by its key), to look only at the same application's. */
function index(events: ChangeEvent[]): Map<string, ChangeEvent[]> {
  const out = new Map<string, ChangeEvent[]>()
  const add = (k: string, e: ChangeEvent) => {
    const l = out.get(k)
    if (l) {
      if (l[l.length - 1] !== e) l.push(e)
    } else out.set(k, [e])
  }
  for (const e of events) {
    if (e.kind === 'merged' && e.mr) add(`mr:${e.mr.repo ?? ''}!${e.mr.iid}`, e)
    else for (const n of names(e)) add(n, e)
  }
  return out
}

/** The events of `idx` that may be `e`'s application (or MR), oldest first. */
function near(idx: Map<string, ChangeEvent[]>, e: ChangeEvent): ChangeEvent[] {
  if (e.kind === 'merged') return e.mr ? (idx.get(`mr:${e.mr.repo ?? ''}!${e.mr.iid}`) ?? []) : []
  const lists = [...names(e)].map((n) => idx.get(n) ?? []).filter((l) => l.length)
  if (lists.length <= 1) return lists[0] ?? []
  return [...new Set(lists.flat())].sort((p, q) => at(p) - at(q))
}

/**
 * A crash of `e`'s application on its environment after it: within 24 h, and before anything
 * else deployed it there (its next change of the same kind, or a rollout of another image).
 */
export function crashAfter(e: ChangeEvent, events: ChangeEvent[]): ChangeEvent | null {
  const end = at(e) + CRASH_WINDOW
  const mine = events.filter((x) => x.env === e.env && at(x) > at(e) && at(x) <= end && sameApp(x, e)).sort((p, q) => at(p) - at(q))
  for (const x of mine) {
    if (isCrash(x)) return x
    const redeploy = (x.kind === e.kind && x.id !== e.id) || (x.kind === 'rollout' && at(x) - at(e) > 10 * MIN && !sharedImage(x, e))
    if (redeploy) return null
  }
  return null
}

/**
 * `a` against `b`, over the whole journal: each change's first arrival on each side linked
 * (oldest first), and, per application B runs, what A deployed after the last change B got.
 */
export function compare(events: ChangeEvent[], a: string, b: string): Comparison {
  const sorted = [...events].sort((p, q) => at(p) - at(q))
  const envA = sorted.filter((e) => e.env === a)
  const onA = envA.filter((e) => DEPLOYS.has(e.kind))
  const onB = sorted.filter((e) => e.env === b && DEPLOYS.has(e.kind))
  const idxA = index(onA)
  const idxB = index(onB)
  const idxCrashA = index(envA.filter(isCrash).concat(onA.filter((e) => e.kind !== 'merged')))
  const crashOf = (x: ChangeEvent) => crashAfter(x, near(idxCrashA, x))
  const matches: Match[] = []
  const used = new Set<string>()
  const known = (y: ChangeEvent, l: { basis: Basis; label: string }) =>
    matches.some((m) => (l.basis === 'image' ? sameApp(m.b, y) && sharedImage(m.b, y) !== null : m.basis === l.basis && link(m.b, y, false) !== null))
  // Revisions and MRs first, then the images of what is left: a sync and its rollout carry the same image.
  for (const images of [false, true]) {
    for (const y of onB) {
      if (used.has(y.id)) continue
      for (const x of near(idxA, y)) {
        if (used.has(x.id)) continue
        const l = link(x, y, images)
        if (!l || (images && l.basis !== 'image')) continue
        if (!known(y, l)) {
          matches.push({ a: x, b: y, ...l, delay: at(y) - at(x), crash: crashOf(x) })
          used.add(x.id)
        }
        used.add(y.id)
        break
      }
    }
  }
  // Waiting: per application B runs, A's syncs (rollouts for an application Argo CD does not
  // deploy) after the last one B got; an application B never ran is not waiting for it.
  const synced = (x: ChangeEvent) => near(idxA, x).some((s) => s.kind === 'sync')
  const apps = new Map<string, ChangeEvent[]>()
  for (const x of onA) {
    if (!(x.kind === 'sync' || (x.kind === 'rollout' && !synced(x)))) continue
    const k = appOf(x).toLowerCase()
    apps.set(k, [...(apps.get(k) ?? []), x])
  }
  const waiting: Waiting[] = []
  for (const list of apps.values()) {
    const theirs = near(idxB, list[0])
    if (!theirs.some((y) => y.kind !== 'merged')) continue
    const reached = (x: ChangeEvent) => theirs.some((y) => link(x, y, true))
    const out: ChangeEvent[] = []
    for (const x of [...list].reverse()) {
      if (reached(x)) break
      out.push(x)
    }
    if (out.length) waiting.push({ e: out[0], crash: crashOf(out[0]), earlier: out.slice(1) })
  }
  return { matches: matches.sort((p, q) => at(p.a) - at(q.a)), waiting: waiting.sort((p, q) => at(q.e) - at(p.e)) }
}

/** An environment's place in the roles' order: dev → staging → preprod → prod. */
export function rank(env: string): number {
  const n = env.toLowerCase()
  if (/pre-?prod|uat/.test(n)) return 2
  if (/(^|[^a-z])(dev|develop|development|local|sandbox)([^a-z]|$)/.test(n)) return 0
  if (/stag|stg|qa|test|recette|int/.test(n)) return 1
  if (/prod|prd|live/.test(n)) return 3
  return 1.5
}

/** The pair Compare opens on: `[changes] compare` when both are known, else the environment shown and the next one in the roles' order. */
export function defaultPair(envs: string[], setting: string[], current: string | null): [string, string] | null {
  if (setting.length >= 2 && setting[0] !== setting[1] && envs.includes(setting[0]) && envs.includes(setting[1])) return [setting[0], setting[1]]
  if (envs.length < 2) return null
  const order = envs.map((e, i) => ({ e, i })).sort((x, y) => rank(x.e) - rank(y.e) || x.i - y.i).map((x) => x.e)
  const i = current ? order.indexOf(current) : -1
  if (i < 0) return [order[0], order[1]]
  return i + 1 < order.length ? [order[i], order[i + 1]] : [order[i - 1], order[i]]
}

/** A delay: `45 min`, `20 h`, `2 d 4 h`. */
export function span(ms: number): string {
  const m = Math.round(Math.abs(ms) / 60_000)
  if (m < 60) return t('{n} min', { n: m })
  const h = Math.round(m / 60)
  if (h < 24) return t('{h} h', { h })
  const d = Math.floor(h / 24)
  return h % 24 ? t('{d} d {h} h', { d, h: h % 24 }) : t('{d} d', { d })
}
