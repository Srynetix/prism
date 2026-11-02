<script lang="ts">
  import { locale } from '../locale'
  import { t, tn } from '../i18n.svelte'
  import { stageEnvs } from '../stages'
  import { openLink } from '../links'
  import { untrusted } from '../untrusted'
  import { argoKey, openArgo } from '../argo-view.svelte'
  import BlockControls from './BlockControls.svelte'
  import Icon from './Icon.svelte'
  import Logo from './Logo.svelte'
  import { isCollapsed, isShort, pref, setPref } from '../layout.svelte'
  import { claudeClick, investigateAction } from '../claude-actions'
  import { ago, span } from '../time'
  import { keys } from '../platform'
  import { argoLabel, argoTone, authoritative, resourceOutcome, resourceTone } from '../argocd'
  import type { ArgoApp, ArgoCd, ArgoInstance, ArgoResource, Environments } from '../types'

  /**
   * The state of ArgoCD itself: every Application its instances manage (the platform's and the
   * customers' included), those in trouble first with why (the resources that failed or drifted
   * from Git, the sync running), the healthy ones as a wall of names by environment, and the sync
   * operations of the last 24 hours whatever started them. No versions nor promotion: what runs
   * where, and what is waiting to ship, is a delivery block's business. Read-only; a click opens
   * the ArgoCD drawer on the Application.
   */
  let { argocd, environments, error }: { argocd: ArgoCd | null | undefined; environments: Environments | null | undefined; error?: string } = $props()

  type Tone = 'ok' | 'warn' | 'ko' | 'info'
  interface Row {
    app: ArgoApp
    instance: ArgoInstance
    key: string
    /** The tab: the Application's group (`data`, `customers`…), `''` without one. */
    tab: string
    /** Where it sits on the wall: its environment, or its namespace for a customer's (all in production). */
    place: string
    tone: Tone
  }

  const RANK: Record<Tone, number> = { ko: 0, info: 1, warn: 2, ok: 3 }
  const DAY = 24 * 3600_000

  const instances = $derived(argocd?.instances ?? [])
  const all = $derived<Row[]>(
    instances.flatMap((instance) =>
      instance.apps
        // Instance replaced for this environment (`[[argocd]] envs`): its states no longer mean anything.
        .filter((app) => authoritative(instance, app.env))
        .map((app) => ({
          app,
          instance,
          key: argoKey(instance.cluster, instance.namespace, app.name),
          tab: app.group ?? '',
          place: app.customer ? (app.namespace ?? instance.namespace) : (app.env ?? 'other'),
          tone: argoTone(app),
        })),
    ),
  )
  /** The groups, those without one first; the customers' instances last. */
  const tabs = $derived(
    [...new Set(all.map((r) => r.tab))].sort((a, b) => +all.some((r) => r.tab === a && r.app.customer) - +all.some((r) => r.tab === b && r.app.customer) || a.localeCompare(b)),
  )
  let picked = $state(pref<string>('argocd.tab', ''))
  const tab = $derived(tabs.includes(picked) ? picked : (tabs[0] ?? ''))
  function setTab(tb: string) {
    picked = tb
    setPref('argocd.tab', tb)
  }
  const rows = $derived(all.filter((r) => r.tab === tab))
  const trouble = $derived(rows.filter((r) => r.tone !== 'ok').sort((a, b) => RANK[a.tone] - RANK[b.tone] || a.app.name.localeCompare(b.app.name)))
  const troubledIn = (tb: string) => all.filter((r) => r.tab === tb && r.tone !== 'ok').length

  /** The wall's rows: the stages' environments in their order (production first), then the others. */
  const places = $derived.by(() => {
    const order = stageEnvs()
    const rank = (p: string) => (order.includes(p) ? order.indexOf(p) : order.length)
    return [...new Set(rows.map((r) => r.place))].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))
  })
  /** On the wall: the component's name when the path gives one, beside its environment. */
  const shortName = (r: Row) => (!r.app.customer && r.app.app) || r.app.name

  const synced = $derived(rows.filter((r) => r.app.sync === 'Synced').length)
  const healthy = $derived(rows.filter((r) => r.app.health === 'Healthy').length)
  const failed = $derived(rows.filter((r) => r.app.operation === 'Failed' || r.app.operation === 'Error').length)
  const drifted = $derived(rows.filter((r) => r.app.sync === 'OutOfSync'))
  const driftedResources = $derived(drifted.reduce((n, r) => n + r.app.drifting.filter((d) => d.status && d.status !== 'Synced').length, 0))

  /** Why it needs a look: the last sync's resources that did not make it, then those off Git or unhealthy now. */
  function causes(a: ArgoApp): ArgoResource[] {
    const seen = new Set<string>()
    return [...a.sync_resources.filter((r) => resourceTone(r) !== 'ok'), ...a.drifting].filter((r) => {
      const k = `${r.kind}/${r.name}`
      if (seen.has(k)) return false
      seen.add(k)
      return true
    })
  }
  /** Investigate, when the Application lands in an environment Prism reads (its cluster is not in ArgoCD's state). */
  function investigate(r: Row) {
    if (r.tone !== 'ko' && r.tone !== 'warn') return null
    const env = environments?.envs.find((e) => e.env === r.app.env && e.namespace === r.app.namespace)
    if (!env) return null
    // ArgoCD's message says what the GitOps repository's manifests and hooks made of it: quoted as data.
    const why = r.app.message ? `:\n\n${untrusted('ArgoCD message', r.app.message)}\n\n` : ''
    return investigateAction(r.app.deployments[0] ?? r.app.app ?? r.app.name, env.namespace, env.context, env.env, `ArgoCD application ${r.app.name} is ${argoLabel(r.app)}${why}`)
  }

  /** The last operation of each Application, when it started in the last 24 hours. */
  const operations = $derived(
    rows
      .filter((r) => r.app.started_at && Date.now() - new Date(r.app.started_at).getTime() < DAY)
      .sort((a, b) => new Date(b.app.started_at!).getTime() - new Date(a.app.started_at!).getTime()),
  )
  const opTone = (a: ArgoApp): Tone => (a.operation === 'Running' ? 'info' : a.operation === 'Failed' || a.operation === 'Error' ? 'ko' : 'ok')
  function opText(a: ArgoApp) {
    if (a.operation === 'Running') return `${t('running')} · ${a.started_at ? span(Date.now() - new Date(a.started_at).getTime()) : ''}`
    const took = a.started_at && a.synced_at ? ` · ${span(new Date(a.synced_at).getTime() - new Date(a.started_at).getTime())}` : ''
    return `${t((a.operation ?? 'done').toLowerCase())}${took}${opTone(a) === 'ko' && a.message ? ` · ${a.message}` : ''}`
  }
  /** Since the sync running started, or the last one ended. */
  const age = (a: ArgoApp) => (a.operation === 'Running' ? ago(a.started_at) : ago(a.synced_at ?? a.started_at))
  const clock = (iso: string) => new Date(iso).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })

  const url = $derived(instances.length === 1 ? instances[0].url : null)
  const open = (r: Row) => openArgo(r.instance.cluster, r.key)
