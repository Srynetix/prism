import { hasCore, invoke } from './bridge'
import type { PodView } from './types'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'
import type { PodInfo } from './bindings/PodInfo'
export type { PodInfo }

/**
 * Logs pane of a workload (Environments block): its pods, then a container's log, read
 * on demand from the cluster and re-read every few seconds while the pane is open
 * (except the "previous" log, frozen). Its last lines first, further back on demand (`loadOlder`, up to
 * the container's start). Nothing is kept once the pane is closed. A block can
 * open it on its own items (the Docker block's containers): they stand for the pods (`read`).
 */

interface PodLog {
  log: string
  truncated: boolean
  grown?: { from: string; same: number }
}

export interface WorkloadRef {
  /** A block's items rather than a cluster's pods (`openLogs`): each "pod" is an item's id, read by this. */
  read?: (id: string) => Promise<string>
  /** Lines `read` gives at most: a log that long was cut. */
  lines?: number
  context: string
  namespace: string
  deployment: string
  /** `cart-api · prod`: the pane's title. */
  label: string
}

export const podLogs = $state({
  open: false,
  target: null as WorkloadRef | null,
  pods: [] as PodInfo[],
  pod: null as string | null,
  container: null as string | null,
  /** Log of the container before its last restart (crash, OOMKilled). */
  previous: false,
  log: '',
  /** `log` made from the one before (`from`) by a poll: its first `same` characters the same. */
  grown: null as { from: string; same: number } | null,
  truncated: false,
  /** How far back the log was read (`OLDER` steps), its default tail when null. */
  depth: null as number | null,
  /** Older lines being read. */
  older: false,
  loading: false,
  error: null as string | null,
})

/** The browser preview has no access to the clusters. */
export const canShowPodLogs = hasCore

const POLL_MS = 5000
/** Lines read on each step back (0: up to the container's start). */
const OLDER = [10_000, 50_000, 0]
let timer: ReturnType<typeof setTimeout> | undefined
/** Incremented on each opening: a response from a previous opening is ignored. */
let generation = 0

/** Workload: its pods, then the log of the most telling one (not ready first, otherwise the most recent). */
/**
 * A block's items (a Compose project's containers…), `chosen` shown first: their logs through
 * `read`, which gives at most `lines` lines. An item's id is its "pod", its name what the list shows.
 */
export interface LogItem {
  id: string
  name: string
  running: boolean
  status: string
  created_at: string | null
}
export async function openLogs(label: string, items: LogItem[], chosen: string, read: (id: string) => Promise<string>, lines: number) {
  clearTimeout(timer)
  generation++
  const pods: PodInfo[] = items.map((c) => ({ name: c.id, status: c.status, ready: c.running, restarts: 0, started_at: c.created_at, containers: [c.name], last_exit: null }))
  Object.assign(podLogs, { open: true, target: { read, lines, context: '', namespace: label, deployment: '', label }, pods, pod: null, container: null, previous: false, log: '', grown: null, truncated: false, depth: null, older: false, error: null, loading: true })
  await selectPod(chosen)
}

/**
 * Pods block: a pod and the others of its workload, already in the snapshot (no listing). Their
 * containers are not known: the main one is read. A pod that exited before opens on its previous log.
 */
export async function openPods(context: string, namespace: string, workload: string, label: string, pods: PodView[], chosen: string) {
  clearTimeout(timer)
  generation++
  const infos: PodInfo[] = pods.map((p) => ({ name: p.name, status: p.status, ready: p.healthy, restarts: p.restarts, started_at: p.started_at, containers: [], last_exit: p.last_exit }))
  Object.assign(podLogs, { open: true, target: { context, namespace, deployment: workload, label }, pods: infos, pod: null, container: null, previous: false, log: '', grown: null, truncated: false, depth: null, older: false, error: null, loading: true })
  const pick = infos.find((p) => p.name === chosen)
  await selectPod(chosen, { previous: !!pick && !pick.ready && !!pick.last_exit })
}

export async function openWorkload(target: WorkloadRef) {
  clearTimeout(timer)
  const gen = ++generation
  Object.assign(podLogs, { open: true, target, pods: [], pod: null, container: null, previous: false, log: '', grown: null, truncated: false, depth: null, older: false, error: null, loading: true })
  try {
    const pods = await invoke<PodInfo[]>('workload_pods', { context: target.context, namespace: target.namespace, deployment: target.deployment })
    if (gen !== generation) return
    podLogs.pods = pods
    const pick = pods.find((p) => !p.ready) ?? pods[0]
    if (pick) await selectPod(pick.name, { previous: !pick.ready && !!pick.last_exit })
    else podLogs.loading = false
  } catch (e) {
    if (gen === generation) {
      podLogs.error = String(e)
      podLogs.loading = false
    }
  }
}

/** A pane closed lately as it was (recent-panels): the pods it listed, the same log chosen, read again. */
async function reopenPodLogs(was: { target: WorkloadRef; pods: PodInfo[]; pod: string | null; container: string | null; previous: boolean }) {
  clearTimeout(timer)
  generation++
  Object.assign(podLogs, { open: true, target: was.target, pods: was.pods, pod: null, container: null, previous: false, log: '', grown: null, truncated: false, depth: null, older: false, error: null, loading: true })
  if (was.pod && was.pods.some((p) => p.name === was.pod)) await selectPod(was.pod, { container: was.container, previous: was.previous })
  else podLogs.loading = false
}

