<script lang="ts">
  import { hasCore } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import Logo from '../../components/Logo.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import { openLink } from '../../links'
  import { ago } from '../../time'
  import { t, tn } from '../../i18n.svelte'
  import { call, type BlockProps } from '../block'
  import { toneOf, troubled, type Level, type StatusState } from './types'

  /**
   * Status pages: one line per service, from its public status page; an incident under way below
   * its service, with its latest update and the components it affects. Short: only the services
   * with something under way.
   */
  let { state: status, error }: BlockProps<StatusState> = $props()

  const services = $derived(status?.services ?? [])
  const bad = $derived(troubled(status))
  const incidents = $derived(services.reduce((n, s) => n + s.incidents.filter((i) => !i.maintenance).length, 0))
  const shown = $derived(isShort('status') ? bad : services)

  const LEVEL: Record<Level, string> = { operational: t('Operational'), maintenance: t('Maintenance'), minor: t('Minor'), major: t('Major'), critical: t('Critical'), unknown: t('Not answering') }

  let refreshing = $state(false)
  async function refresh() {
    refreshing = true
    try {
      await call('status', 'refresh')
    } finally {
      setTimeout(() => (refreshing = false), 1500)
    }
  }
</script>

<section class="panel" style="--src: var(--accent)" class:collapsed={isCollapsed('status')}>
  <h2>
    <span class="title"><Icon name="activity" />{t('Status pages')}</span>
    {#if status && services.length}
      <small>{incidents ? tn(incidents, '{n} incident', '{n} incidents') : bad.length ? tn(bad.length, '{n} degraded', '{n} degraded') : t('All operational')}</small>
      {#if hasCore()}<button class="icon" class:spin={refreshing} onclick={refresh} title={t('Read the status pages again now')} aria-label={t('Refresh')}><Icon name="refresh" /></button>{/if}
    {/if}
    <BlockControls id="status" />
  </h2>
  {#if !status}
    <div class="empty">{t('Status pages are not read. Turn on Settings › General › Status pages, then restart Prism.')}</div>
  {:else if !services.length}
    <div class="empty">{error ?? t('Reading the status pages…')}</div>
  {:else}
    <div class="scroll">
      {#each shown as s (s.id)}
        <div class="svc">
          <span class="dot {toneOf(s.level)}" title={LEVEL[s.level]}></span>
          {#if s.logo}<Logo name={s.logo} size={13} />{/if}
          <button class="name" onclick={() => openLink(s.url)} title={t('Open {url}', { url: s.url })}>{s.name}</button>
          <span class="sum muted" title="{s.error ?? s.summary}{s.checked_at ? ` · ${t('read {time} ago', { time: ago(s.checked_at) })}` : ''}">{s.summary}</span>
        </div>
        {#each s.incidents as i (i.id)}
          <button class="inc {toneOf(i.impact)}" class:maint={i.maintenance} onclick={() => openLink(i.url)} title={t('Open the incident on {name}’s status page', { name: s.name })}>
            <span class="line">
              <span class="pill">{i.maintenance ? t('maintenance') : t(i.impact)}</span>
              <b>{i.name}</b>
            </span>
            <span class="meta muted">{[i.status, i.started_at && t('since {time}', { time: ago(i.started_at) }), i.updated_at && t('updated {time} ago', { time: ago(i.updated_at) })].filter(Boolean).join(' · ')}</span>
            {#if i.update && !isShort('status')}<span class="upd">{i.update}</span>{/if}
            {#if i.components.length}<span class="comps">{#each i.components as c (c)}<span class="chip">{c}</span>{/each}</span>{/if}
          </button>
        {/each}
        {#if !s.incidents.length && s.components.length}
          <div class="comps alone">{#each s.components as c (c.name)}<span class="chip {toneOf(c.level)}" title={LEVEL[c.level]}>{c.name}</span>{/each}</div>
        {/if}
      {:else}
        <div class="empty">{t('All {n} services operational.', { n: services.length })}</div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .empty { padding: 14px 16px; color: var(--text-muted); font-size: var(--fs-body); }
  .muted { color: var(--text-muted); }
  .icon { display: inline-grid; place-items: center; width: 22px; height: 22px; padding: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .icon :global(svg) { width: 13px; height: 13px; }
  .spin :global(svg) { animation: spin 0.8s linear infinite; }
  .svc { display: flex; align-items: center; gap: 7px; padding: 6px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); }
  .svc:first-child { border-top: none; }
  .dot { flex-shrink: 0; width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); opacity: 0.5; }
  .dot.ok { background: var(--ok); opacity: 1; }
  .dot.warn { background: var(--warn); opacity: 1; }
  .dot.ko { background: var(--ko); opacity: 1; box-shadow: 0 0 0 3px color-mix(in srgb, var(--ko) 20%, transparent); }
  .name { padding: 0; border: none; background: none; color: var(--text); font: inherit; font-weight: 500; cursor: pointer; white-space: nowrap; }
  .name:hover { color: var(--info); }
  .sum { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-meta); }
  .inc { display: flex; flex-direction: column; gap: 3px; width: calc(100% - 24px); margin: 0 12px 6px 27px; padding: 6px 8px; border: 1px solid var(--border); border-left: 3px solid var(--text-muted); border-radius: var(--r6); background: var(--surface-muted); color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; width: auto; }
  .inc:hover { border-color: var(--info); }
  .inc.warn { border-left-color: var(--warn); }
  .inc.ko { border-left-color: var(--ko); }
  .line { display: flex; align-items: baseline; gap: 6px; }
  .line b { font-weight: 500; }
  .pill { flex-shrink: 0; padding: 0 6px; border-radius: 9px; background: var(--surface); color: var(--text-muted); font-size: var(--fs-tag); text-transform: capitalize; }
  .inc.warn .pill { background: var(--warn-bg); color: var(--warn); }
  .inc.ko .pill { background: var(--ko-bg); color: var(--ko); }
  .meta { font-size: var(--fs-meta); }
  .upd { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; font-size: var(--fs-meta); color: var(--text-muted); }
  .comps { display: flex; flex-wrap: wrap; gap: 4px; }
  .comps.alone { padding: 0 12px 6px 27px; }
  .chip { padding: 0 6px; border: 1px solid var(--border); border-radius: 9px; font-size: var(--fs-tag); color: var(--text-muted); }
  .chip.warn { color: var(--warn); }
  .chip.ko { color: var(--ko); }
</style>
