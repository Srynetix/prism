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
  <a class="go" class:wide href={primary.href} title={primary.title} onclick={(e) => e.stopPropagation()}>{primary.label}</a>
{:else}
  <button class="go" class:wide class:error={!!error} class:armed={armed.is()} disabled={busy} onclick={run} title={error ? t('Failed: {error}', { error }) : armed.is() ? t('Click again to confirm') : primary.title}>
    {#if busy}…{:else if primary.confirm}<ArmedLabel label={primary.label} armedLabel={primary.confirm} armed={armed.is()} />{:else}{primary.label}{/if}
  </button>
{/if}

<style>
  .go { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; padding: 2px 10px; border: 1px solid var(--accent); border-radius: var(--r6); background: var(--accent); color: var(--on-accent); font: inherit; font-size: var(--fs-meta); font-weight: 600; line-height: 18px; white-space: nowrap; text-decoration: none; cursor: pointer; }
  .go:hover { filter: brightness(1.08); text-decoration: none; }
  .go:disabled { opacity: 0.6; cursor: progress; }
  .go.armed { border-color: var(--warn); background: var(--warn); }
  .go.error { border-color: var(--ko); background: var(--ko); }
  .go.wide { width: 100%; padding: 9px 12px; border-radius: var(--r10); font-size: 15px; }
</style>
