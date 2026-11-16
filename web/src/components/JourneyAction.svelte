<script lang="ts">
  import { t } from '../i18n.svelte'
  import { armer } from '../armed.svelte'
  import type { Primary } from '../mr-actions'
  import ArmedLabel from './ArmedLabel.svelte'

  /**
   * The one action that moves a ticket on (journey-actions.ts), as a button: a link to Claude
   * Desktop, or run in place; what cannot be undone (a merge) on a second click. `wide`: the
   * phone's, the whole width.
   */
  let { primary, wide = false }: { primary: Primary; wide?: boolean } = $props()

  let busy = $state(false)
  let error = $state<string | null>(null)
  const armed = armer()
  async function run(e: MouseEvent) {
    e.stopPropagation()
    if (!('run' in primary) || busy) return
    if (primary.confirm && !armed.confirm()) return
    armed.disarm()
    busy = true
    error = null
    try {
      await primary.run()
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }
</script>

{#if 'href' in primary}
  <a class="btn sm primary go" class:wide href={primary.href} title={primary.title} onclick={(e) => e.stopPropagation()}>{primary.label}</a>
{:else}
  <button class="btn sm go" class:primary={!armed.is()} class:wide class:error={!!error} class:armed={armed.is()} class:busy disabled={busy} onclick={run} title={error ? t('Failed: {error}', { error }) : primary.title}>
    {#if busy}<span class="spinner"></span>{primary.label}{:else if primary.confirm}<ArmedLabel label={primary.label} armedLabel={t('Click again to confirm')} armed={armed.is()} />{:else}{primary.label}{/if}
  </button>
{/if}

<style>
  .go.error { --btn-c: var(--ko); }
  .go.wide { width: 100%; height: auto; padding: 9px 12px; border-radius: var(--r10); font-size: var(--fs-head); }
</style>
