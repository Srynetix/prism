/// <reference types="node" />
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { LAUNCH_MODES } from './agents.svelte'

describe('the launch modes', () => {
  it('are the core’s, in its order', () => {
    const rust = readFileSync(new URL('../../src-tauri/crates/core/src/config/agents.rs', import.meta.url), 'utf8')
    const list = /pub const LAUNCH_MODES: \[&str; \d+\] = \[([^\]]*)\]/.exec(rust)?.[1]
    expect(list?.match(/"([^"]+)"/g)?.map((m) => m.slice(1, -1))).toEqual([...LAUNCH_MODES])
  })
})
