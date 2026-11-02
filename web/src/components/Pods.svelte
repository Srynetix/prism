<script lang="ts">
  import BlockControls from './BlockControls.svelte'
  import { t } from '../i18n.svelte'
  import { locale } from '../locale'
  import { isCollapsed, isShort } from '../layout.svelte'
  import Icon from './Icon.svelte'
  import Logo from './Logo.svelte'
  import type { NodeLoad, PodView, Usage } from '../types'
  import { ago, cpu, mem } from '../time'
  import { podDown, usageKey } from '../rules'
  import { canShowPodLogs, openPods } from '../pod-logs.svelte'
  import { claudeClick, investigateAction } from '../claude-actions'
  import { keys } from '../platform'
  import { openLink } from '../links'
  let { pods, usage, nodes = [] }: { pods: PodView[]; usage: Record<string, Usage>; nodes?: NodeLoad[] } = $props()

  /**
   * Pods: what needs a look first (failing and pending pods, why, their actions), then each target
   * with its nodes' load on one line and one line per workload (its replicas folded, a click
   * unfolds them) or CI job. Short: the counters and what needs a look.
   */
  /** A finished pod (a CI job's, a Job's) is no longer shown. */
  const live = $derived(pods.filter((p) => p.status !== 'Succeeded'))
  const attention = $derived(live.filter((p) => !p.healthy).sort((a, b) => Number(podDown(b)) - Number(podDown(a))))
  const failing = $derived(live.filter(podDown).length)
  const pending = $derived(live.filter((p) => !p.healthy && !podDown(p)).length)
  const jobs = $derived(live.filter((p) => p.role === 'job').length)
  const totalCpu = $derived(Object.values(usage).reduce((n, u) => n + u.cpu_millis, 0))
  const totalMem = $derived(Object.values(usage).reduce((n, u) => n + u.mem_bytes, 0))
  const targets = $derived(new Set(pods.map((p) => p.target)).size)

  /** A line: a workload's pods (its replicas) or a pod on its own. */
  interface Line {
    key: string
    name: string
    pods: PodView[]
  }
  /** By target: its CI jobs, then each namespace's workloads, in the snapshot's order. */
  const groups = $derived.by(() => {
    const out: { target: string; jobs: PodView[]; namespaces: { name: string; lines: Line[] }[] }[] = []
    for (const p of live) {
      let tg = out.find((g) => g.target === p.target)
      if (!tg) out.push((tg = { target: p.target, jobs: [], namespaces: [] }))
      if (p.role === 'job') {
        tg.jobs.push(p)
        continue
      }
      let ns = tg.namespaces.find((n) => n.name === p.namespace)
      if (!ns) tg.namespaces.push((ns = { name: p.namespace, lines: [] }))
      const key = `${p.target}/${p.namespace}/${p.workload ?? p.name}`
      const line = ns.lines.find((l) => l.key === key)
      if (line) line.pods.push(p)
      else ns.lines.push({ key, name: p.workload ?? p.name, pods: [p] })
    }
    return out
  })

  let open = $state<Record<string, boolean>>({})

  const pct = (v: number | undefined, limit: number | null) => (v !== undefined && limit ? Math.min(100, (v / limit) * 100) : null)
  const level = (p: number | null) => (p === null ? '' : p >= 90 ? 'crit' : p >= 70 ? 'hi' : '')
  /** Share of a quantity in the node's allocatable, in %. */
  const share = (v: number | null, of: number | null) => (v !== null && of ? Math.min(100, (v / of) * 100) : null)
  const jobLabel = (p: PodView) => p.job_ref?.replace(/^refs\/merge-requests\/(\d+)\/head$/, '!$1') ?? null
  const nameOf = (p: PodView) => (p.role === 'job' ? (p.job_name ?? p.name) : (p.workload ?? p.name))
  /** The dot: ok, pending (hollow), failing. */
  const tone = (p: PodView) => (p.healthy ? 'ok' : podDown(p) ? 'ko' : 'pend')
  const worst = (ps: PodView[]) => (ps.some(podDown) ? 'ko' : ps.some((p) => !p.healthy) ? 'pend' : 'ok')
  /** Why it needs a look, in words: the window's language; `say = plain` for the agent's prompt, in English. */
  const why = (p: PodView, say: typeof t = t) =>
    [
      p.restarts ? (p.restarts > 1 ? say('{n} restarts', { n: p.restarts }) : say('{n} restart', { n: p.restarts })) : '',
      p.last_exit ? `${p.last_exit}${p.last_exit_at ? say(', {ago} ago', { ago: ago(p.last_exit_at) }) : ''}` : '',
      p.unscheduled ? say('waiting for a node: {reason}', { reason: p.unscheduled }) : '',
    ]
      .filter(Boolean)
      .join(' · ')
  const plain = (text: string, vars?: Record<string, string | number>) => text.replace(/\{(\w+)\}/g, (_, k: string) => String(vars?.[k] ?? ''))
  const issue = (p: PodView) => `pod ${p.name} is ${p.status}${why(p, plain) ? ` (${why(p, plain)})` : ''}`
  const siblings = (p: PodView) => live.filter((q) => q.target === p.target && q.namespace === p.namespace && (q.workload ?? q.name) === (p.workload ?? p.name))
  const logs = (p: PodView) => () => openPods(p.context, p.namespace, p.workload ?? '', `${nameOf(p)} · ${p.target}`, siblings(p), p.name)
  /** A pod's logs: once it runs somewhere. */
  const hasLogs = (p: PodView) => canShowPodLogs() && !!p.node

  /** A line's usage: the highest of its pods, in % of their limit; the exact values in its title. */
  function load(ps: PodView[], kind: 'cpu' | 'mem') {
    let top: number | null = null
    const parts: string[] = []
    for (const p of ps) {
      const u = usage[usageKey(p)]
      const v = u ? (kind === 'cpu' ? u.cpu_millis : u.mem_bytes) : undefined
      const limit = kind === 'cpu' ? p.cpu_limit_millis : p.mem_limit_bytes
      const f = kind === 'cpu' ? cpu : mem
      const r = pct(v, limit)
      if (r !== null) top = Math.max(top ?? 0, r)
      parts.push(`${ps.length > 1 ? `${p.name}: ` : ''}${v !== undefined ? f(v) : '—'}${limit ? ` / ${f(limit)}` : ''}`)
    }
    return { pct: top, title: `${t('{kind} used / limit', { kind: kind === 'cpu' ? 'CPU' : 'RAM' })}\n${parts.join('\n')}` }
  }
  const nodeTitle = (n: NodeLoad, kind: 'cpu' | 'mem') => {
    const [used, req, alloc, f] = kind === 'cpu' ? [n.cpu_used_millis, n.cpu_requested_millis, n.cpu_allocatable_millis, cpu] : [n.mem_used_bytes, n.mem_requested_bytes, n.mem_allocatable_bytes, mem]
    return `${n.name} · ${kind === 'cpu' ? 'CPU' : 'RAM'}\n${t('Used {used} · requested {requested}', { used: used !== null ? f(used) : '—', requested: f(req) })}${alloc ? ` · ${t('allocatable {value}', { value: f(alloc) })}` : ''}`
  }
