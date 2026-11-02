/**
 * An action a component runs on a click (a rebase, a retry…): one at a time, `busy` while it runs,
 * its `error` kept until the next run.
 */
export function runner() {
  let busy = $state(false)
  let error = $state<string | null>(null)
  return {
    get busy() {
      return busy
    },
    get error() {
      return error
    },
    /** Runs `f`, unless a run is under way. */
    async run(f: () => Promise<unknown>) {
      if (busy) return
      busy = true
      error = null
      try {
        await f()
      } catch (e) {
        error = String(e)
      } finally {
        busy = false
      }
    },
  }
}
