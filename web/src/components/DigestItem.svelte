<script lang="ts">
  import { t } from '../i18n.svelte'
  import { alertSettings } from '../conventions'
  import { clearDelivered, deliver, digest, nextDue, silenced, type Held } from '../digest.svelte'
  import { locale } from '../locale'
  import { ago } from '../time'
  import Icon from './Icon.svelte'

  /**
   * The status bar's notifications item: what the digest holds and when it goes out, quiet hours
   * or focus while they silence everything, the last summary until read. A click opens its panel:
   * the last summary (held, resolved meanwhile, sent at once), then what waits for the next one.
   */
  let { now }: { now: number } = $props()

  const cfg = $derived(alertSettings())
  const at = $derived(new Date(now))
  const off = $derived(silenced(at))
  const next = $derived(cfg.digest ? nextDue(cfg.digest_times ?? [], at) : null)
  const hm = (d: Date | string) => new Date(d).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
  const quietEnd = $derived(cfg.quiet_hours?.split('-')[1]?.trim())
  const waiting = $derived(digest.held.length)
  const unread = $derived(digest.delivered ? digest.delivered.held.length + digest.delivered.resolved.length : 0)
  const shown = $derived(cfg.digest || !!off || waiting > 0 || unread > 0)

  let open = $state(false)
  const KIND: Record<Held['kind'], string> = { infra: t('Infra'), deploy: t('Deploy'), agent: t('Agent'), pipeline: 'CI', review: t('Review'), data: t('Data'), block: t('Block'), routine: t('Routine') }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && (open = false)} onclick={() => (open = false)} />

{#snippet list(items: Held[], tone: string)}
  {#each items as h (h.key + h.at)}
    <div class="it">
      <span class="dot {tone}"></span>
      <span class="k">{KIND[h.kind]}</span>
      <span class="tx" title={h.body}><b>{h.title}</b> {h.body}</span>
      <span class="ago">{ago(h.at)}</span>
    </div>
  {/each}
{/snippet}

{#if shown}
  <button class="sbi digest" class:hot={unread > 0} onclick={(e) => { e.stopPropagation(); open = !open }} title={off === 'quiet' ? t('Quiet hours: no notification') : off === 'focus' ? t('Focus: no notification') : cfg.digest ? t('Notifications held for the digest') : t('Notifications held')}>
    <span class="g"><Icon name={off ? 'pause' : 'alarm'} /></span>
    {#if unread}{t('Digest')} <b>{unread}</b>{:else if off === 'quiet'}{t('Quiet')}{#if quietEnd}{' '}{t('until')} <b>{quietEnd}</b>{/if}{:else if off === 'focus'}{t('Focus')}{:else if waiting}<b>{waiting}</b> {t('held')}{/if}
    {#if next && !unread}<span class="muted">{waiting || off ? '·' : ''} {t('next')} <b>{hm(next)}</b></span>{/if}
  </button>
{/if}

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="pop" role="dialog" aria-label={t('Notifications digest')} tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <div class="hd">
      <b>{cfg.digest ? t('Digest') : t('Notifications')}</b>
      <span class="muted">{cfg.digest ? t('at {times}', { times: (cfg.digest_times ?? []).join(', ') }) : t('live')}{cfg.quiet_hours ? ` · ${t('quiet {hours}', { hours: cfg.quiet_hours })}` : ''}</span>
      <span class="grow"></span>
      <button class="x" onclick={() => (open = false)} aria-label={t('Close')}><Icon name="close" /></button>
    </div>
    {#if digest.delivered}
      {@const d = digest.delivered}
      <div class="sec">
        <h4>{t('Digest of {time}', { time: hm(d.at) })} <span class="muted">{ago(d.at)}</span></h4>
        {#if d.held.length}<h5>{t('For you')} <span>{d.held.length}</span></h5>{@render list(d.held, 'warn')}{/if}
        {#if d.resolved.length}<h5>{t('Resolved meanwhile')} <span>{d.resolved.length}</span></h5>{@render list(d.resolved, 'ok')}{/if}
        {#if d.sent.length}<h5>{t('Sent at once')} <span>{d.sent.length}</span></h5>{@render list(d.sent, 'ko')}{/if}
      </div>
    {/if}
    <div class="sec">
      <h4>{cfg.digest && next ? t('For the {time} digest', { time: hm(next) }) : t('Held')} <span class="muted">{off === 'quiet' ? t('quiet hours') : off === 'focus' ? t('focus') : ''}</span></h4>
      {#if digest.held.length}{@render list(digest.held, 'warn')}{/if}
      {#if digest.resolved.length}<h5>{t('Resolved meanwhile')} <span>{digest.resolved.length}</span></h5>{@render list(digest.resolved, 'ok')}{/if}
      {#if digest.sent.length}<h5>{t('Sent at once')} <span>{digest.sent.length}</span></h5>{@render list(digest.sent, 'ko')}{/if}
      {#if !digest.held.length && !digest.resolved.length && !digest.sent.length}<p class="muted none">{digest.last ? t('Nothing held since {time}.', { time: hm(digest.last) }) : t('Nothing held since launch.')}</p>{/if}
    </div>
    <div class="ft">
      {#if digest.held.length || digest.resolved.length}<button onclick={() => deliver()}>{t('Send now')}</button>{/if}
      {#if digest.delivered}<button onclick={() => { clearDelivered(); open = false }}>{t('Mark read')}</button>{/if}
    </div>
  </div>
{/if}

<style>
  .digest.hot { color: var(--accent); }
  .muted { color: var(--text-muted); }
  .pop {
    position: fixed; left: 50%; bottom: 30px; z-index: 60; width: min(480px, 94vw); max-height: 60vh; overflow: auto; transform: translateX(-50%);
    border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-drawer); font-size: var(--fs-meta);
  }
  .hd { display: flex; align-items: baseline; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); position: sticky; top: 0; background: var(--surface); }
  .grow { flex: 1; }
  .x { display: inline-flex; padding: 2px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .sec { padding: 6px 12px 8px; border-bottom: 1px solid var(--border); }
  h4 { margin: 2px 0 4px; font-size: var(--fs-meta); font-weight: 600; display: flex; gap: 6px; }
  h5 { margin: 6px 0 2px; font-size: var(--fs-tag); font-weight: 500; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h5 span { font-family: var(--mono); color: var(--text); margin-left: 4px; }
  .it { display: grid; grid-template-columns: 8px 48px minmax(0, 1fr) auto; gap: 6px; align-items: center; padding: 2px 0; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); }
  .dot.warn { background: var(--warn); } .dot.ok { background: var(--ok); } .dot.ko { background: var(--ko); }
  .k { font-size: var(--fs-tag); color: var(--text-muted); }
  .tx { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tx b { font-weight: 500; }
  .ago { font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .none { margin: 2px 0; }
  .ft { display: flex; justify-content: flex-end; gap: 6px; padding: 8px 12px; }
  .ft button { padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface-muted); color: var(--text); font: inherit; cursor: pointer; }
  .ft button:hover { border-color: var(--accent); }
</style>
