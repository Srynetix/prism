<script lang="ts">
  import { GRACE, machineState, type MachineReason } from '../machine.svelte'
  import { localInvoke, profileWindow } from '../bridge'
  import { clock } from '../clock.svelte'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Over a machine's window, always: whose Prism this is, so that nothing is done here believing it
   * is this computer; out of reach, why in words (the system's own in its tooltip) and since when,
   * the page showing its last state. Over another profile's window (`profileWindow()`), only while
   * its Prism does not answer, with what starts it again.
   */
  const local = profileWindow()
  let starting = $state(false)
  async function start() {
    starting = true
    await localInvoke('open_profile', { id: local }).catch(() => {})
    setTimeout(() => (starting = false), 4000)
  }
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

{#if local}
  {#if !machineState.connected && away >= GRACE}
    <div class="bar machine-bar off">
      <b>{profile ?? local}</b>
      <span class="why" title={machineState.error ?? undefined}>{t('its Prism does not answer')}</span>
      <span class="mut">{t('its last state shown · trying again')}</span>
      <button class="start" onclick={start} disabled={starting}>{starting ? t('Starting…') : t('Start')}</button>
    </div>
  {/if}
{:else}
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
{/if}

<style>
  .start { margin-left: auto; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r5, 5px); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 3px 12px; font-size: var(--fs-meta); color: var(--text); background: color-mix(in srgb, var(--info) 12%, var(--surface)); border-bottom: 1px solid color-mix(in srgb, var(--info) 35%, transparent); white-space: nowrap; overflow: hidden; }
  .bar.off { background: var(--warn-bg); border-bottom-color: color-mix(in srgb, var(--warn) 35%, transparent); }
  .dot { width: 7px; height: 7px; flex-shrink: 0; border-radius: 50%; background: var(--info); }
  .off .dot { background: var(--warn); }
  .mut { color: var(--text-muted); }
  .why { min-width: 0; overflow: hidden; text-overflow: ellipsis; font-weight: 600; }
</style>
