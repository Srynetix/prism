import { describe, expect, it } from 'vitest'
import { imageState } from './vuln-images'
import type { VulnProject } from './types'

const scanned = { reference: 'r/x:1', digest: 'sha256:aa', created: null, scanned_at: '2026-11-13T08:00:00Z', platform: 'linux/amd64', gcp: false }
const remote = (more: object) => ({ at: '2026-11-13T09:00:00Z', digest: 'sha256:aa', newer: null, error: null, ...more })

describe('imageState', () => {
  it('against the registry: the same digest, another, or unread', () => {
    expect(imageState({ scanned, remote: null } as unknown as VulnProject).kind).toBe('unasked')
    expect(imageState({ scanned, remote: remote({}) } as unknown as VulnProject).kind).toBe('same')
    expect(imageState({ scanned, remote: remote({ digest: 'sha256:bb' }) } as unknown as VulnProject).kind).toBe('changed')
    expect(imageState({ scanned, remote: remote({ digest: null, error: 'unauthorized' }) } as unknown as VulnProject).kind).toBe('unknown')
    expect(imageState({ scanned: { ...scanned, digest: null }, remote: remote({}) } as unknown as VulnProject).kind).toBe('unknown')
  })
})
