<script lang="ts">
  import { GRACE, machineState, type MachineReason } from '../machine.svelte'
  import { clock } from '../clock.svelte'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Over a machine's window, always: whose Prism this is, so that nothing is done here believing it
   * is this computer; out of reach, why in words (the system's own in its tooltip) and since when,
   * the page showing its last state.
   */
  const profile = $derived(live.snapshot?.profile?.name)
  const now = $derived(machineState.since ? clock(1000).now : 0)
  const away = $derived(machineState.since ? now - machineState.since : 0)
  const minutes = $derived(Math.floor(away / 60_000))
  const WHY: Record<MachineReason, () => string> = {
    token: () => t('its Prism no longer knows this computer: pair it again'),
    refused: () => t('its Prism does not answer: closed, or its remote off'),
    unknown_host: () => t('its address is not found: the tunnel (Tailscale) down here or there'),
    unreachable: () => t('no answer: no network to it, or it is asleep'),
    certificate: () => t('its HTTPS certificate is refused'),
    dropped: () => t('the connection dropped'),
  }
  const why = $derived(machineState.reason ? WHY[machineState.reason]() : null)
</script>

<div class="bar machine-bar" class:off={!machineState.connected && away >= GRACE}>
  <span class="dot"></span>
  <b>{machineState.name}</b>
  {#if profile}<span>{profile}</span>{/if}
  {#if machineState.connected || away < GRACE}
    <span class="mut">{machineState.connected ? t('another computer’s Prism') : t('reconnecting…')}</span>
  {:else}
    <span class="why" title={machineState.error ?? undefined}>{why ?? t('out of reach')}</span>
    <span class="mut">{minutes ? tn(minutes, 'for {n} minute, its last state shown · trying again', 'for {n} minutes, its last state shown · trying again') : t('its last state shown · trying again')}</span>
  {/if}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 8px; padding: 3px 12px; font-size: var(--fs-meta); color: var(--text); background: color-mix(in srgb, var(--info) 12%, var(--surface)); border-bottom: 1px solid color-mix(in srgb, var(--info) 35%, transparent); white-space: nowrap; overflow: hidden; }
  .bar.off { background: var(--warn-bg); border-bottom-color: color-mix(in srgb, var(--warn) 35%, transparent); }
  .dot { width: 7px; height: 7px; flex-shrink: 0; border-radius: 50%; background: var(--info); }
  .off .dot { background: var(--warn); }
  .mut { color: var(--text-muted); }
  .why { min-width: 0; overflow: hidden; text-overflow: ellipsis; font-weight: 600; }
</style>
