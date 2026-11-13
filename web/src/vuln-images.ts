import type { VulnProject } from './types'

/**
 * A scanned image against what its registry said when last asked (« Check the registry »):
 * `changed`, its tag points to another digest than the scanned one; `same`, it does not;
 * `unknown`, the registry could not be read; `unasked`, never asked since the scan.
 */
export type ImageState = { kind: 'unasked' | 'same' | 'changed' | 'unknown' }

export function imageState(p: Pick<VulnProject, 'scanned' | 'remote'>): ImageState {
  const r = p.remote
  if (!r) return { kind: 'unasked' }
  if (r.error || !r.digest) return { kind: 'unknown' }
  if (!p.scanned?.digest) return { kind: 'unknown' }
  return { kind: r.digest === p.scanned.digest ? 'same' : 'changed' }
}
