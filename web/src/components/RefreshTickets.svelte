<script lang="ts" module>
  import { hasCore, invoke } from '../bridge'

  /** Only one refresh at a time, shared by the tracker blocks (Sprint, To pick, Refinement). */
  const state = $state({ busy: false, error: null as string | null })

  /** Full re-read of the tracker (lists, sprint, slow queues); returns when done. */
  export async function refreshTickets() {
    if (state.busy || !hasCore()) return
    state.busy = true
    state.error = null
    try {
      await invoke<void>('refresh_tickets')
    } catch (e) {
      state.error = String(e)
    } finally {
      state.busy = false
    }
  }
</script>

<script lang="ts">
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import { trackerOn } from '../live.svelte'
  import { trackerName } from '../tracker'

  /** ↻ button in a tracker block's title: don't wait for the next collection. */
</script>

{#if hasCore() && trackerOn()}
  <button
    class="refresh"
    class:busy={state.busy}
    class:error={!!state.error}
    disabled={state.busy}
    onclick={refreshTickets}
    title={state.error ? t('Refresh failed: {error}', { error: state.error }) : state.busy ? t('Reloading {tracker}…', { tracker: trackerName() }) : t('Reload from {tracker} now (r)', { tracker: trackerName() })}
    aria-label={t('Reload from {tracker}', { tracker: trackerName() })}
  ><Icon name="refresh" spin={state.busy} /></button>
{/if}

<style>
  .refresh { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 20px; height: 20px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 14px; line-height: 1; cursor: pointer; }
  .refresh:hover { background: var(--surface-muted); color: var(--text); }
  .refresh.busy { cursor: progress; }
  .refresh.error { color: var(--ko); }
</style>
