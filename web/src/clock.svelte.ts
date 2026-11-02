import { createSubscriber } from 'svelte/reactivity'

/**
 * A clock to show what grows with time (a running stage, an age, a countdown): `clock(ms).now`
 * is the time, read again every `ms` while something on screen reads it. One timer per period,
 * shared by every reader, stopped when the last one goes.
 */
class Clock {
  #subscribe: () => void

  constructor(ms: number) {
    this.#subscribe = createSubscriber((update) => {
      const t = setInterval(update, ms)
      return () => clearInterval(t)
    })
  }

  get now() {
    this.#subscribe()
    return Date.now()
  }
}

const clocks = new Map<number, Clock>()

export function clock(ms: number): Clock {
  let c = clocks.get(ms)
  if (!c) clocks.set(ms, (c = new Clock(ms)))
  return c
}
