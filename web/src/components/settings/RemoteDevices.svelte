<script lang="ts">
  import { clock } from '../../clock.svelte'
  import { invoke, isLocal } from '../../bridge'
  import { live } from '../../live.svelte'
  import { armer } from '../../armed.svelte'
  import { ago } from '../../time'
  import ArmedLabel from '../ArmedLabel.svelte'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { openLink } from '../../links'

  /**
   * Settings › Remote › Devices: the phones paired with this Prism, and pairing one. The QR code
   * holds a one-time address; the phone then shows a code, which is confirmed here: a QR code seen
   * by someone else pairs nothing on its own. This computer's browser can be paired the same way (Open
   * the pairing page instead of the QR code): the remote then opens on the desk, to try it at a
   * phone's size (the browser's device mode), as the phone sees it.
   *
   * Below, the other computers whose Prism opens this one in a window (`[[machines]]` there):
   * paired by a code typed there, their name accepted here; revoked, their windows close.
   */
  const remote = $derived(live.snapshot?.remote ?? null)
  const pairing = $derived(remote?.pairing ?? null)
  const armed = armer()
  let error = $state<string | null>(null)
  const now = $derived(clock(1000).now)
  const left = $derived(pairing ? Math.max(0, Math.round((Date.parse(pairing.expires_at) - now) / 1000)) : 0)

  const run = async (command: string, args?: Record<string, unknown>) => {
    error = null
    try {
      await invoke(command, args)
    } catch (e) {
      error = String(e)
    }
  }
  const revoke = (id: string) => armed.confirm(`revoke:${id}`) && run('remote_revoke', { id })
  const peerPairing = $derived(remote?.peer_pairing ?? null)
  const peerLeft = $derived(peerPairing ? Math.max(0, Math.round((Date.parse(peerPairing.expires_at) - now) / 1000)) : 0)
  const revokePeer = (name: string) => armed.confirm(`peer:${name}`) && run('remote_peer_revoke', { name })

  /** The phone being renamed, and its name as typed. */
  let editing = $state<string | null>(null)
  let draft = $state('')
  function rename(id: string, name: string) {
    editing = id
    draft = name
  }
  async function saveName() {
    const id = editing
    if (!id) return
    editing = null
    const name = draft.trim()
    if (name && name !== remote?.devices.find((d) => d.id === id)?.name) await run('remote_rename', { id, name })
  }
  const focus = (el: HTMLInputElement) => {
    el.focus()
    el.select()
  }
</script>

