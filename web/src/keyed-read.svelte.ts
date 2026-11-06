import { hasCore } from './bridge'

/**
 * A list read from the Rust core per key (a repository's members, a tracker list's), kept for the
 * session: read again on the next `load` once `ttl` has passed; after a failure, not asked again
 * before `retry` (an `@` menu calls `load` at each key typed: a refused read must not become one
 * request per key).
 */
export function keyedRead<T>(read: (key: string) => Promise<T>, ttl = 3_600_000, retry = 60_000) {
  const state = $state({ byKey: {} as Record<string, T>, error: {} as Record<string, string>, loading: {} as Record<string, boolean> })
  const readAt: Record<string, number> = {}
  const failedAt: Record<string, number> = {}
  return {
    /** The list; `null` before its first read. */
    get: (key: string): T | null => state.byKey[key] ?? null,
    /** The last read's failure; empty: none. */
    error: (key: string): string => state.error[key] ?? '',
    async load(key: string) {
      const now = Date.now()
      if (!hasCore() || state.loading[key] || (key in state.byKey && now - readAt[key] < ttl) || now - (failedAt[key] ?? -Infinity) < retry) return
      state.loading[key] = true
      try {
        state.byKey[key] = await read(key)
        readAt[key] = Date.now()
        state.error[key] = ''
      } catch (e) {
        failedAt[key] = Date.now()
        state.error[key] = String(e)
      } finally {
        state.loading[key] = false
      }
    },
  }
}
