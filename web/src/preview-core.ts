import type { Transport } from './bridge'

/**
 * Development only: `/?core` in the browser preview answers as if a core were there, so the views
 * that need one (Agents, the agents' panels) show too, on the preview's snapshot
 * (`dev-snapshot.json`). Each other call is refused, as a core out of reach: to look at the
 * views' layout at any width (`scripts/overlaps.mjs`), not to use them.
 */
export function previewCore(): Transport {
  const snapshot = () => fetch('/dev-snapshot.json', { cache: 'no-store' }).then((r) => r.json())
  return {
    async invoke<T>(command: string): Promise<T> {
      if (command === 'get_snapshot') return (await snapshot()) as T
      if (command === 'snapshot_sync') return { rev: 0, snapshot: await snapshot() } as T
      throw new Error(`preview: ${command}`)
    },
    listen: async () => () => {},
  }
}
