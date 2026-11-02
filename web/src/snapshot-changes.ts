/**
 * The snapshot as changes to the last one (`prism_remote::snapshot::diff`): the phone's socket
 * and the window's `snapshot-changes` event send them, not the whole snapshot each time.
 */

/** One change of the snapshot: the value now at `path` (an object's key, an array's index), or the key removed. */
export interface Change {
  path: string[]
  value?: unknown
  delete?: boolean
}

type Json = Record<string, unknown>

/**
 * `changes` applied to `snap` without touching it: only the objects and arrays along each path
 * are copied, so that what did not change keeps its identity.
 */
export function applyChanges(snap: unknown, changes: Change[]): unknown {
  let root = snap
  for (const c of changes) {
    if (!c.path.length) {
      root = c.value
      continue
    }
    const copy = (o: unknown): Json => (Array.isArray(o) ? ([...o] as unknown as Json) : { ...((o && typeof o === 'object' ? o : {}) as Json) })
    const top = copy(root)
    let at = top
    for (const key of c.path.slice(0, -1)) {
      at[key] = copy(at[key])
      at = at[key] as Json
    }
    const last = c.path[c.path.length - 1]
    if (c.delete) delete at[last]
    else at[last] = c.value
    root = top
  }
  return root
}
