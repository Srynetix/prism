<script lang="ts">
  import { locale } from '../locale'
  import { t, tn } from '../i18n.svelte'
  import { stageEnvs, stageOf } from '../stages'
  import { openLink } from '../links'
  import { argoKey, openArgo } from '../argo-view.svelte'
  import BlockControls from './BlockControls.svelte'
  import Icon from './Icon.svelte'
  import Logo from './Logo.svelte'
  import { isCollapsed, isShort, pref, setPref, showBlock } from '../layout.svelte'
  import { claudeClick, investigateAction } from '../claude-actions'
  import { columns, grip } from '../columns.svelte'
  import { ago, minutesSince } from '../time'
  import { canShowPodLogs, openWorkload } from '../pod-logs.svelte'
  import type { ArgoCd, ClusterLoad, EnvStatus, Environments, Workload, WorkloadLoad } from '../types'
  import { argoLabel, argoTone, argoUrl, byDeployment, deploymentKey, type ArgoRef } from '../argocd'
  import { keys } from '../platform'

  /**
   * Environment state: per environment (prod, demo, staging tabs), the Deployments
   * of each configured namespace, with their ready pods, restarts, waiting reason or
   * last termination reason, and the running image compared with the last git tag. A struggling workload
   * is diagnosed with Claude (read-only), and its logs open in a drawer (click on
   * the name, or "Logs"). Tab remembered under `envs.tab`.
   */
  let { environments, argocd, error }: { environments: Environments | null | undefined; argocd?: ArgoCd | null; error?: string } = $props()

  /** ArgoCD: the Application managing each Deployment, and those of an application (deduplicated). */
  const argo = $derived(byDeployment(argocd))
  const argoOf = (rows: { s: EnvStatus; w: Workload }[]): ArgoRef[] => {
    const seen = new Map<string, ArgoRef>()
    for (const { s, w } of rows) {
      const r = argo.get(deploymentKey(s.env, s.namespace, w.name))
      if (r) seen.set(`${r.instance.cluster}/${r.instance.namespace}/${r.app.name}`, r)
    }
    return [...seen.values()]
  }
  /** A click shows what happens on its syncs; ⌘-click opens it in ArgoCD. */
  function argoClick(e: MouseEvent, r: ArgoRef) {
    e.preventDefault()
    e.stopPropagation()
    const url = argoUrl(r)
    if ((e.metaKey || e.ctrlKey) && url) return openLink(url)
    openArgo(r.instance.cluster, argoKey(r.instance.cluster, r.instance.namespace, r.app.name))
  }
  const argoTitle = (r: ArgoRef) =>
    [
      `ArgoCD ${r.instance.cluster}/${r.instance.namespace} · ${r.app.name}`,
      `${t(r.app.sync)} · ${t(r.app.health)}${r.app.operation ? ` · ${t('last sync {operation}', { operation: t(r.app.operation.toLowerCase()) })}` : ''}${r.app.synced_at ? ` ${ago(r.app.synced_at)}` : ''}`,
      r.app.revision ? `${t('revision {revision}', { revision: r.app.revision })}${r.app.images.length ? ` · ${t('images {images}', { images: [...new Set(r.app.images)].join(', ') })}` : ''}` : '',
      r.app.message ?? '',
      argoUrl(r) ? t('Click: its syncs · ⌘-click: open in ArgoCD') : t('Click: its syncs'),
    ]
      .filter(Boolean)
      .join('\n')

  /** Production first, then the preprod and branch stages (`[clusters] stages`), then the others. */
  const ORDER = $derived(stageEnvs())
  /** Beyond this, restarts are flagged. */
  const RESTARTS = 5
  /** An older termination is no longer a signal. */
  const RECENT_EXIT_HOURS = 24

  const names = $derived([...new Set(environments?.envs.map((e) => e.env) ?? [])].sort((a, b) => (ORDER.indexOf(a) + 1 || 99) - (ORDER.indexOf(b) + 1 || 99)))
  let tab = $state(pref<string>('envs.tab', ''))
  function setTab(name: string) {
    tab = name
    setPref('envs.tab', name)
  }
  const env = $derived(names.includes(tab) ? tab : names[0])
  const sections = $derived(environments?.envs.filter((e) => e.env === env) ?? [])

  const recentExit = (w: Workload) => !!w.last_exit && minutesSince(w.last_exit_at) < RECENT_EXIT_HOURS * 60
  /** `ko`: pods are missing or a container is stuck; `warn`: restarts, recent termination or outdated image. */
  function health(w: Workload): 'ok' | 'warn' | 'ko' {
    if (w.ready < w.desired || w.waiting) return 'ko'
    if (w.restarts > RESTARTS || recentExit(w) || w.drift) return 'warn'
    return 'ok'
  }
  /** What is wrong, in the window's language; `say = plain` for the agent's prompt, in English. */
  function issue(w: Workload, say: typeof t = t): string {
    const parts = []
    if (w.ready < w.desired) parts.push(say('{ready}/{desired} pods ready', { ready: w.ready, desired: w.desired }))
    if (w.waiting) parts.push(w.waiting)
    if (w.last_exit) parts.push(say('last exit {reason} {ago}', { reason: w.last_exit, ago: ago(w.last_exit_at) }))
    if (w.restarts) parts.push(say('{n} restarts', { n: w.restarts }))
    if (w.drift) parts.push(say('runs {tag}, latest tag {expected}', { tag: w.image_tag ?? '', expected: w.expected ?? '' }))
    return parts.join(', ')
  }
  const plain = (text: string, vars?: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (_, k: string) => String(vars?.[k] ?? ''))
  const all = (e: string) => environments?.envs.filter((s) => s.env === e).flatMap((s) => s.workloads) ?? []
  const troubled = (e: string) => all(e).filter((w) => health(w) === 'ko').length
  const workloads = $derived(all(env))
  const ready = $derived(workloads.filter((w) => w.ready >= w.desired && !w.waiting).length)
  const restarts = $derived(workloads.reduce((n, w) => n + w.restarts, 0))
  const drifted = $derived(workloads.filter((w) => w.drift))
  const failing = $derived(sections.filter((s) => s.error).length)
  const label = (s: EnvStatus) => `${s.cluster} / ${s.namespace}`

  const cores = (m: number) => (m / 1000).toFixed(m < 10_000 ? 1 : 0)
  const gib = (b: number) => (b / 1024 ** 3).toFixed(b < 10 * 1024 ** 3 ? 1 : 0)
  const pct = (used: number | null, total: number) => (used !== null && total ? Math.min(100, (100 * used) / total) : null)
  const tone = (p: number | null) => (p === null ? '' : p >= 90 ? 'ko' : p >= 75 ? 'warn' : '')
  type Row = { s: EnvStatus; w: Workload }
  type App = { key: string; label: string; items: Row[] }
  /**
   * The tab per cluster (in config order): its load line at the top, the
   * unreadable namespaces, then its namespaces holding Deployments (in config order, a pattern's
   * by name), each with its Deployments by application (one component per role: api, internal,
   * worker…). Each Deployment keeps its section (context, namespace) for its logs and its diagnosis.
   */
  const byCluster = $derived.by(() => {
    const contexts = [...new Set(sections.map((s) => s.context))]
    return contexts.map((context) => {
      const mine = sections.filter((s) => s.context === context)
      const cluster = mine[0].cluster
      const rowsOf = new Map<string, Row[]>()
      for (const s of mine) if (s.workloads.length) rowsOf.set(s.namespace, [...(rowsOf.get(s.namespace) ?? []), ...s.workloads.map((w) => ({ s, w }))])
      const namespaces = [...rowsOf].map(([name, rows]) => {
        rows.sort((a, b) => (a.w.app ?? a.w.name).localeCompare(b.w.app ?? b.w.name) || a.w.name.localeCompare(b.w.name))
        const apps = new Map<string, App>()
        for (const r of rows) {
          const label = r.w.app ?? r.w.name
          const key = `${env}:${cluster}:${name}:${label}`
          if (!apps.has(key)) apps.set(key, { key, label, items: [] })
          apps.get(key)!.items.push(r)
        }
        return { name, key: `ns:${env}:${context}:${name}`, workloads: rows.length, apps: [...apps.values()] }
      })
      return {
        context,
        name: cluster,
        load: environments?.clusters?.find((c) => c.context === context) ?? null,
        unreadable: mine.filter((s) => s.error),
        workloads: namespaces.reduce((n, ns) => n + ns.workloads, 0),
        namespaces,
        apps: namespaces.flatMap((ns) => ns.apps),
      }
    })
  })
  /**
   * Collapsed applications (by `<env>:<cluster>:<namespace>:<app>`), namespaces
   * (`ns:<env>:<context>:<namespace>`) and clusters (`cluster:<env>:<context>`),
   * remembered under `envs.folded`.
   */
  let folded = $state<string[]>(pref<string[]>('envs.folded', []))
  const isFolded = (key: string) => folded.includes(key)
  function fold(key: string) {
    folded = isFolded(key) ? folded.filter((k) => k !== key) : [...folded, key]
    setPref('envs.folded', folded)
  }
  const RANK = { ok: 0, warn: 1, ko: 2 } as const
  /** An application's row: the most severe state, the sums, the most recent rollout. */
  function summary(ws: Workload[]) {
    const worst = ws.map(health).reduce((a, b) => (RANK[b] > RANK[a] ? b : a), 'ok' as 'ok' | 'warn' | 'ko')
    const loads = ws.map((w) => w.load)
    const all = loads.every((l) => l && l.cpu_used_millis !== null)
    const sum = (f: (l: WorkloadLoad) => number) => loads.reduce((n, l) => n + (l ? f(l) : 0), 0)
    const mem = all ? sum((l) => l.mem_used_bytes ?? 0) : null
    const limits = loads.every((l) => l?.mem_limit_bytes !== null && l?.mem_limit_bytes !== undefined)
    const cap = limits ? sum((l) => l.mem_limit_bytes ?? 0) : sum((l) => l.mem_requested_bytes)
    return {
      health: worst,
      troubled: ws.filter((w) => health(w) !== 'ok').length,
      ready: ws.reduce((n, w) => n + w.ready, 0),
      desired: ws.reduce((n, w) => n + w.desired, 0),
      restarts: ws.reduce((n, w) => n + w.restarts, 0),
      cpu: all ? sum((l) => l.cpu_used_millis ?? 0) : null,
      mem,
      ratio: mem !== null && cap ? Math.min(100, (100 * mem) / cap) : null,
      updated: ws.map((w) => w.updated_at).filter((u): u is string => !!u).sort().at(-1) ?? null,
    }
  }
  /** A rollout less than 24 h old stands out. */
  const RECENT_ROLLOUT_HOURS = 24
  const updatedTitle = (w: Workload) =>
    w.updated_at ? `${t('Last rollout {date}', { date: new Date(w.updated_at).toLocaleString(locale()) })}${w.revision ? ` · ${t('revision {revision}', { revision: w.revision })}` : ''}` : ''
  /** CPU: `120m` under one core; RAM: `340Mi` under 1 GiB. */
  const cpuShort = (m: number) => (m < 1000 ? `${m}m` : cores(m))
  const memShort = (b: number) => (b < 1024 ** 3 ? `${Math.round(b / 1024 ** 2)}Mi` : `${gib(b)}Gi`)
  /** RAM relative to the limit (what triggers an OOMKilled), otherwise to the requests. */
  const memRatio = (l: WorkloadLoad) => {
    const cap = l.mem_limit_bytes ?? (l.mem_requested_bytes || null)
    return l.mem_used_bytes !== null && cap ? Math.min(100, (100 * l.mem_used_bytes) / cap) : null
  }
  function loadTitle(l: WorkloadLoad) {
    const lim = (v: number | null, f: (n: number) => string) => (v === null ? t('no limit') : t('limit {value}', { value: f(v) }))
    return [
      `CPU ${l.cpu_used_millis !== null ? cpuShort(l.cpu_used_millis) : '?'} · ${t('request {value}', { value: cpuShort(l.cpu_requested_millis) })} · ${lim(l.cpu_limit_millis, cpuShort)}`,
      `RAM ${l.mem_used_bytes !== null ? memShort(l.mem_used_bytes) : '?'} · ${t('request {value}', { value: memShort(l.mem_requested_bytes) })} · ${lim(l.mem_limit_bytes, memShort)}`,
    ].join('\n')
  }
  const clusterTitle = (c: ClusterLoad) =>
    `${c.context} · ${tn(c.nodes, '{n} node', '{n} nodes')}${c.cpu_used_millis !== null ? ` · CPU ${t('{used} / {total} cores', { used: cores(c.cpu_used_millis), total: cores(c.cpu_allocatable_millis) })}` : ''}${c.mem_used_bytes !== null ? ` · RAM ${gib(c.mem_used_bytes)} / ${gib(c.mem_allocatable_bytes)} GiB` : ''}${c.error ? `\n${c.error}` : ''}`
  const logsOf = (s: EnvStatus, w: Workload) => () =>
    openWorkload({ context: s.context, namespace: s.namespace, deployment: w.name, label: `${w.app ?? w.name}${w.app ? ` ${w.role}` : ''} · ${s.env}` })
  /** The cluster's ArgoCD summary: the ArgoCD block when this page shows it, otherwise the drawer. */
  function argoSummary(cluster: string) {
    if (document.querySelector('[data-block="argocd"]')) showBlock('argocd')
    else openArgo(cluster)
  }
