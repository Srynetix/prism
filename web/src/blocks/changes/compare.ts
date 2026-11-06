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
  /** A crash of the same application on A after `a`, before its next deployment there. */
  crash: ChangeEvent | null
}

export interface Waiting {
  /** On A, not on B. */
  e: ChangeEvent
  crash: ChangeEvent | null
}

export interface Comparison {
  matches: Match[]
  /** Newest first. */
  waiting: Waiting[]
}

const at = (e: ChangeEvent) => Date.parse(e.at)
const DEPLOYS: ReadonlySet<ChangeKind> = new Set(['sync', 'rollout', 'merged'])

/** An application's names, the environment taken off (`orders-api-prod` → `orders-api`), and its Deployments. */
function names(e: ChangeEvent): Set<string> {
  const out = new Set(e.workloads.map((w) => w.toLowerCase()))
  const app = e.app?.toLowerCase()
  if (app) {
    out.add(app)
    const env = e.env?.toLowerCase()
    if (env) for (const sep of ['-', '_', '.']) {
      if (app.endsWith(sep + env)) out.add(app.slice(0, -(env.length + 1)))
      if (app.startsWith(env + sep)) out.add(app.slice(env.length + 1))
    }
  }
  return out
}

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

/** An image's tag (`api:2.15.0` → `2.15.0`); `latest` tells nothing. */
const tags = (e: ChangeEvent) => e.images.map((i) => i.slice(i.lastIndexOf(':') + 1)).filter((tg) => tg && tg !== 'latest')
const sharedTag = (x: ChangeEvent, y: ChangeEvent) => {
  const ys = tags(y)
  return tags(x).find((tg) => ys.includes(tg)) ?? null
}

/** How `x` (on A) and `y` (on B) are the same change: revision or MR first, the image otherwise. */
function link(x: ChangeEvent, y: ChangeEvent, images: boolean): { basis: Basis; label: string } | null {
  if (x.kind !== y.kind) return null
  if (x.kind === 'merged') return x.mr && y.mr && x.mr.iid === y.mr.iid && x.mr.repo === y.mr.repo ? { basis: 'mr', label: `!${x.mr.iid}` } : null
  if (!sameApp(x, y)) return null
  if (sameRevision(x.revision, y.revision)) return { basis: 'revision', label: x.revision! }
  if (!images) return null
  const tag = sharedTag(x, y)
  const image = tag && (y.images.find((i) => i.endsWith(`:${tag}`)) ?? x.images.find((i) => i.endsWith(`:${tag}`)))
  return image ? { basis: 'image', label: image } : null
}

/** A crash of `e`'s application on its environment after it, until the next deployment of the same kind there. */
export function crashAfter(e: ChangeEvent, events: ChangeEvent[]): ChangeEvent | null {
  const mine = events.filter((x) => x.env === e.env && at(x) > at(e) && sameApp(x, e))
  const next = mine.filter((x) => x.kind === e.kind && x.id !== e.id).reduce((m, x) => Math.min(m, at(x)), Infinity)
  return mine.filter((x) => isCrash(x) && at(x) < next).sort((p, q) => at(p) - at(q))[0] ?? null
}

/**
 * `a` against `b`, over the whole journal: each change's first arrival on each side linked
 * (oldest first), and, per application, what A deployed after the last of its changes B got.
 */
export function compare(events: ChangeEvent[], a: string, b: string): Comparison {
  const sorted = [...events].sort((p, q) => at(p) - at(q))
  const onA = sorted.filter((e) => e.env === a && DEPLOYS.has(e.kind))
  const onB = sorted.filter((e) => e.env === b && DEPLOYS.has(e.kind))
  const matches: Match[] = []
  const used = new Set<string>()
  const known = (y: ChangeEvent, l: { basis: Basis; label: string }) =>
    matches.some((m) => (l.basis === 'image' ? sameApp(m.b, y) && sharedTag(m.b, y) !== null : m.basis === l.basis && link(m.b, y, false) !== null))
  // Revisions and MRs first, then the images of what is left: a sync and its rollout carry the same image.
  for (const images of [false, true]) {
    for (const y of onB) {
      if (used.has(y.id)) continue
      for (const x of onA) {
        if (used.has(x.id)) continue
        const l = link(x, y, images)
        if (!l || (images && l.basis !== 'image')) continue
        if (!known(y, l)) {
          matches.push({ a: x, b: y, ...l, delay: at(y) - at(x), crash: crashAfter(x, sorted) })
          used.add(x.id)
        }
        used.add(y.id)
        break
      }
    }
  }
  // Waiting: syncs (rollouts for an application Argo CD does not deploy) after the last one B got.
  const synced = (x: ChangeEvent) => onA.some((s) => s.kind === 'sync' && sameApp(s, x))
  const candidates = onA.filter((x) => x.kind === 'sync' || (x.kind === 'rollout' && !synced(x)))
  const reached = (x: ChangeEvent) => onB.some((y) => link(x, y, true))
  const waiting: Waiting[] = []
  for (const x of candidates) {
    const later = candidates.filter((z) => z.kind === x.kind && sameApp(z, x) && at(z) >= at(x))
    if (!later.some(reached)) waiting.push({ e: x, crash: crashAfter(x, sorted) })
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

/** The pair Compare opens on: `[changes] compare`, else the environment shown and the next one in the roles' order. */
export function defaultPair(envs: string[], setting: string[], current: string | null): [string, string] | null {
  if (setting.length >= 2 && setting[0] !== setting[1]) return [setting[0], setting[1]]
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

/** An application's name without its environment (`orders-api-prod` → `orders-api`). */
export function appOf(e: ChangeEvent): string {
  const app = e.app ?? e.workloads[0] ?? e.title
  const env = e.env?.toLowerCase()
  if (!env) return app
  for (const sep of ['-', '_', '.']) {
    if (app.toLowerCase().endsWith(sep + env)) return app.slice(0, -(env.length + 1))
    if (app.toLowerCase().startsWith(env + sep)) return app.slice(env.length + 1)
  }
  return app
}