</script>

<section class="panel argo-block" style="--src: var(--src-k8s)" class:collapsed={isCollapsed('argocd')}>
  <h2>
    <span class="title"><Icon name="argocd" />ArgoCD</span>
    {#if all.length}
      <small>{tn(rows.length, '{n} app', '{n} apps')}{#if trouble.length}{' '}· <span class="warn">{tn(trouble.length, '{n} needs a look', '{n} need a look')}</span>{/if}</small>
    {/if}
    {#if tabs.length > 1}
      <span class="tabs" role="group" aria-label={t('Group')}>
        {#each tabs as tb (tb)}
          {@const n = troubledIn(tb)}
          <button class:on={tb === tab} onclick={() => setTab(tb)}>{tb || t('default')}{#if n}<b class="dot-n">{n}</b>{/if}</button>
        {/each}
      </span>
    {/if}
    {#if url}<button class="txt" onclick={() => openLink(url)} title={t('Open ArgoCD')}>ArgoCD <Icon name="external" /></button>{/if}
    <BlockControls id="argocd" />
  </h2>
  {#if !instances.length}
    <div class="empty">{error ? t('Unavailable: {error}', { error }) : t('No ArgoCD read: declare [[argocd]], or an argocd namespace on the default cluster ([clusters] default_context).')}</div>
  {:else if isShort('argocd')}
    <div class="short-body">
      {#each places as p (p)}
        {@const here = rows.filter((r) => r.place === p)}
        {@const ok = here.filter((r) => r.tone === 'ok').length}
        <div class="srow">
          <span class="k">{p}</span>
          <span class="stack">{#each ['ok', 'info', 'warn', 'ko'] as tone (tone)}{@const n = here.filter((r) => r.tone === tone).length}{#if n}<i class={tone} style="flex: {n}"></i>{/if}{/each}</span>
          <span class="mono">{ok}/{here.length}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="scroll">
      <div class="kpis">
        <div class="kpi"><span>{t('Synced')}</span><b class:warn={synced < rows.length}>{synced}<small>/{rows.length}</small></b><small>{t('with Git')}</small></div>
        <div class="kpi"><span>{t('Healthy')}</span><b class:ko={healthy < rows.length}>{healthy}<small>/{rows.length}</small></b><small>{t('Applications')}</small></div>
        <div class="kpi"><span>{t('Failed syncs')}</span><b class:ko={failed > 0}>{failed}</b><small>{t('last operation')}</small></div>
        <div class="kpi"><span>{t('Drift')}</span><b class:warn={drifted.length > 0}>{drifted.length}</b><small>{drifted.length ? `${tn(drifted.length, 'app', 'apps')} · ${tn(driftedResources, '{n} resource', '{n} resources')}` : t('all as in Git')}</small></div>
      </div>

      {#each instances.filter((i) => i.error) as i (i.namespace)}
        <p class="err">{t('{cluster} · {namespace} unreadable: {error}', { cluster: i.cluster, namespace: i.namespace, error: i.error ?? '' })}</p>
      {/each}

      {#if trouble.length}
        <h5>{t('Need a look')} <b>{trouble.length}</b></h5>
        {#each trouble as r (r.key)}
          {@const list = causes(r.app)}
          {@const act = investigate(r)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="app" onclick={() => open(r)} title="{r.app.name} · {r.app.sync} · {r.app.health} — {t('open its syncs')}">
            <span class="dot {r.tone}"></span>
            <span class="n"><b>{r.app.name}</b><small>{[r.app.group, r.app.env, r.app.namespace].filter(Boolean).join(' · ')}{#if r.app.operation}<span class="why">{r.app.automated ? t('auto-sync') : t('by hand')}</span>{/if}<span class="ago-s">{age(r.app)}</span></small></span>
            <span class="pill {r.tone}">{t(argoLabel(r.app))}</span>
            {#if act?.href}<a class="act" href={act.href} onclick={(e) => { e.stopPropagation(); claudeClick(act)(e) }} title={`${act.prompt}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{act.label}</a>{/if}
            <span class="ago">{age(r.app)}</span>
            {#if list.length || (r.tone === 'ko' && r.app.message)}
              <span class="res">
                {#each list.slice(0, 3) as c, i (i)}
                  <span class="{resourceTone(c)}"><i>{resourceTone(c) === 'ko' ? '✕' : resourceTone(c) === 'info' ? '◌' : '◐'}</i> <b>{c.kind}</b> {c.name} · {resourceOutcome(c)}{c.message ? ` — ${c.message}` : ''}</span>
                {/each}
                {#if list.length > 3}<span>{t('+{n} more', { n: list.length - 3 })}</span>{/if}
                {#if !list.length && r.app.message}<span class="ko">{r.app.message}</span>{/if}
              </span>
            {/if}
          </div>
        {/each}
      {/if}

      {#if rows.length > trouble.length}
        <h5>{t('Synced and healthy')} <b>{rows.length - trouble.length}</b></h5>
        {#each places as p (p)}
          {@const ok = rows.filter((r) => r.place === p && r.tone === 'ok')}
          {#if ok.length}
            <div class="wall">
              <span class="k">{p}</span>
              <span class="chips">{#each ok as r (r.key)}<button onclick={() => open(r)} title="{r.app.name} · {t('open its syncs')}"><i class="dot ok"></i>{shortName(r)}</button>{/each}</span>
            </div>
          {/if}
        {/each}
      {/if}

      {#if operations.length}
        <h5>{t('Sync operations')} <b>{operations.length}</b><span class="muted">24 h{operations.some((r) => opTone(r.app) === 'ko') ? ` · ${tn(operations.filter((r) => opTone(r.app) === 'ko').length, '{n} failed', '{n} failed')}` : ''}</span></h5>
        {#each operations.slice(0, 8) as r (r.key)}
          <button class="op" onclick={() => open(r)}>
            <span class="t">{clock(r.app.started_at!)}</span>
            <span class="dot {opTone(r.app)}"></span>
            <span class="w"><b>{r.app.name}</b> — {opText(r.app)}</span>
            {#if r.app.revision}<span class="rev">{r.app.revision}</span>{/if}
            <span class="how">{r.app.automated ? t('auto') : t('by hand')}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .argo-block { container-type: inline-size; }
  h2 small .warn { color: var(--warn); }
  .tabs { display: inline-flex; gap: 1px; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .tabs button { display: inline-flex; align-items: center; gap: 4px; padding: 0 8px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .tabs button.on { background: var(--surface); color: var(--text); font-weight: 500; }
  .dot-n { min-width: 14px; padding: 0 3px; border-radius: 7px; background: var(--ko); color: #fff; font-size: 9.5px; line-height: 14px; text-align: center; }
  .txt { display: inline-flex; align-items: center; gap: 4px; padding: 0 6px; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); font-weight: 400; cursor: pointer; }
  .txt:hover { text-decoration: underline; }
  .err { margin: 0; padding: 6px 12px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); }

  .kpis { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 1px; background: var(--border); border-bottom: 1px solid var(--border); }
  .kpi { display: flex; flex-direction: column; gap: 1px; min-width: 0; padding: 6px 12px; background: var(--surface); }
  .kpi span { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.3px; }
  .kpi b { font-size: 16px; font-weight: 600; white-space: nowrap; }
  .kpi b small { font-size: var(--fs-meta); font-weight: 400; color: var(--text-muted); }
  .kpi b.ko { color: var(--ko); }
  .kpi b.warn { color: var(--warn); }
  .kpi > small { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

  h5 { display: flex; gap: 6px; align-items: baseline; margin: 0; padding: 10px 12px 4px; font-size: var(--fs-tag); font-weight: 500; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h5 b { font-weight: 600; color: var(--text); }
  h5 .muted { margin-left: auto; text-transform: none; letter-spacing: 0; }

  .dot { width: 8px; height: 8px; border-radius: 50%; display: inline-block; flex-shrink: 0; background: var(--ok); }
  .dot.warn { background: var(--warn); }
  .dot.ko { background: var(--ko); }
  .dot.info { background: var(--info); animation: pulse 1.4s infinite; }

  .app { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto auto 56px; gap: 2px 8px; align-items: center; padding: 6px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); cursor: pointer; }
  .app:hover { background: var(--surface-muted); }
  .n { display: flex; flex-direction: column; min-width: 0; }
  .n b { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .n small { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; overflow: hidden; }
  .why { padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); }
  .app > .pill { grid-area: 1 / 3; }
  .ago { grid-area: 1 / 5; text-align: right; color: var(--text-muted); font-size: var(--fs-meta); white-space: nowrap; }
  .act { grid-area: 1 / 4; display: inline-flex; justify-self: end; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--src-claude) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; }
  .act:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); text-decoration: none; }
  .res { grid-area: 2 / 2 / auto / -1; display: flex; flex-direction: column; gap: 1px; font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .res > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .res b { font-weight: 500; color: var(--text); }
  .res i { font-style: normal; }
  .res .ko i, .res > span.ko { color: var(--ko); }
  .res .warn i { color: var(--warn); }
  .res .info i { color: var(--info); }

  .wall { display: grid; grid-template-columns: 76px minmax(0, 1fr); align-items: start; padding: 3px 12px; }
  .wall .k { padding-top: 3px; font-size: var(--fs-tag); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chips button { display: inline-flex; align-items: center; gap: 5px; padding: 1px 8px 1px 6px; border: 1px solid var(--border); border-radius: var(--r9); background: none; color: var(--text); font: inherit; font-size: var(--fs-tag); white-space: nowrap; cursor: pointer; }
  .chips button:hover { background: var(--surface-muted); }
  .chips .dot { width: 6px; height: 6px; }

  .op { display: grid; grid-template-columns: max-content 8px minmax(0, 1fr) auto auto; gap: 8px; align-items: center; width: 100%; padding: 3px 12px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .op:last-child { margin-bottom: 6px; }
  .op:hover { background: var(--surface-muted); }
  .op .t, .op .rev { font: var(--fs-tag) var(--mono); color: var(--text-muted); white-space: nowrap; }
  .ago-s { display: none; }
  .op .w { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .op .w b { font-weight: 550; }
  .how { padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }

  .short-body .k { width: 76px; flex: none; color: var(--text-muted); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; }
  .stack { flex: 1; height: 6px; border-radius: 3px; overflow: hidden; display: flex; background: var(--surface-muted); }
  .stack i { display: block; height: 100%; background: var(--ok); }
  .stack i.info { background: var(--info); } .stack i.warn { background: var(--warn); } .stack i.ko { background: var(--ko); }

  /* A narrow column (and the phone): the figures two by two, Investigate and the age under the name. */
  @container (max-width: 520px) {
    .kpis { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .app { grid-template-columns: 8px minmax(0, 1fr) auto; }
    .app .act { grid-area: 2 / 2; justify-self: start; }
    .app .ago { display: none; }
    .ago-s { display: inline; }
    .app .res { grid-area: 3 / 2 / auto / -1; }
    .op { grid-template-columns: max-content 8px minmax(0, 1fr) auto; }
    .op .rev { display: none; }
  }
</style>
