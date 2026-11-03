<script lang="ts">
  import { machineState } from '../machine.svelte'
  import { clock } from '../clock.svelte'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Over a machine's window, always: whose Prism this is, so that nothing is done here believing it
   * is this computer; out of reach, since when, the page showing its last state.
   */
  const profile = $derived(live.snapshot?.profile?.name)
  const minutes = $derived(machineState.since ? Math.floor((clock(30_000).now - machineState.since) / 60_000) : 0)
</script>

<div class="bar" class:off={!machineState.connected}>
  <span class="dot"></span>
  <b>{machineState.name}</b>
  {#if profile}<span>{profile}</span>{/if}
  {#if machineState.connected}
    <span class="mut">{t('another computer’s Prism')}</span>
  {:else}
    <span>{minutes ? tn(minutes, 'out of reach for {n} minute: its last state shown', 'out of reach for {n} minutes: its last state shown') : t('reconnecting…')}</span>
    {#if machineState.error}<span class="mut err" title={machineState.error}>{machineState.error}</span>{/if}
  {/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 8px; padding: 3px 12px; font-size: var(--fs-meta); color: var(--text); background: color-mix(in srgb, var(--info) 12%, var(--surface)); border-bottom: 1px solid color-mix(in srgb, var(--info) 35%, transparent); white-space: nowrap; overflow: hidden; }
  .bar.off { background: var(--warn-bg); border-bottom-color: color-mix(in srgb, var(--warn) 35%, transparent); }
  .dot { width: 7px; height: 7px; flex-shrink: 0; border-radius: 50%; background: var(--info); }
  .off .dot { background: var(--warn); }
  .mut { color: var(--text-muted); }
  .err { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
</style>