</script>

{#snippet gauge(n: NodeLoad, kind: 'cpu' | 'mem')}
  {@const u = share(kind === 'cpu' ? n.cpu_used_millis : n.mem_used_bytes, kind === 'cpu' ? n.cpu_allocatable_millis : n.mem_allocatable_bytes)}
  {@const r = share(kind === 'cpu' ? n.cpu_requested_millis : n.mem_requested_bytes, kind === 'cpu' ? n.cpu_allocatable_millis : n.mem_allocatable_bytes)}
  {@const l = level(Math.max(u ?? 0, r ?? 0))}
  <!-- Requests faint behind, actual usage on top; the colour follows the higher one. -->
  <span class="g" title={nodeTitle(n, kind)}>
    <span>{kind === 'cpu' ? 'CPU' : 'RAM'}</span>
    <span class="bar"><i class="req {l}" style:width="{r ?? 0}%"></i><i class={l} style:width="{u ?? 0}%"></i></span>
    <span class="v">{u !== null ? `${Math.round(u)}%` : '—'}</span>
  </span>
{/snippet}

{#snippet mini(ps: PodView[], kind: 'cpu' | 'mem')}
  {@const m = load(ps, kind)}
  <span class="mini" title={m.title}>
    <span>{kind === 'cpu' ? 'cpu' : 'ram'}<em>{m.pct !== null ? `${Math.round(m.pct)}%` : '—'}</em></span>
    <span class="bar"><i class={level(m.pct)} style:width="{m.pct ?? 0}%"></i></span>
  </span>
{/snippet}

{#snippet podName(p: PodView, text: string)}
  {#if p.role === 'job' && jobLabel(p)}<span class="chip mono">{jobLabel(p)}</span>{/if}
  {#if hasLogs(p)}
    <button class="mono link" onclick={logs(p)} title="{p.name} · {t('open the logs')}">{text}</button>
  {:else}
    <span class="mono" title={p.name}>{text}</span>
  {/if}
{/snippet}

{#snippet card(p: PodView)}
  {@const down = podDown(p)}
  {@const a = down && p.role !== 'job' ? investigateAction(p.workload ?? p.name, p.namespace, p.context, p.target, issue(p)) : null}
  <div class="card" class:warn={!down}>
    <span class="dot {tone(p)}"></span>
    <span class="head">
      {@render podName(p, nameOf(p))}
      <span class="pill" class:ko={down} class:warn={!down}>{p.status}{#if !down && p.started_at}{` ${ago(p.started_at)}`}{/if}</span>
    </span>
    <span class="acts">
      {#if hasLogs(p)}<button class="btn" onclick={logs(p)} title={p.last_exit ? t('Logs of {name}, before its last restart', { name: p.name }) : t('Logs of {name}', { name: p.name })}>{t('Logs')}</button>{/if}
      {#if p.job_url}<button class="btn" onclick={() => openLink(p.job_url!)} title={t('Open the CI job')}>{t('Job')} ↗</button>{/if}
      {#if a?.href}<a class="btn act" href={a.href} onclick={claudeClick(a)} title={`${a.prompt}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}><Logo name="agent" size={10} />{a.label}</a>{/if}
    </span>
    <span class="what" title={p.unscheduled ?? ''}>{[targets > 1 ? p.target : '', p.namespace, why(p)].filter(Boolean).join(' · ')}</span>
  </div>
{/snippet}

{#snippet row(p: PodView, text: string, sub: boolean)}
  <div class="row" class:sub>
    <span class="dot {tone(p)}" title={p.status}></span>
    <span class="n">{@render podName(p, text)}{#if p.restarts}<small class:ko={podDown(p)} title={t('Restarts')}>{p.restarts}<Icon name="refresh" /></small>{/if}</span>
    {@render mini([p], 'cpu')}
    {@render mini([p], 'mem')}
    <span class="age" title={p.started_at ? t('Started {date}', { date: new Date(p.started_at).toLocaleString(locale()) }) : p.status}>{p.started_at ? ago(p.started_at) : '—'}</span>
  </div>
{/snippet}

<section class="panel" style="--src: var(--src-k8s)" class:collapsed={isCollapsed('pods')}>
  <h2>
    <span class="title"><Logo name="kubernetes" />Pods</span>
    <small>{targets > 1 ? t('{n} targets', { n: targets }) : (pods[0]?.target ?? '')}</small>
    {#if totalCpu || totalMem}<small class="total" title={t('Used by the pods shown')}>CPU {cpu(totalCpu)} · RAM {mem(totalMem)}</small>{/if}
    <BlockControls id="pods" />
  </h2>
  <div class="stats">
    <span><b>{live.length}</b>{t('pods')}</span>
    <span class:ko={failing > 0}><b>{failing}</b>{t('failing')}</span>
    <span class:warn={pending > 0}><b>{pending}</b>{t('pending')}</span>
    {#if jobs}<span><b>{jobs}</b>{jobs > 1 ? t('CI jobs') : t('CI job')}</span>{/if}
  </div>
  {#if attention.length}
    <div class="attn" class:short={isShort('pods')}>
      {#each attention as p (`${p.target}/${p.namespace}/${p.name}`)}{@render card(p)}{/each}
    </div>
  {/if}
  {#if !isShort('pods')}
    <div class="scroll">
      <div class="targets">
        {#each groups as g (g.target)}
          {@const mine = nodes.filter((n) => n.target === g.target)}
          <div class="target">
            <div class="tgt">
              <span class="t">{g.target}{#if mine.length === 1}<small class="mono" title="{mine[0].name}{mine[0].error ? `\n${mine[0].error}` : ''}">{mine[0].name} · {mine[0].pods}{mine[0].max_pods ? `/${mine[0].max_pods}` : ''}</small>{/if}</span>
              {#if mine.length === 1}{@render gauge(mine[0], 'cpu')}{@render gauge(mine[0], 'mem')}{/if}
            </div>
            {#if mine.length > 1}
              {#each mine as n (n.name)}
                <div class="tgt node">
                  <span class="t mono" title="{n.name}{n.error ? `\n${n.error}` : ''}"><Icon name="kube" /> {n.name}<small>{n.pods}{n.max_pods ? `/${n.max_pods}` : ''}</small></span>
                  {@render gauge(n, 'cpu')}{@render gauge(n, 'mem')}
                </div>
              {/each}
            {/if}
            {#if g.jobs.length}
              <div class="grp">{t('CI jobs')}</div>
              {#each g.jobs as p (p.name)}{@render row(p, nameOf(p), false)}{/each}
            {/if}
            {#each g.namespaces as ns (ns.name)}
              <div class="grp"><span class="mono">{ns.name}</span></div>
              {#each ns.lines as l (l.key)}
                {#if l.pods.length === 1}
                  {@render row(l.pods[0], l.name, false)}
                {:else}
                  {@const ready = l.pods.filter((p) => p.healthy).length}
                  {@const restarts = l.pods.reduce((n, p) => n + p.restarts, 0)}
                  <div class="row">
                    <span class="dot {worst(l.pods)}"></span>
                    <button class="n fold" onclick={() => (open[l.key] = !open[l.key])} aria-expanded={!!open[l.key]} title="{t('{n} pods', { n: l.pods.length })} · {open[l.key] ? t('fold') : t('unfold')}">
                      <Icon name={open[l.key] ? 'chevronDown' : 'chevronRight'} /><span class="mono">{l.name}</span>
                      <span class="rep" class:ko={ready < l.pods.length}>{ready}/{l.pods.length}</span>
                      {#if restarts}<small title={t('Restarts')}>{restarts}<Icon name="refresh" /></small>{/if}
                    </button>
                    {@render mini(l.pods, 'cpu')}
                    {@render mini(l.pods, 'mem')}
                    <span class="age">{ago(l.pods.map((p) => p.started_at).filter(Boolean).sort().at(-1))}</span>
                  </div>
                  {#if open[l.key]}{#each l.pods as p (p.name)}{@render row(p, l.name && p.name.startsWith(`${l.name}-`) ? p.name.slice(l.name.length + 1) : p.name, true)}{/each}{/if}
                {/if}
              {/each}
            {/each}
          </div>
        {:else}
          <div class="empty">{t('No pod.')}</div>
        {/each}
      </div>
    </div>
  {/if}
</section>

<style>
  h2 .total { margin-left: auto; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .stats { display: flex; flex-wrap: wrap; gap: 4px 14px; padding: 6px 12px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); }
  .stats b { margin-right: 3px; color: var(--text); font-weight: 600; font-variant-numeric: tabular-nums; }
  .stats .ko, .stats .ko b { color: var(--ko); }
  .stats .warn, .stats .warn b { color: var(--warn); }

  /* Needs a look: first, whatever the target. */
  .attn { display: flex; flex-wrap: wrap; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--ko) 5%, transparent); }
  .attn.short { max-height: 220px; overflow: auto; }
  .card { flex: 1 1 300px; min-width: 0; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 2px 8px; align-items: center; padding: 5px 8px; border: 1px solid color-mix(in srgb, var(--ko) 35%, var(--border)); border-left: 3px solid var(--ko); border-radius: var(--r6); background: var(--surface); font-size: var(--fs-body); }
  .card.warn { border-color: color-mix(in srgb, var(--warn) 35%, var(--border)); border-left-color: var(--warn); }
  .card .head { display: flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .card .what { grid-column: 2 / 4; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-meta); color: var(--text-muted); }
  .acts { display: flex; gap: 4px; }
  .btn { display: inline-flex; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface-muted); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; cursor: pointer; }
  .btn:hover { border-color: color-mix(in srgb, var(--src-k8s) 60%, var(--border)); text-decoration: none; }
  .btn.act { border-color: color-mix(in srgb, var(--src-claude) 60%, var(--border)); background: var(--surface); }
  .btn.act:hover { background: color-mix(in srgb, var(--src-claude) 12%, var(--surface)); }

  /* Targets: one under the other, side by side when the block is wide. */
  .targets { display: grid; grid-template-columns: repeat(auto-fit, minmax(340px, 1fr)); }
  .target { min-width: 0; border-right: 1px solid var(--border); }
  .tgt { display: grid; grid-template-columns: minmax(0, 1fr) 110px 110px; gap: 10px; align-items: center; padding: 5px 12px; border-bottom: 1px solid var(--border); background: var(--surface-muted); font-size: var(--fs-meta); }
  .tgt .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; color: var(--text); }
  .tgt .t small { margin-left: 6px; font-weight: 400; color: var(--text-muted); }
  .tgt.node { background: none; }
  .tgt.node .t { display: flex; align-items: center; gap: 4px; font-weight: 400; }
  .g { display: grid; grid-template-columns: 24px 1fr 30px; gap: 4px; align-items: center; font-size: var(--fs-tag); color: var(--text-muted); }
  .g .bar { position: relative; height: 6px; }
  .g .bar i { position: absolute; left: 0; top: 0; bottom: 0; }
  .g .bar i.req { opacity: 0.35; }
  .g .v { text-align: right; font-family: var(--mono); color: var(--text); }
  .grp { padding: 5px 12px 2px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); }
  .grp .mono { text-transform: none; letter-spacing: 0; }

  /* One line per workload, CI job or pod. */
  .row { display: grid; grid-template-columns: 10px minmax(0, 1fr) 60px 60px 32px; gap: 8px; align-items: center; padding: 3px 12px; font-size: var(--fs-body); }
  .row:hover { background: var(--surface-muted); }
  .row.sub { padding-left: 30px; font-size: var(--fs-meta); }
  .n { display: flex; align-items: center; gap: 5px; min-width: 0; overflow: hidden; white-space: nowrap; }
  .n > .mono, .n .link { overflow: hidden; text-overflow: ellipsis; }
  .n small { display: inline-flex; align-items: center; gap: 1px; font-size: var(--fs-tag); color: var(--text-muted); }
  .n small.ko { color: var(--ko); }
  .n small :global(svg) { width: 10px; height: 10px; }
  .fold { padding: 0; border: none; background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .fold :global(svg) { flex-shrink: 0; width: 11px; height: 11px; color: var(--text-muted); }
  .link { padding: 0; border: none; background: none; color: var(--text); font-size: inherit; cursor: pointer; }
  .link:hover { color: var(--info); }
  .chip { flex-shrink: 0; padding: 0 5px; border-radius: var(--r4); background: color-mix(in srgb, var(--info) 15%, transparent); color: var(--info); font-size: var(--fs-tag); }
  .rep { flex-shrink: 0; padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .rep.ko { color: var(--ko); }
  .age { text-align: right; font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); }
  .dot.pend { background: none; border: 1.5px solid var(--warn); }
  .dot.ko { background: var(--ko); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ko) 20%, transparent); }
  .mini { display: grid; gap: 1px; }
  .mini > span:first-child { display: flex; justify-content: space-between; font: 10px var(--mono); color: var(--text-muted); }
  .mini em { font-style: normal; }
  .mini .bar { height: 3px; }
  .bar { border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .tgt .bar { background: color-mix(in srgb, var(--text) 10%, transparent); }
  .bar i { display: block; height: 100%; background: var(--ok); }
  .bar i.hi { background: var(--warn); }
  .bar i.crit { background: var(--ko); }
  .empty { padding: 14px 16px; color: var(--text-muted); font-size: var(--fs-body); }
</style>
