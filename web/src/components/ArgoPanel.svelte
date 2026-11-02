<script lang="ts">
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { t, tn } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { argoKey, argoView, closeArgo } from '../argo-view.svelte'
  import { argoLabel, argoTone, argoUrl, resourceOutcome, resourceTone } from '../argocd'
  import { openLink } from '../links'
  import { ago, span } from '../time'
  import Icon from './Icon.svelte'
  import type { ArgoApp, ArgoInstance, ArgoResource } from '../types'

  /**
   * What happens on ArgoCD's syncs: on the left the Applications of a cluster's instances, those in
   * trouble or syncing first; on the right the chosen one's last operation (phase, start and end,
   * auto-sync or not, message), the resources of that sync with their result, those out of sync or
   * unhealthy now, and its last deployments. Live: the snapshot follows a running sync every 5 s.
   */
  const RANK = { ko: 0, info: 1, warn: 2, ok: 3 }
  const rows = $derived(
    (live.snapshot?.argocd?.instances ?? [])
      .filter((i) => !argoView.cluster || i.cluster === argoView.cluster)
      .flatMap((instance) => instance.apps.map((app) => ({ app, instance, key: argoKey(instance.cluster, instance.namespace, app.name) })))
      .sort((a, b) => RANK[argoTone(a.app)] - RANK[argoTone(b.app)] || a.app.name.localeCompare(b.app.name)),
  )
  const chosen = $derived(rows.find((r) => r.key === argoView.app) ?? rows[0] ?? null)
  const several = $derived(new Set(rows.map((r) => r.instance.namespace)).size > 1)

  const tone = resourceTone
  const outcome = resourceOutcome
  const duration = (a: ArgoApp) =>
    a.started_at ? span((a.operation === 'Running' || !a.synced_at ? Date.now() : new Date(a.synced_at).getTime()) - new Date(a.started_at).getTime()) : null
  const url = (a: ArgoApp, i: ArgoInstance) => argoUrl({ app: a, instance: i })

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !argoView.open) return
    e.stopImmediatePropagation()
    closeArgo()
  }
  let drawerEl: HTMLElement | undefined = $state()
  /** On the phone, the detail is under the list: another Application chosen, it starts from its top. */
  let detailEl: HTMLElement | undefined = $state()
  $effect(() => {
    void chosen?.key
    if (document.documentElement.classList.contains('remote') && detailEl) detailEl.scrollTop = 0
  })
  function onOutside(e: PointerEvent) {
    if (argoView.open && clickedOutside(e, drawerEl)) {
      closeArgo()
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if argoView.open}
  <aside class="drawer" bind:this={drawerEl} data-drawer="51" data-pinned={isPinned('argo')} aria-label={t('ArgoCD syncs')}>
    <header>
      <Icon name="argocd" />
      <span class="what">ArgoCD{argoView.cluster ? ` · ${argoView.cluster}` : ''}</span>
      <span class="muted">{t('{n} of {total} need a look', { n: rows.filter((r) => argoTone(r.app) !== 'ok').length, total: rows.length })}</span>
      <span class="grow"></span>
      <PinButton kind="argo" />
      <button class="icon" onclick={closeArgo} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      <nav class="apps" aria-label={t('Applications')}>
        {#each rows as r (r.key)}
          <button class="app" class:on={chosen?.key === r.key} onclick={() => (argoView.app = r.key)} title="{r.app.name} · {r.app.sync} · {r.app.health}">
            <span class="dot {argoTone(r.app)}"></span>
            <span class="n">{r.app.name}{#if several}<small> · {r.instance.namespace}</small>{/if}</span>
            <span class="l {argoTone(r.app)}">{t(argoLabel(r.app))}</span>
          </button>
        {:else}
          <p class="muted pad">{t('No Application read.')}</p>
        {/each}
      </nav>
      <div class="detail" bind:this={detailEl}>
        {#if chosen}
          {@const a = chosen.app}
          {@const link = url(a, chosen.instance)}
          <div class="head">
            <h3>{a.name}</h3>
            {#if link}<button class="txt" onclick={() => openLink(link)} title={t('Open it in ArgoCD')}>ArgoCD <Icon name="external" /></button>{/if}
          </div>
          <p class="meta">
            {[a.env, a.group, a.namespace].filter(Boolean).join(' · ')}
            {#if a.revision} · <span class="mono">{a.revision}</span>{/if}
            {#if a.images.length} · <span class="mono">{a.images.join(', ')}</span>{/if}
          </p>
          <div class="pills">
            <span class="pill {a.sync === 'Synced' ? 'ok' : a.sync === 'OutOfSync' ? 'warn' : ''}">{t(a.sync)}</span>
            <span class="pill {a.health === 'Healthy' ? 'ok' : a.health === 'Progressing' ? 'info' : a.health === 'Degraded' || a.health === 'Missing' ? 'ko' : 'warn'}">{t(a.health)}</span>
          </div>

          <h4>{t('Last operation')}</h4>
          {#if a.operation}
            <p class="op">
              <span class="pill {a.operation === 'Succeeded' ? 'ok' : a.operation === 'Running' ? 'info' : 'ko'}">{a.operation === 'Running' ? t('Syncing') : t(a.operation.toLowerCase())}</span>
              {#if a.automated}<span class="muted">{t('auto-sync')}</span>{:else if a.automated === false}<span class="muted">{t('started by hand')}</span>{/if}
              {#if a.started_at}<span class="muted">· {a.operation === 'Running' ? t('started {ago} ago', { ago: ago(a.started_at) }) : t('ran {ago} ago', { ago: ago(a.started_at) })}{#if duration(a) && a.operation !== 'Running'}{t(', took {duration}', { duration: duration(a) ?? '' })}{/if}</span>{/if}
            </p>
            {#if a.message}<p class="msg" class:ko={argoTone(a) === 'ko'}>{a.message}</p>{/if}
          {:else}
            <p class="muted">{t('No operation recorded.')}</p>
          {/if}

          {#if a.sync_resources?.length}
            <h4>{t('Resources of that sync')} <span class="muted">{a.sync_resources.filter((r) => tone(r) !== 'ok').length ? tn(a.sync_resources.filter((r) => tone(r) !== 'ok').length, '{n} not done', '{n} not done') : t('all done')}</span></h4>
            {@render resources(a.sync_resources)}
          {/if}

          {#if a.drifting?.length}
            <h4>{t('Out of sync or unhealthy now')}</h4>
            {@render resources(a.drifting)}
          {/if}

          {#if a.history?.length}
            <h4>{t('Last deployments')}</h4>
            {#each a.history as h, i (i)}
              <p class="hist"><span class="mono">{h.revision ?? '—'}</span><span class="muted">{h.deployed_at ? t('{ago} ago', { ago: ago(h.deployed_at) }) : ''}</span>{#if i === 0}<span class="muted">{t('current')}</span>{/if}</p>
            {/each}
          {/if}
        {/if}
      </div>
    </div>
  </aside>
{/if}

{#snippet resources(list: ArgoResource[])}
  <div class="res">
    {#each list as r, i (i)}
      <div class="r">
        <span class="dot {tone(r)}"></span>
        <span class="k">{r.kind}</span>
        <span class="mono rn" title={r.name}>{r.name}</span>
        <span class="o {tone(r)}">{outcome(r)}</span>
        {#if r.message}<span class="rm" title={r.message}>{r.message}</span>{/if}
      </div>
    {/each}
  </div>
{/snippet}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 51; width: min(880px, 94vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-k8s) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; white-space: nowrap;
    border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--src-k8s);
    font-size: var(--fs-meta); color: var(--text-muted);
  }
  .what { color: var(--text); font-weight: 600; }
  .grow { flex: 1; }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(200px, 280px) minmax(0, 1fr); }
  .apps { overflow: auto; border-right: 1px solid var(--border); padding: 4px 0; }
  .app { display: flex; align-items: center; gap: 7px; width: 100%; padding: 5px 12px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .app:hover { background: var(--surface-muted); }
  .app.on { background: color-mix(in srgb, var(--src-k8s) 16%, transparent); }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .n small { color: var(--text-muted); }
  .l { flex-shrink: 0; font-size: var(--fs-tag); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--ok); }
  .dot.info { background: var(--info); }
  .dot.warn { background: var(--warn); }
  .dot.ko { background: var(--ko); }
  .l.ok, .o.ok { color: var(--ok); } .l.info, .o.info { color: var(--info); } .l.warn, .o.warn { color: var(--warn); } .l.ko, .o.ko { color: var(--ko); }
  .detail { overflow: auto; padding: 12px 16px 24px; font-size: var(--fs-body); }
  .head { display: flex; align-items: center; gap: 10px; }
  h3 { margin: 0; flex: 1; font-size: var(--fs-title); font-weight: 600; overflow-wrap: anywhere; }
  h4 { margin: 16px 0 6px; font-size: var(--fs-meta); font-weight: 600; color: var(--text); }
  h4 .muted { font-weight: 400; margin-left: 4px; }
  .meta { margin: 4px 0 8px; color: var(--text-muted); font-size: var(--fs-meta); overflow-wrap: anywhere; }
  .pills, .op { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin: 0; font-size: var(--fs-meta); }
  .pill { padding: 0 7px; border-radius: var(--r9); font-size: var(--fs-tag); font-weight: 500; line-height: 17px; background: var(--surface-muted); color: var(--text-muted); }
  .pill.ok { color: var(--ok); background: var(--ok-bg); } .pill.info { color: var(--info); background: var(--info-bg); } .pill.warn { color: var(--warn); background: var(--warn-bg); } .pill.ko { color: var(--ko); background: var(--ko-bg); }
  .msg { margin: 6px 0 0; padding: 6px 8px; border-radius: var(--r6); background: var(--surface-muted); font-size: var(--fs-meta); white-space: pre-wrap; overflow-wrap: anywhere; }
  .msg.ko { color: var(--ko); background: var(--ko-bg); }
  .res { display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .r { display: grid; grid-template-columns: 8px 90px minmax(0, 1fr) auto; gap: 2px 8px; align-items: center; padding: 4px 8px; border-top: 1px solid var(--border); font-size: var(--fs-meta); }
  .r:first-child { border-top: none; }
  .k { color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; }
  .rn { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-mono); }
  .o { font-size: var(--fs-tag); white-space: nowrap; }
  .rm { grid-column: 3 / 5; color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hist { display: flex; gap: 10px; margin: 2px 0; font-size: var(--fs-meta); }
  .txt { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .txt:hover { border-color: var(--src-k8s); }
  /* On the phone: the whole screen, the Applications on top (a third of it, scrolled), the chosen one's detail under them. */
  :global(html.remote) .drawer { width: 100vw; border-left: none; }
  :global(html.remote) header { padding-top: calc(10px + env(safe-area-inset-top)); }
  :global(html.remote) .body { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, auto) minmax(0, 1fr); }
  :global(html.remote) .apps { max-height: 34dvh; border-right: none; border-bottom: 1px solid var(--border); }
  :global(html.remote) .app { padding-block: 9px; }
  :global(html.remote) .detail { padding: 12px 14px calc(24px + env(safe-area-inset-bottom)); }
  :global(html.remote) .r { grid-template-columns: 8px max-content minmax(0, 1fr) auto; }
  :global(html.remote) :is(.rn, .rm) { white-space: normal; overflow-wrap: anywhere; }
  .muted { color: var(--text-muted); }
  .pad { padding: 8px 12px; }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
</style>
