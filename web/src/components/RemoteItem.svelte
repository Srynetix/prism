<script lang="ts">
  import { t } from '../i18n.svelte'
  import { armer } from '../armed.svelte'
  import { openSettings, saveNow } from '../settings.svelte'
  import type { RemoteStatus } from '../types'
  import Icon from './Icon.svelte'
  import { invoke } from '../bridge'
  import { live } from '../live.svelte'

  /**
   * The remote in the status bar: shown as long as it is on, and never hidden (a phone may be
   * watching). Stop turns it off (`[remote] enabled = false`): every session ends. Restart starts
   * its server again as it is set (Tailscale was off, or went down): the sessions end, the phones
   * stay paired.
   */
  let { remote }: { remote: RemoteStatus } = $props()
  const armed = armer()
  const connected = $derived(remote.devices.filter((d) => d.connected))
  const agentTitle = (id: string) => live.snapshot?.agents.find((a) => a.id === id)?.title ?? id
  const typing = $derived(remote.controls[0] ?? null)
  const label = $derived(remote.error ? t('Remote: off') : typing ? t('{device} types in {agent}', { device: typing.device, agent: agentTitle(typing.agent) }) : connected.length ? (connected.length === 1 ? t('{name} connected', { name: connected[0].name }) : t('{n} phones connected', { n: connected.length })) : t('Remote on'))
  const stop = () => armed.confirm() && saveNow(['remote', 'enabled'], false)
  let restarting = $state(false)
  async function restart() {
    if (restarting || (!remote.error && !armed.confirm('restart'))) return
    restarting = true
    try {
      await invoke('remote_restart')
    } finally {
      restarting = false
    }
  }
</script>

<span class="sbi remote" class:live={connected.length > 0} class:typing={!!typing} class:ko={!!remote.error}>
  <button class="go" onclick={() => openSettings('remote-devices')} title={remote.error ?? t('Prism answers phones on {url} · Settings › Remote', { url: remote.url ?? '' })}>
    <span class="g"><Icon name={remote.error ? 'warning' : 'web'} /></span>{label}
  </button>
  {#if typing}<button class="stop" onclick={() => invoke('remote_release', { agent: typing.agent })} title={t('Take the terminal back: the phone goes back to watching (a key typed in it on the desk does too)')}>{t('Take back')}</button>{/if}
  <button class="stop" class:armed={armed.is('restart')} onclick={restart} disabled={restarting} title={remote.error ? t('Start the remote again: Tailscale up now?') : t('Start the remote again: the phones’ sessions end, they stay paired (click twice)')}>{restarting ? t('Restarting…') : armed.is('restart') ? t('Restart?') : t('Restart')}</button>
  <button class="stop" class:armed={armed.is()} onclick={stop} title={t('Turn the remote off: every phone’s session ends (click twice)')}>{armed.is() ? t('Stop?') : t('Stop')}</button>
</span>

<style>
  .remote { display: inline-flex; align-items: center; gap: 2px; padding: 0 2px 0 0; border-radius: 10px; background: color-mix(in srgb, var(--violet) 14%, transparent); color: var(--violet); }
  .remote.live { background: color-mix(in srgb, var(--violet) 26%, transparent); }
  .remote.typing { background: var(--warn-bg); color: var(--warn); }
  .remote.ko { background: var(--ko-bg); color: var(--ko); }
  button { background: none; border: none; color: inherit; font: inherit; cursor: pointer; display: inline-flex; align-items: center; gap: 4px; padding: 0 6px; }
  .stop { opacity: 0.75; text-decoration: underline; }
  .stop:hover, .stop.armed { opacity: 1; }
  .stop.armed { color: var(--ko); }
</style>