</script>

{#snippet nameOf(w: Workload, branch: '' | 'mid' | 'last')}
  {#if branch}<span class="tree" aria-hidden="true">{branch === 'last' ? '└' : '├'}</span><b class="comp">{w.role}</b>
  {:else}<b>{w.app ?? w.name}</b>{#if w.app}<span class="role">{w.role}</span>{/if}{/if}
{/snippet}

{#snippet workloadRow(s: EnvStatus, w: Workload, branch: '' | 'mid' | 'last')}
  {@const h = health(w)}
  {@const a = h !== 'ok' ? investigateAction(w.name, s.namespace, s.context, s.env, issue(w, plain)) : null}
  <div class="wl {h}" class:child={!!branch}>
    <span class="dot-st" title={h === 'ok' ? t('Healthy') : issue(w)}></span>
    {#if canShowPodLogs()}
      <button class="name" title="{w.name} · {label(s)} · {t('open the logs')}" onclick={logsOf(s, w)}>{@render nameOf(w, branch)}</button>
    {:else}
      <span class="name" title="{w.name} · {label(s)}">{@render nameOf(w, branch)}</span>
    {/if}
    <span class="pods" title={t('{ready}/{desired} ready · {pods} pods', { ready: w.ready, desired: w.desired, pods: w.pods })}>{#each Array(Math.max(w.desired, w.pods)) as _, i (i)}<i class:up={i < w.ready}></i>{/each}</span>
    {#if w.load && w.load.cpu_used_millis !== null}
      {@const r = memRatio(w.load)}
      <span class="res {tone(r)}" title={loadTitle(w.load)}><span class="v">{cpuShort(w.load.cpu_used_millis)}</span><span class="v">{memShort(w.load.mem_used_bytes ?? 0)}</span><span class="mb" title="RAM / {w.load.mem_limit_bytes !== null ? t('limit') : t('request')}"><i style:width="{r ?? 0}%"></i></span></span>
    {:else}<span class="res"></span>{/if}
    <span class="upd" class:recent={!!w.updated_at && minutesSince(w.updated_at) < RECENT_ROLLOUT_HOURS * 60} title={updatedTitle(w)}>{w.updated_at ? ago(w.updated_at) : ''}</span>
    <span class="rs" class:warn={w.restarts > RESTARTS} title={t('Restarts of the current pods')}>{#if w.restarts}<Icon name="refresh" /> {w.restarts}{/if}</span>
    <span class="why">
      {#if w.waiting}<span class="b ko">{w.waiting}</span>
      {:else if w.last_exit && recentExit(w)}<span class="b warn" title={w.last_exit_at ? new Date(w.last_exit_at).toLocaleString(locale()) : ''}>{w.last_exit} · {ago(w.last_exit_at)}</span>
      {:else if w.last_exit}<span class="old" title={t('Last container exit')}>{w.last_exit} · {ago(w.last_exit_at)}</span>{/if}
    </span>
    <span class="img" title={w.drift ? t('Runs {tag}; the latest tag is {expected} (not deployed, or its image was never published)', { tag: w.image_tag ?? '', expected: w.expected ?? '' }) : (w.image_tag ?? '')}>
      <span class="ver" class:drift={w.drift}>{w.image_tag ?? '—'}</span>{#if w.drift}<span class="muted">≠ {w.expected}</span>{/if}
    </span>
    {#if canShowPodLogs()}<button class="logs" onclick={logsOf(s, w)} title={t('Logs of the pods')}>{t('Logs')}</button>{:else}<span></span>{/if}
    {#if a?.href}<a class="act" href={a.href} onclick={claudeClick(a)} title={`${a.prompt}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{a.label}</a>{:else}<span></span>{/if}
  </div>
{/snippet}

<section class="panel" style="--src: var(--src-k8s)" class:collapsed={isCollapsed('envs')}>
  <h2>
    <span class="title"><Logo name="kubernetes" />{t('Environments')}</span>
    <small>{t('pods · read-only')}</small>
    {#if names.length}
      <span class="tabs" role="group" aria-label={t('Environment')}>
        {#each names as n (n)}
          {@const bad = troubled(n)}
          <button class:on={n === env} onclick={() => setTab(n)}>{n}{#if bad}<b class="dot" title={tn(bad, '{n} workload in trouble', '{n} workloads in trouble')}>{bad}</b>{/if}</button>
        {/each}
      </span>
    {/if}
    <BlockControls id="envs" />
  </h2>
  {#if !environments}
    <div class="empty">{error ? t('Unavailable: {error}', { error }) : t('No environment configured ([[environments]] in the config), or reading the clusters…')}</div>
  {:else if isShort('envs')}
    <div class="short-body">
      {#each names as n (n)}
        {@const ws = all(n)}
        {@const bad = troubled(n)}
        <div class="srow">
          <span class="t">{n}</span>
          <span class="mono">{t('{ready}/{total} ready', { ready: ws.filter((w) => w.ready >= w.desired && !w.waiting).length, total: ws.length })}</span>
          {#if bad}<span class="pill ko">{t('{n} in trouble', { n: bad })}</span>{/if}
          {#if ws.some((w) => w.drift)}<span class="pill warn">{t('{n} behind tag', { n: ws.filter((w) => w.drift).length })}</span>{/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="kpis">
      <div class="kpi"><span>{t('Ready')}</span><b class:ko={ready < workloads.length}>{ready}/{workloads.length}</b><small>{failing ? tn(failing, '{n} namespace unreadable', '{n} namespaces unreadable') : t('workloads')}</small></div>
      <div class="kpi"><span>{t('Restarts')}</span><b class:warn={restarts > RESTARTS}>{restarts}</b><small>{t('current pods')}</small></div>
      <div class="kpi">
        <span>{t('Images')}</span>
        {#if env === stageOf('staging')?.env}<b>—</b><small>{t('release branch builds')}</small>
        {:else}<b class:warn={drifted.length > 0}>{drifted.length ? t('{n} behind', { n: drifted.length }) : t('up to date')}</b><small>{env === stageOf('preprod')?.env ? t('vs latest {stage} tag', { stage: stageOf('preprod')!.name }) : t('vs latest tag')}</small>{/if}
      </div>
    </div>
    <div class="scroll"><div class="table" {@attach columns('envs')}>
      <!-- The columns' names; their right edges resize them. -->
      <div class="wl head" data-columns-head>
        <span></span><span class="fit" {@attach grip}><span>{t('Workload')}</span></span><span {@attach grip}>Pods</span><span class="fit" {@attach grip}><span>CPU · RAM</span></span><span class="ic" title={t('Last rollout')} {@attach grip}><Icon name="rocket" /></span><span class="ic" title={t('Restarts of the current pods')} {@attach grip}><Icon name="refresh" /></span><span class="why fit" {@attach grip}><span>{t('Issue')}</span></span><span class="img fit" {@attach grip}><span>{t('Image')}</span></span><span></span><span></span>
      </div>
      {#each byCluster as cl (cl.context)}
        {@const c = cl.load}
        {@const argoApps = argoOf(cl.apps.flatMap((g) => g.items))}
        {@const argoBad = argoApps.filter((r) => argoTone(r.app) !== 'ok')}
        {@const cpu = c ? pct(c.cpu_used_millis, c.cpu_allocatable_millis) : null}
        {@const mem = c ? pct(c.mem_used_bytes, c.mem_allocatable_bytes) : null}
        {@const clKey = `cluster:${env}:${cl.context}`}
        {@const clFolded = isFolded(clKey)}
        {@const sums = cl.apps.map((g) => summary(g.items.map((x) => x.w)))}
        {@const troubled = sums.reduce((n, x) => n + x.troubled, 0)}
        <!-- A click on the cluster folds its applications; folded, it still says what is in trouble. -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="cl" class:folded={clFolded} onclick={() => fold(clKey)} title={`${c ? clusterTitle(c) : cl.context}\n${clFolded ? t('Show its applications') : t('Hide its applications')}`}>
          <span class="cn"><span class="caret" class:open={!clFolded} aria-hidden="true"><Icon name="chevronRight" /></span>{cl.name}{#if clFolded && (troubled || cl.unreadable.length)}<span class="b {sums.some((x) => x.health === 'ko') || !troubled ? 'ko' : 'warn'}">{troubled ? t('{n} in trouble', { n: troubled }) : tn(cl.unreadable.length, '{n} namespace unreadable', '{n} namespaces unreadable')}</span>{/if}<small>{cl.namespaces.length === 1 ? cl.namespaces[0].name : tn(cl.namespaces.length, '{n} namespace', '{n} namespaces')} · {tn(cl.apps.length, '{n} app', '{n} apps')} · {tn(cl.workloads, '{n} deployment', '{n} deployments')}{c?.nodes ? ` · ${tn(c.nodes, '{n} node', '{n} nodes')}` : ''}</small>{#if argoApps.length}<button class="argo-sum {argoBad.some((r) => argoTone(r.app) === 'ko') ? 'ko' : argoBad.some((r) => argoTone(r.app) === 'warn') ? 'warn' : argoBad.length ? 'info' : 'ok'}" onclick={(e) => { e.stopPropagation(); argoSummary(argoApps[0].instance.cluster) }} title={argoApps.map((r) => `${r.app.name}: ${t(r.app.sync)} · ${t(r.app.health)}`).join('\n')}><Icon name="argocd" /> {argoBad.length ? t('{bad}/{total} not synced', { bad: argoBad.length, total: argoApps.length }) : t('{n} synced', { n: argoApps.length })}</button>{/if}</span>
          {#if cpu === null && mem === null}
            <span class="unav">{c?.error ? t('load unavailable') : c ? '…' : ''}</span>
          {:else if c}
            <span class="m {tone(cpu)}"><span class="k">CPU</span><span class="bar"><i style:width="{cpu ?? 0}%"></i></span><b>{cpu === null ? '—' : `${Math.round(cpu)}%`}</b><small>{c.cpu_used_millis !== null ? `${cores(c.cpu_used_millis)}/${cores(c.cpu_allocatable_millis)}` : ''}</small></span>
            <span class="m {tone(mem)}"><span class="k">RAM</span><span class="bar"><i style:width="{mem ?? 0}%"></i></span><b>{mem === null ? '—' : `${Math.round(mem)}%`}</b><small>{c.mem_used_bytes !== null ? `${gib(c.mem_used_bytes)}/${gib(c.mem_allocatable_bytes)} GiB` : ''}</small></span>
          {/if}
        </div>
        {#if !clFolded}
        {#each cl.unreadable as s (s.namespace)}
          <p class="err" title="{s.context} · {s.error}">{t('{namespace} unreadable: {error}', { namespace: s.namespace, error: s.error ?? '' })}</p>
        {/each}
        {#each cl.namespaces as ns (ns.name)}
        <!-- A namespace's line when the cluster has several; one is named in the cluster's line. -->
        {@const several = cl.namespaces.length > 1}
        {@const nsFolded = several && isFolded(ns.key)}
        {#if several}
          {@const nsSums = ns.apps.map((g) => summary(g.items.map((x) => x.w)))}
          {@const nsBad = nsSums.reduce((n, x) => n + x.troubled, 0)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="ns" onclick={() => fold(ns.key)} title={`${cl.name} / ${ns.name}\n${nsFolded ? t('Show its applications') : t('Hide its applications')}`}>
            <span class="caret" class:open={!nsFolded} aria-hidden="true"><Icon name="chevronRight" /></span><b>{ns.name}</b><small>{tn(ns.apps.length, '{n} app', '{n} apps')} · {tn(ns.workloads, '{n} deployment', '{n} deployments')}</small>{#if nsFolded && nsBad}<span class="b {nsSums.some((x) => x.health === 'ko') ? 'ko' : 'warn'}">{t('{n} in trouble', { n: nsBad })}</span>{/if}
          </div>
        {/if}
        {#if !nsFolded}
        {#each ns.apps as g (g.key)}
          {@const folded = isFolded(g.key)}
          {@const sum = summary(g.items.map((x) => x.w))}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="wl app {sum.health}" onclick={() => fold(g.key)} title={folded ? t('Show the components') : t('Hide the components')}>
            <span class="dot-st" title={sum.health === 'ok' ? t('Every component is healthy') : tn(sum.troubled, '{n} component needs attention', '{n} components need attention')}></span>
            <span class="name"><span class="caret" class:open={!folded} aria-hidden="true"><Icon name="chevronRight" /></span><b>{g.label}</b><span class="role">{tn(g.items.length, '{n} component', '{n} components')}</span>{#each argoOf(g.items) as r (`${r.instance.cluster}/${r.instance.namespace}/${r.app.name}`)}{@const url = argoUrl(r)}<a class="argo {argoTone(r.app)}" href={url ?? undefined} target="_blank" rel="noreferrer" title={argoTitle(r)} onclick={(e) => argoClick(e, r)}><Icon name="argocd" /> {t(argoLabel(r.app))}</a>{/each}</span>
            <span class="pods agg" title={t('{ready}/{desired} pods ready', { ready: sum.ready, desired: sum.desired })}>{sum.ready}/{sum.desired}</span>
            {#if sum.cpu !== null}
              <span class="res {tone(sum.ratio)}" title={t('Sum of the components')}><span class="v">{cpuShort(sum.cpu)}</span><span class="v">{memShort(sum.mem ?? 0)}</span><span class="mb"><i style:width="{sum.ratio ?? 0}%"></i></span></span>
            {:else}<span class="res"></span>{/if}
            <span class="upd" class:recent={!!sum.updated && minutesSince(sum.updated) < RECENT_ROLLOUT_HOURS * 60} title={sum.updated ? t('Last rollout of a component {date}', { date: new Date(sum.updated).toLocaleString(locale()) }) : ''}>{sum.updated ? ago(sum.updated) : ''}</span>
            <span class="rs" class:warn={sum.restarts > RESTARTS}>{#if sum.restarts}<Icon name="refresh" /> {sum.restarts}{/if}</span>
            <span class="why">{#if sum.troubled}<span class="b {sum.health}">{t('{bad}/{total} in trouble', { bad: sum.troubled, total: g.items.length })}</span>{/if}</span>
            <span class="img"></span>
            <span></span><span></span>
          </div>
          {#if !folded}
            {#each g.items as { s, w }, k (s.context + s.namespace + w.name)}
              {@render workloadRow(s, w, k === g.items.length - 1 ? 'last' : 'mid')}
            {/each}
          {/if}
        {/each}
        {/if}
        {/each}
        {/if}
      {/each}
    </div></div>
  {/if}
</section>

<style>
  section { container-type: inline-size; }
  .tabs { display: inline-flex; gap: 2px; margin-left: auto; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .tabs button { display: inline-flex; align-items: center; gap: 4px; padding: 0 8px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .tabs button.on { background: var(--surface); color: var(--text); font-weight: 500; }
  .tabs .dot { min-width: 14px; padding: 0 3px; border-radius: var(--r7); background: var(--ko); color: var(--surface); font-size: 9.5px; line-height: 14px; text-align: center; }
  .kpis { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 1px; background: var(--border); border-bottom: 1px solid var(--border); }
  .kpi { display: flex; flex-direction: column; gap: 1px; min-width: 0; padding: 6px 12px; background: var(--surface); }
  .kpi span { font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.3px; }
  .kpi b { font-size: 16px; font-weight: 600; white-space: nowrap; }
  .kpi b.ko { color: var(--ko); }
  .kpi b.warn { color: var(--warn); }
  .kpi small { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  /* Cluster header: separates the sections, with its load. */
  .cl { position: sticky; top: 0; z-index: 1; display: grid; grid-template-columns: minmax(0, 1.2fr) minmax(0, 1fr) minmax(0, 1fr); align-items: center; gap: 12px; padding: 5px 12px; font-size: var(--fs-tag); color: var(--text-muted); background: var(--bg); border-bottom: 1px solid var(--border); box-shadow: inset 3px 0 var(--src-k8s); }
  .wl + .cl, .err + .cl { border-top: 1px solid var(--border); }
  .cn { font-weight: 600; }
  .cl { cursor: pointer; }
  .cl .caret { display: inline-flex; align-items: center; justify-content: center; width: 12px; align-self: center; font-size: 11px; color: var(--text-muted); transition: transform 0.12s; }
  .cl .caret.open { transform: rotate(90deg); }
  /* A namespace's line, under its cluster's. */
  .ns { display: flex; align-items: center; gap: 6px; padding: 4px 12px 4px 10px; font-size: var(--fs-tag); color: var(--text-muted); border-bottom: 1px solid var(--border); cursor: pointer; }
  .ns b { font-family: var(--mono); font-weight: 600; color: var(--text); }
  .ns .caret { display: inline-flex; align-items: center; justify-content: center; width: 12px; font-size: 11px; transition: transform 0.12s; }
  .ns .caret.open { transform: rotate(90deg); }
  /* ArgoCD: the Application's sync and health, as a pill; discreet green when all is well. */
  .argo, .argo-sum { flex-shrink: 0; margin-left: 6px; padding: 0 6px; border-radius: var(--r9); font-family: var(--sans); font-size: var(--fs-tag); font-weight: 500; line-height: 16px; white-space: nowrap; text-decoration: none; }
  .argo.ok, .argo-sum.ok { color: var(--ok); background: color-mix(in srgb, var(--ok-bg) 60%, transparent); }
  .argo.info, .argo-sum.info { color: var(--info); background: var(--info-bg); }
  .argo.warn, .argo-sum.warn { color: var(--warn); background: var(--warn-bg); }
  .argo.ko, .argo-sum.ko { color: var(--ko); background: var(--ko-bg); }
  a.argo:hover, button.argo-sum:hover { text-decoration: underline; }
  a.argo, button.argo-sum { cursor: pointer; }
  button.argo-sum { border: none; }
  .cn small { font-weight: 400; }
  .cn { display: flex; align-items: baseline; gap: 6px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); font-family: var(--mono); }
  .cn small, .m small { color: var(--text-muted); font-family: var(--mono); }
  /* On the phone: the cluster's details and its ArgoCD summary go to the next line rather than cut. */
  :global(html.remote) .cn { flex-wrap: wrap; white-space: normal; row-gap: 2px; }
  .unav { grid-column: span 2; }
  .m { display: grid; grid-template-columns: 28px minmax(30px, 1fr) 34px auto; align-items: center; gap: 6px; min-width: 0; --fill: var(--src-k8s); white-space: nowrap; }
  .m.warn { --fill: var(--warn); }
  .m.ko { --fill: var(--ko); }
  .m .k { text-transform: uppercase; letter-spacing: 0.3px; }
  .m .bar { height: 6px; border-radius: var(--r3); overflow: hidden; background: var(--surface-muted); }
  .m .bar i { display: block; height: 100%; background: var(--fill); }
  .m b { text-align: right; font-weight: 600; color: var(--text); font-variant-numeric: tabular-nums; }
  .m.warn b { color: var(--warn); }
  .m.ko b { color: var(--ko); }
  .err { margin: 0; padding: 4px 12px; background: var(--warn-bg); color: var(--warn); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /*
   * A single grid for all rows (subgrid): each column takes the width of its
   * widest content, a column empty everywhere takes nothing, and the name comes before the
   * reason for a problem. Columns: state, name, pods, CPU/RAM, update, restarts,
   * reason, image, Logs, Claude.
   */
  .table { display: grid; grid-template-columns: 20px minmax(120px, 2fr) auto auto auto auto minmax(0, 1fr) auto auto auto; column-gap: 10px; }
  /* Widths set by hand (columns.svelte.ts): over the block's own, whatever its width. */
  .table:global([data-sized]) { grid-template-columns: var(--cols); }
  .table:global([data-sized]) .wl:not(.head) > * { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .table > :global(*) { grid-column: 1 / -1; }
  .wl.head { padding: 4px 0; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); white-space: nowrap; }
  .wl.head:hover { background: none; }
  /* The names follow the columns, they do not widen them. */
  .wl.head > .fit { contain: inline-size; }
  .wl.head > .fit > span { display: block; overflow: hidden; text-overflow: ellipsis; }
  .wl.head .ic { display: inline-flex; }
  .wl.head .ic :global(svg) { width: 11px; height: 11px; }
  /* Side margins are in the 1st column (the dot, aligned right) and on the last
     cell: padding on a subgrid row shifts its cells. */
  .wl > :last-child { margin-right: 12px; }
  .wl > .dot-st { justify-self: end; }
  .wl { display: grid; grid-template-columns: subgrid; align-items: center; padding: 3px 0; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .wl.app { cursor: pointer; background: color-mix(in srgb, var(--src-k8s) 4%, var(--surface)); }
  .wl.app .caret { display: inline-flex; align-items: center; justify-content: center; width: 12px; font-size: 11px; color: var(--text-muted); transition: transform 0.12s; }
  .wl.app .caret.open { transform: rotate(90deg); }
  .wl.child { border-bottom-color: color-mix(in srgb, var(--border) 50%, transparent); }
  .wl.child .name { padding-left: 14px; }
  .tree { width: 12px; flex-shrink: 0; color: var(--text-muted); font-family: var(--mono); }
  .comp { font-family: var(--mono); font-size: var(--fs-mono); font-weight: 500; }
  .pods.agg { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .wl:hover { background: color-mix(in srgb, var(--accent) 5%, var(--surface)); }
  .dot-st { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); }
  .wl.warn .dot-st { background: var(--warn); }
  .wl.ko .dot-st { background: var(--ko); }
  .name { display: flex; align-items: baseline; gap: 6px; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .name b { font-weight: 500; }
  button.name { padding: 0; border: none; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  button.name:hover b { color: var(--src-k8s); text-decoration: underline; text-underline-offset: 2px; }
  .logs { padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; opacity: 0; }
  .wl:hover .logs, .wl.ko .logs, .logs:focus-visible { opacity: 1; }
  .logs:hover { color: var(--text); border-color: color-mix(in srgb, var(--src-k8s) 60%, var(--border)); }
  .role { color: var(--text-muted); font-size: var(--fs-tag); font-family: var(--mono); }
  .pods { display: inline-flex; justify-content: flex-end; gap: 2px; overflow: hidden; }
  .pods.agg { justify-content: flex-end; }
  .pods i { width: 6px; height: 12px; border-radius: var(--r2); background: var(--ko); }
  .pods i.up { background: var(--ok); }
  .res { display: grid; grid-template-columns: 38px 44px 14px; align-items: center; gap: 4px; font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; --fill: var(--src-k8s); }
  .res .v { text-align: right; }
  .res .mb { height: 5px; border-radius: var(--r2); overflow: hidden; background: var(--surface-muted); }
  .res .mb i { display: block; height: 100%; background: var(--fill); }
  .res.warn { --fill: var(--warn); color: var(--warn); }
  .res.ko { --fill: var(--ko); color: var(--ko); }
  .upd { font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; text-align: right; }
  .upd.recent { color: var(--info); font-weight: 500; }
  .rs { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .rs.warn { color: var(--warn); }
  .why { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; font-size: var(--fs-tag); }
  .b { padding: 0 6px; border-radius: var(--r9); white-space: nowrap; }
  .b.ko { background: var(--ko-bg); color: var(--ko); }
  .b.warn { background: var(--warn-bg); color: var(--warn); }
  .old { color: var(--text-muted); }
  .img { display: inline-flex; align-items: center; gap: 4px; min-width: 0; overflow: hidden; white-space: nowrap; font-size: var(--fs-tag); }
  .ver { padding: 0 5px; border-radius: var(--r4); background: var(--surface-muted); font-family: var(--mono); max-width: 150px; overflow: hidden; text-overflow: ellipsis; }
  .ver.drift { background: var(--warn-bg); color: var(--warn); }
  .muted { color: var(--text-muted); }
  .act { display: inline-flex; justify-self: end; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--src-claude) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; }
  .act:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); text-decoration: none; }
  /* On the phone the last columns have no room: Logs and the agent's action go on a line of their own, under the name. */
  :global(html.remote) .wl .logs { grid-column: 2; justify-self: start; margin: 3px 0; }
  :global(html.remote) .wl .act { grid-column: 3 / -1; justify-self: start; margin: 3px 0; }
  /* Narrow block: state, pods and action; the rest is in the dot's tooltip. */
  /* Medium block: the reason for a problem moves into the dot's tooltip. Narrow: the image too. */
  @container (max-width: 820px) {
    .table { grid-template-columns: 20px minmax(120px, 2fr) auto auto auto auto 0 auto auto auto; }
    .why { display: none; }
  }
  @container (max-width: 560px) {
    .table { grid-template-columns: 20px minmax(100px, 1fr) auto auto auto auto 0 0 auto auto; }
    .why, .img { display: none; }
    .cl { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
    .cn { grid-column: span 2; }
    .m small { display: none; }
  }
</style>
