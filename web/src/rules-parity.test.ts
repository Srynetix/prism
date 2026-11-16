import { afterEach, describe, expect, it } from 'vitest'
import cases from './rules-parity.json'
import { onlyWaitsForCi } from './rules'
import { isDone } from './conventions'
import { kindOf } from './agents.svelte'
import { live } from './live.svelte'
import type { MergeRequest, Snapshot } from './types'

/**
 * The rules the window repeats from the Rust core, on the cases the core's parity test reads too
 * (`rules-parity.json`): one side changed alone fails one of the two.
 */
describe('rules shared with the core', () => {
  afterEach(() => {
    live.snapshot = null
  })

  it('onlyWaitsForCi as only_waits_for_ci', () => {
    const { base, cases: list } = cases.only_waits_for_ci
    for (const c of list) expect([c.with, onlyWaitsForCi({ ...base, ...c.with } as unknown as MergeRequest)]).toEqual([c.with, c.waits])
  })

  it('isDone as Conventions::is_done', () => {
    live.snapshot = { conventions: { done_statuses: cases.is_done.done_statuses } } as unknown as Snapshot
    for (const c of cases.is_done.cases) expect([c, isDone(c)]).toEqual([c, c.done])
  })

  it('kindOf as kind_of', () => {
    live.snapshot = { agents_settings: { kinds: cases.kind_of.kinds } } as unknown as Snapshot
    for (const c of cases.kind_of.cases) expect([c.prompt, kindOf(c.prompt)]).toEqual([c.prompt, c.kind])
  })
})
