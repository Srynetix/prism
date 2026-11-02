import { describe, expect, it } from 'vitest'
import { runner } from './runner.svelte'

describe('runner', () => {
  it('runs one action at a time, and keeps its error until the next run', async () => {
    const task = runner()
    let release!: () => void
    const first = task.run(() => new Promise<void>((r) => (release = r)))
    expect(task.busy).toBe(true)
    let second = false
    await task.run(async () => void (second = true))
    expect(second).toBe(false)
    release()
    await first
    expect(task.busy).toBe(false)

    await task.run(async () => Promise.reject(new Error('refused')))
    expect(task.error).toBe('Error: refused')
    await task.run(async () => {})
    expect(task.error).toBeNull()
  })
})