export async function selectPod(name: string, opts: { container?: string | null; previous?: boolean } = {}) {
  const pod = podLogs.pods.find((p) => p.name === name)
  if (!pod) return
  if (podLogs.pod !== name) Object.assign(podLogs, { log: '', grown: null, depth: null })
  podLogs.pod = name
  podLogs.container = opts.container !== undefined ? opts.container : pod.containers.includes(podLogs.container ?? '') ? podLogs.container : (pod.containers[0] ?? null)
  if (opts.previous !== undefined) podLogs.previous = opts.previous && !!pod.last_exit
  await reload()
}

/** Incremented on each read: an older one answering late is ignored. */
let reads = 0

/** `whole`: read as far back as `depth` again, rather than only the end joined to what is shown. */
export async function reload(whole = false) {
  clearTimeout(timer)
  const t = podLogs.target
  if (!t || !podLogs.pod) return
  const gen = generation
  const seq = ++reads
  const key = `${podLogs.pod}/${podLogs.container}/${podLogs.previous}`
  podLogs.loading = true
  podLogs.error = null
  try {
    // A block's read gives its last `lines` lines: a log that long was cut.
    const item = async (read: (id: string) => Promise<string>) => {
      const log = await read(podLogs.pod!)
      return { log, truncated: log.split('\n').length > (t.lines ?? Infinity) }
    }
    const kube = (lines: number | null) =>
      invoke<PodLog>('pod_log', { context: t.context, namespace: t.namespace, pod: podLogs.pod, container: podLogs.container, previous: podLogs.previous, lines })
    let r: PodLog
    if (t.read) r = await item(t.read)
    else if (whole || podLogs.depth === null || !podLogs.log) r = await kube(podLogs.depth)
    else {
      // Read further back: polled, only its end is read again and joined to what is shown.
      const from = podLogs.log
      const joined = joinAt(from, (await kube(null)).log)
      r = joined === null ? await kube(podLogs.depth) : { log: joined.log, truncated: podLogs.truncated, grown: { from, same: joined.same } }
    }
    if (gen !== generation || seq !== reads || key !== `${podLogs.pod}/${podLogs.container}/${podLogs.previous}`) return
    podLogs.grown = r.grown ?? null
    podLogs.log = r.log
    podLogs.truncated = r.truncated
    if (!podLogs.previous) timer = setTimeout(() => podLogs.open && reload(), POLL_MS)
  } catch (e) {
    if (gen === generation && seq === reads) podLogs.error = String(e)
  } finally {
    if (gen === generation && seq === reads) podLogs.loading = false
  }
}

/**
 * `tail` (a log's last lines, read again) after `log`: where its first lines start again in `log`'s
 * end, the rest of it is new. Null when it starts nowhere (the container restarted): read it all again.
 */
export function join(log: string, tail: string): string | null {
  return joinAt(log, tail)?.log ?? null
}

/** `join`, and how many of the log's first characters stayed the same (all but its changed end). */
export function joinAt(log: string, tail: string): { log: string; same: number } | null {
  const next = tail.split('\n')
  if (next.at(-1) === '') next.pop()
  if (!next.length) return { log, same: log.length }
  // Where `tail` starts again is in `log`'s end, no longer than `tail`: only that is split (a log
  // read back to its start, 50 MB, was split and joined again whole every poll).
  const from = log.length - tail.length - 1
  const start = from <= 0 ? 0 : log.lastIndexOf('\n', from) + 1
  const old = log.slice(start).split('\n')
  if (old.at(-1) === '') old.pop()
  for (let k = Math.max(0, old.length - next.length); k < old.length; k++) {
    let same = true
    for (let j = 0; k + j < old.length && same; j++) same = old[k + j] === next[j]
    if (same) {
      const was = log.slice(start)
      const now = [...old.slice(0, k), ...next].join('\n') + (tail.endsWith('\n') ? '\n' : '')
      let n = 0
      while (n < was.length && n < now.length && was.charCodeAt(n) === now.charCodeAt(n)) n++
      return { log: log.slice(0, start) + now, same: start + n }
    }
  }
  return null
}

/** Further back in a cut log (scrolled to its top): the next `OLDER` step, up to the container's start. */
export async function loadOlder() {
  if (!podLogs.truncated || podLogs.older || podLogs.target?.read || !podLogs.pod) return
  const at = podLogs.depth === null ? -1 : OLDER.indexOf(podLogs.depth)
  if (at === OLDER.length - 1) return
  podLogs.depth = OLDER[at + 1]
  podLogs.older = true
  try {
    await reload(true)
  } finally {
    podLogs.older = false
  }
}

export function setPrevious(on: boolean) {
  podLogs.previous = on
  podLogs.log = ''
  podLogs.depth = null
  reload()
}

export function setContainer(c: string) {
  podLogs.container = c
  podLogs.log = ''
  podLogs.depth = null
  reload()
}

export function closePodLogs() {
  const { open, target, pods, pod, container, previous } = podLogs
  if (open && target) {
    const was = { target, pods, pod, container, previous }
    const which = [pod, container, previous && t('previous')].filter(Boolean).join(' · ')
    rememberPanel({ key: `pod-logs:${target.context}/${target.namespace}/${target.label}`, kind: 'pod-logs', title: t('Logs · {label}', { label: target.label }), hint: which, reopen: () => void reopenPodLogs(was) })
  }
  clearTimeout(timer)
  generation++
  Object.assign(podLogs, { open: false, target: null, pods: [], pod: null, container: null, log: '', depth: null, older: false, error: null, loading: false })
}