<div class="card">
  {#if !remote}
    <p class="muted">{t('The remote is off: turn it on in Remote › Access, then pair a phone here.')}</p>
  {:else if remote.error}
    <p class="ko"><Icon name="warning" /> {remote.error}</p>
    <button class="btn" onclick={() => run('remote_restart')}>{t('Restart the remote')}</button>
  {:else if remote.relay}
    <!-- Tunnel relay: the phones are paired with the profile whose remote has the tunnel. -->
    <div class="row head">
      <div>
        {#if remote.url}
          <b>{t('Reached through another profile at')} <span class="mono">{remote.url}</span></b>
        {:else}
          <b>{t('Reached through another profile’s Prism, closed now')}</b>
        {/if}
        <small>{t('Tunnel relay: phones are paired in the profile whose remote has the tunnel, and switch to this one from their header; this profile keeps its own rights (Remote › Exposure).')}</small>
      </div>
    </div>
  {:else}
    <div class="row head">
      <div>
        <b>{t('Prism answers on')} <span class="mono">{remote.url}</span></b>
        <small>{t('A phone opens it once paired; Chrome on Android then installs it as an app.')}</small>
      </div>
      {#if !pairing}
        <span class="row">
          {#if remote.url}<button class="btn" onclick={() => remote.url && openLink(remote.url)} title={t('The remote in this computer’s browser, as a phone sees it: pair this browser once (Pair a phone › this computer’s browser), then its device mode gives a phone’s size')}>{t('Open on this computer')}</button>{/if}
          <button class="btn primary" onclick={() => run('remote_pair_start')}>{t('Pair a phone')}</button>
        </span>
      {/if}
    </div>
    {#if pairing}
      <div class="pair">
        <!-- Prism's own SVG, built by this computer's server from the pairing's address: drawn as
             HTML only here, never from a machine's snapshot (its window has another's). -->
        {#if isLocal()}<div class="qr">{@html pairing.qr_svg}</div>{/if}
        <div class="steps">
          {#if !pairing.code}
            <b>{t('Scan this code with the phone’s camera')}</b>
            <small>{t('A one-time address, for {time}. Only this computer’s tailnet user, or your tunnel, reaches it.', { time: `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}` })}</small>
            <small>{t('To try the remote on this computer:')} <button class="link" onclick={() => openLink(pairing.url)}>{t('pair this computer’s browser instead')}</button> {t('(its passkey), then open it at a phone’s size with the browser’s device mode.')}</small>
          {:else}
            <b>{t('{device} shows the code', { device: pairing.device ?? '' })}</b>
            <span class="code mono">{pairing.code.split('').join(' ')}</span>
            {#if pairing.ready}
              <small>{t('Is it the code on the phone? Only then confirm.')}</small>
            {:else}
              <small>{t('Waiting for the phone to create its passkey…')}</small>
            {/if}
          {/if}
          <span class="row">
            {#if pairing.ready}<button class="btn primary" onclick={() => run('remote_pair_confirm')}><Icon name="check" /> {t('It matches')}</button>{/if}
            <button class="btn" onclick={() => run('remote_pair_cancel')}>{t('Cancel')}</button>
          </span>
        </div>
      </div>
    {/if}
  {/if}
  {#if error}<p class="ko"><Icon name="warning" /> {error}</p>{/if}
</div>

{#if remote?.devices.length}
  <div class="card">
    {#each remote.devices as d (d.id)}
      <div class="row device">
        <span class="dot" class:on={d.connected} title={d.connected ? t('Connected now') : t('Not connected')}></span>
        <div class="grow">
          {#if editing === d.id}
            <input class="name" bind:value={draft} maxlength="40" use:focus onblur={saveName} onkeydown={(e) => { if (e.key === 'Enter') saveName(); else if (e.key === 'Escape') editing = null }} aria-label={t('Name of {name}', { name: d.name })} />
          {:else}
            <b class="named">{d.name}<button class="edit" onclick={() => rename(d.id, d.name)} title={t('Rename (its passkey and sessions stay)')} aria-label={t('Rename {name}', { name: d.name })}><Icon name="edit" /></button></b>
          {/if}
          <small>{t('paired {ago}', { ago: ago(d.paired_at) })}{d.connected ? t(' · connected now') : d.last_seen ? t(' · last seen {ago}', { ago: ago(d.last_seen) }) : ''}</small>
        </div>
        <button class="btn" class:armed={armed.is(`revoke:${d.id}`)} onclick={() => revoke(d.id)} title={t('Its passkey opens nothing anymore, its sessions end (click twice)')}>
          <ArmedLabel label={t('Revoke')} armedLabel={t('Revoke {name}?', { name: d.name })} armed={armed.is(`revoke:${d.id}`)} />
        </button>
      </div>
    {/each}
  </div>
{/if}

{#if remote && !remote.error && !remote.relay}
  <div class="card">
    <div class="row head">
      <div>
        <b>{t('Other computers')}</b>
        <small>{t('A computer’s Prism opens this one in a window of its own (Settings › Remote › Machines there), as this account would: its agents, terminals, settings. What it may do: Remote › Machines here.')}</small>
      </div>
      {#if !peerPairing}<button class="btn primary" onclick={() => run('remote_peer_pair_start')}>{t('Pair a computer')}</button>{/if}
    </div>
    {#if peerPairing}
      <div class="pair">
        <div class="steps">
          {#if !peerPairing.name}
            <b>{t('On the other computer: profile menu › Other computers › this machine, then this code')}</b>
            <span class="code mono">{peerPairing.code.slice(0, 3)} {peerPairing.code.slice(3)}</span>
            <small>{t('For {time}; five wrong codes end it.', { time: `${Math.floor(peerLeft / 60)}:${String(peerLeft % 60).padStart(2, '0')}` })}</small>
          {:else}
            <b>{t('{name} asks to open this Prism', { name: peerPairing.name })}</b>
            <small>{t('As this account would: its agents, terminals and settings. Accept only a computer of yours, which you just paired.')}</small>
          {/if}
          <span class="row">
            {#if peerPairing.name}
              <button class="btn primary" onclick={() => run('remote_peer_pair_answer', { accept: true })}><Icon name="check" /> {t('Accept')}</button>
              <button class="btn" onclick={() => run('remote_peer_pair_answer', { accept: false })}>{t('Refuse')}</button>
            {:else}
              <button class="btn" onclick={() => run('remote_peer_pair_cancel')}>{t('Cancel')}</button>
            {/if}
          </span>
        </div>
      </div>
    {/if}
    {#each remote.peers as p (p.name)}
      <div class="row device">
        <span class="dot" class:on={p.connected} title={p.connected ? t('Connected now') : t('Not connected')}></span>
        <div class="grow">
          <b>{p.name}</b>
          <small>{t('paired {ago}', { ago: ago(p.paired_at) })}{p.connected ? t(' · connected now') : ''}</small>
        </div>
        <button class="btn" class:armed={armed.is(`peer:${p.name}`)} onclick={() => revokePeer(p.name)} title={t('Its token opens nothing anymore, its windows close (click twice)')}>
          <ArmedLabel label={t('Revoke')} armedLabel={t('Revoke {name}?', { name: p.name })} armed={armed.is(`peer:${p.name}`)} />
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .card { margin-bottom: 14px; padding: 12px 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); display: flex; flex-direction: column; gap: 10px; }
  .row { display: flex; align-items: center; gap: 10px; }
  .head { justify-content: space-between; }
  .head > div, .grow { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .grow { flex: 1; }
  small, .muted { color: var(--text-muted); font-size: var(--fs-meta); }
  p { margin: 0; }
  .ko { color: var(--ko); display: flex; gap: 6px; align-items: center; }
  .mono { font-family: var(--font-mono, ui-monospace, monospace); }
  .pair { display: flex; gap: 16px; align-items: center; padding: 12px; border-radius: var(--r10); background: var(--surface-muted); }
  .qr { width: 176px; height: 176px; flex: none; background: #fff; border-radius: 8px; padding: 6px; }
  .qr :global(svg) { width: 100%; height: 100%; display: block; }
  .steps { display: flex; flex-direction: column; gap: 8px; }
  .code { font-size: 26px; font-weight: 600; letter-spacing: 6px; color: var(--accent); }
  .device + .device { border-top: 1px solid var(--border); padding-top: 10px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); opacity: 0.5; flex: none; }
  .dot.on { background: var(--ok); opacity: 1; }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover { border-color: var(--accent); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
  .btn.armed { color: var(--ko); border-color: var(--ko); }
  .named { display: flex; align-items: center; gap: 4px; }
  .edit { display: inline-flex; padding: 2px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; opacity: 0; }
  .device:hover .edit, .edit:focus-visible { opacity: 1; }
  .edit:hover { color: var(--text); background: var(--surface-muted); }
  .name { max-width: 260px; padding: 2px 6px; border: 1px solid var(--accent); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-weight: 600; }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; text-decoration: underline; }
</style>
