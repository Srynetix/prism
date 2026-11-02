<script lang="ts">
  import { armer } from '../../armed.svelte'
  import { hasCore } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import Logo from '../../components/Logo.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import { ago } from '../../time'
  import { t, tn, tc } from '../../i18n.svelte'
  import { openLogs } from '../../pod-logs.svelte'
  import { call, type BlockProps } from '../block'
  import { containerFailing } from '.'
  import type { DockerContainer, DockerDisk, DockerState } from './types'

  /**
   * Docker: the disk per kind (images, containers, volumes, build cache) and what each frees, then
   * the containers by Compose project, the images or the volumes. A container starts, stops,
   * restarts, shows its log, or goes once stopped; an unused image goes; each kind's unused data
   * is pruned. Removals take two clicks (Tauri has no confirm()).
   */
  let { state: docker, error }: BlockProps<DockerState> = $props()
  const enabled = $derived(docker !== undefined)

  type Tab = 'containers' | 'images' | 'volumes'
  let tab = $state<Tab>('containers')
  let busy = $state<string | null>(null)
  let failed = $state<{ key: string; message: string } | null>(null)
  let done = $state<string | null>(null)

  /** Two clicks: the first arms the button for 3 s, the second acts. */
  const arming = armer()

  async function run(key: string, command: string, args: Record<string, string>) {
    busy = key
    failed = null
    done = null
    try {
      const out = await call<string | null>('docker', command, args)
      if (typeof out === 'string') done = out
    } catch (e) {
      failed = { key, message: String(e) }
    } finally {
      busy = null
    }
  }
  const act = (c: DockerContainer, action: 'start' | 'stop' | 'restart' | 'remove') => {
    if ((action === 'remove' || action === 'stop') && !arming.confirm(`${action}:${c.id}`)) return
    void run(`${action}:${c.id}`, 'container', { id: c.id, action })
  }
  const removeImage = (id: string) => arming.confirm(`rmi:${id}`) && void run(`rmi:${id}`, 'remove_image', { id })
  const prune = (kind: DockerDisk['kind']) => arming.confirm(`prune:${kind}`) && void run(`prune:${kind}`, 'prune', { kind })

  const up = $derived(!!docker?.server)
  const containers = $derived(docker?.containers ?? [])
  const running = $derived(containers.filter((c) => c.state === 'running').length)
  const failing = $derived(containers.filter(containerFailing))
  const reclaimable = $derived((docker?.disk ?? []).reduce((n, k) => n + k.reclaimable_bytes, 0))
  const total = $derived((docker?.disk ?? []).reduce((n, k) => n + k.size_bytes, 0))
  /** By Compose project (those without one last), in the snapshot's order. */
  const projects = $derived.by(() => {
    const out: { name: string | null; containers: DockerContainer[] }[] = []
    for (const c of containers) {
      let g = out.find((p) => p.name === c.project)
      if (!g) out.push((g = { name: c.project, containers: [] }))
      g.containers.push(c)
    }
    return out.sort((a, b) => (a.name === null ? 1 : b.name === null ? -1 : 0))
  })

  const KINDS: Record<DockerDisk['kind'], { label: string; prune: string; what: string }> = {
    images: { label: t('Images'), prune: t('Prune dangling'), what: t('the untagged images no container uses') },
    containers: { label: t('Containers'), prune: t('Prune stopped'), what: t('every stopped container') },
    volumes: { label: t('Volumes'), prune: t('Prune unused'), what: t('every volume no container uses, named ones included: their data is lost') },
    build_cache: { label: t('Build cache'), prune: t('Prune'), what: t('the build cache') },
  }
  /** Decimal, as Docker counts: `4.0 GB`, `512 MB`. */
  const size = (b: number | null | undefined) => (b === null || b === undefined ? '—' : b >= 1e9 ? t('{n} GB', { n: (b / 1e9).toFixed(1) }) : b >= 1e6 ? t('{n} MB', { n: Math.round(b / 1e6) }) : b >= 1e3 ? t('{n} kB', { n: Math.round(b / 1e3) }) : t('{n} B', { n: b }))
  const tone = (c: DockerContainer) => (containerFailing(c) ? 'ko' : c.state === 'running' ? (c.health === 'starting' ? 'warn' : 'ok') : '')
  const logs = (c: DockerContainer) => {
    const group = projects.find((p) => p.name === c.project)
    const items = (group?.name ? group.containers : [c]).map((x) => ({ id: x.id, name: x.name, running: x.state === 'running', status: x.status, created_at: x.created_at }))
    // `docker logs` gives the last 1000 lines (`LOG_LINES`).
    void openLogs(c.project ?? c.name, items, c.id, (id) => call<string>('docker', 'logs', { id }), 1000)
  }
</script>

<section class="panel" style="--src: var(--info)" class:collapsed={isCollapsed('docker')}>
  <h2>
    <span class="title"><Logo name="docker" />Docker</span>
    <span class="head">
      {#if up}<small title="{t('docker context {name}', { name: docker?.context ?? '' })}{docker?.server ? ` · Engine ${docker.server}` : ''}">{t('{n}/{total} running', { n: running, total: containers.length })} · {size(total)}</small>{/if}
      {#if up && !isShort('docker')}
        <span class="seg" role="radiogroup" aria-label={t('Show')}>
          {#each ['containers', 'images', 'volumes'] as const as tb (tb)}
            <button role="radio" aria-checked={tab === tb} class:on={tab === tb} onclick={() => (tab = tb)}>{t(tb[0].toUpperCase() + tb.slice(1))}</button>
          {/each}
        </span>
      {/if}
    </span>
    <BlockControls id="docker" />
  </h2>
  {#if !enabled}
    <div class="empty">{t('Docker is not read. Turn on Settings › Infra › Docker, then restart Prism.')}</div>
  {:else if !docker || (!up && !error)}
    <div class="empty">{t('Reading Docker…')}</div>
  {:else if !up}
    <div class="empty">{docker.context ? t('Docker does not answer (context {name}): is Docker Desktop, OrbStack or colima running?', { name: docker.context }) : t('Docker does not answer: is Docker Desktop, OrbStack or colima running?')}{#if error}<br /><small>{error}</small>{/if}</div>
  {:else if isShort('docker')}
    <div class="short-body">
      {#each failing as c (c.id)}
        <div class="srow"><span class="t mono">{c.name}</span><span class="pill ko">{c.health === 'unhealthy' ? t('unhealthy') : c.state === 'exited' ? tc('blocks', 'exit {code}', { code: c.exit_code ?? '' }) : t(c.state)}</span></div>
      {:else}
        <div class="srow more">{t('{n} running', { n: running })}{reclaimable ? ` · ${t('{size} reclaimable', { size: size(reclaimable) })}` : ''}</div>
      {/each}
    </div>
  {:else}
    <div class="disk">
      {#each docker.disk as k (k.kind)}
        {@const share = k.size_bytes ? (k.reclaimable_bytes / k.size_bytes) * 100 : 0}
        <div class="kind">
          <span class="kl">{KINDS[k.kind].label} <span class="muted">{k.active}/{k.total}</span></span>
          <span class="bar" title={t('{part} of {size} reclaimable', { part: size(k.reclaimable_bytes), size: size(k.size_bytes) })}><i style="width: {100 - share}%"></i></span>
          <span class="mono num">{size(k.size_bytes)}</span>
          {#if k.reclaimable_bytes > 0 && hasCore()}
            <button class="act" class:armed={arming.is(`prune:${k.kind}`)} disabled={busy !== null} onclick={() => prune(k.kind)} title={arming.is(`prune:${k.kind}`) ? t('Click again: removes {what}', { what: KINDS[k.kind].what }) : t('Free {size}: removes {what}', { size: size(k.reclaimable_bytes), what: KINDS[k.kind].what })}>
              {busy === `prune:${k.kind}` ? '…' : arming.is(`prune:${k.kind}`) ? t('Sure?') : `${KINDS[k.kind].prune} · ${size(k.reclaimable_bytes)}`}
            </button>
          {:else}<span></span>{/if}
        </div>
      {/each}
      {#if docker.disk_at}<div class="measured muted">{t('Measured {time}', { time: ago(docker.disk_at) })}{reclaimable ? ` · ${t('{size} reclaimable in all', { size: size(reclaimable) })}` : ''}</div>{/if}
    </div>
    {#if failed}<p class="err" title={failed.message}>{failed.message}</p>{/if}
    {#if done}<p class="ok-msg">{done}</p>{/if}
    <div class="scroll">
      {#if tab === 'containers'}
        {#each projects as p (p.name ?? '')}
          <div class="section">{p.name ?? t('Standalone')} <span class="muted">{p.containers.filter((c) => c.state === 'running').length}/{p.containers.length}</span></div>
          {#each p.containers as c (c.id)}
            <div class="row">
              <span class="dot {tone(c)}"></span>
              <span class="main">
                <span class="mono name" title={c.name}>{c.name}</span>
                <span class="sub muted" title={c.image}>{c.image}{#if c.ports.length}{' '}· {c.ports.join(', ')}{/if}</span>
              </span>
              <span class="pill {tone(c)}" title={c.status}>{c.health === 'unhealthy' ? t('unhealthy') : c.state === 'exited' ? t('exited ({code})', { code: c.exit_code ?? '' }) : t(c.state)}</span>
              <span class="mono num" title="{t('CPU · memory')}{c.size_bytes !== null ? ` · ${t('writable layer {size}', { size: size(c.size_bytes) })}` : ''}">{c.cpu_percent !== null ? `${c.cpu_percent.toFixed(1)}%` : ''}{c.mem_bytes !== null ? ` · ${size(c.mem_bytes)}` : ''}</span>
              <span class="acts">
                {#if hasCore()}
                  <button class="icon" onclick={() => logs(c)} title={t('Log')} aria-label={t('Log of {name}', { name: c.name })}><Icon name="logs" /></button>
                  {#if c.state === 'running'}
                    <button class="icon" disabled={busy !== null} onclick={() => act(c, 'restart')} title={t('Restart')} aria-label={t('Restart {name}', { name: c.name })}><Icon name="retry" /></button>
                    <button class="icon" class:armed={arming.is(`stop:${c.id}`)} disabled={busy !== null} onclick={() => act(c, 'stop')} title={arming.is(`stop:${c.id}`) ? t('Click again to stop it') : t('Stop')} aria-label={t('Stop {name}', { name: c.name })}><Icon name="stop" /></button>
                  {:else}
                    <button class="icon" disabled={busy !== null} onclick={() => act(c, 'start')} title={t('Start')} aria-label={t('Start {name}', { name: c.name })}><Icon name="play" /></button>
                    <button class="icon" class:armed={arming.is(`remove:${c.id}`)} disabled={busy !== null} onclick={() => act(c, 'remove')} title={arming.is(`remove:${c.id}`) ? t('Click again to remove it') : t('Remove')} aria-label={tc('blocks', 'Remove {name}', { name: c.name })}><Icon name="trash" /></button>
                  {/if}
                {/if}
              </span>
            </div>
          {/each}
        {:else}
          <div class="empty">{t('No container.')}</div>
        {/each}
      {:else if tab === 'images'}
        {#each docker.images as i (i.id)}
          <div class="row">
            <span class="dot" class:ok={i.containers > 0}></span>
            <span class="main">
              <span class="mono name" title={i.name}>{i.name}</span>
              <span class="sub muted">{i.containers ? tn(i.containers, '{n} container', '{n} containers') : t('unused')}{i.created_at ? ` · ${ago(i.created_at)}` : ''}</span>
            </span>
            <span class="mono num" title={i.unique_bytes !== null ? t('Its own part {own}, layers shared included {size}', { own: size(i.unique_bytes), size: size(i.size_bytes) }) : t('Layers shared included {size}', { size: size(i.size_bytes) })}>{size(i.unique_bytes ?? i.size_bytes)}</span>
            <span class="acts">
              {#if hasCore() && i.containers === 0}
                <button class="icon" class:armed={arming.is(`rmi:${i.id}`)} disabled={busy !== null} onclick={() => removeImage(i.id)} title={arming.is(`rmi:${i.id}`) ? t('Click again to remove it') : t('Remove (frees {size})', { size: size(i.unique_bytes ?? i.size_bytes) })} aria-label={tc('blocks', 'Remove {name}', { name: i.name })}><Icon name="trash" /></button>
              {/if}
            </span>
          </div>
        {:else}
          <div class="empty">{docker.disk_at ? t('No image.') : t('Measuring the disk…')}</div>
        {/each}
      {:else}
        {#each docker.volumes as v (v.name)}
          <div class="row">
            <span class="dot" class:ok={v.links > 0}></span>
            <span class="main">
              <span class="mono name" title={v.name}>{v.name}</span>
              <span class="sub muted">{v.links ? tn(v.links, 'used by {n} container', 'used by {n} containers') : t('unused: a volume prune removes it')}</span>
            </span>
            <span class="mono num">{size(v.size_bytes)}</span>
            <span class="acts"></span>
          </div>
        {:else}
          <div class="empty">{docker.disk_at ? t('No volume.') : t('Measuring the disk…')}</div>
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .head { display: flex; gap: 10px; align-items: center; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 0 8px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); font-weight: 400; cursor: pointer; }
  .seg button.on { background: color-mix(in srgb, var(--src) 18%, transparent); color: var(--text); }
  .disk { padding: 6px 12px 4px; border-bottom: 1px solid var(--border); display: flex; flex-direction: column; gap: 4px; }
  .kind { display: grid; grid-template-columns: 120px minmax(40px, 1fr) 64px auto; gap: 8px; align-items: center; font-size: var(--fs-meta); }
  .bar { height: 6px; border-radius: var(--r3); background: color-mix(in srgb, var(--warn) 45%, var(--surface-muted)); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--ok); }
  .measured { font-size: var(--fs-tag); }
  .num { text-align: right; font-size: var(--fs-tag); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .act { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  .act:hover:not(:disabled) { border-color: var(--warn); }
  .act.armed, .icon.armed { border-color: var(--ko); color: var(--ko); }
  .act:disabled, .icon:disabled { opacity: 0.5; cursor: default; }
  .err { margin: 0; padding: 4px 12px; color: var(--ko); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ok-msg { margin: 0; padding: 4px 12px; color: var(--ok); font-size: var(--fs-meta); }
  .section { padding: 4px 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); border-top: 1px solid var(--border); }
  .section:first-child { border-top: none; }
  .row { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto auto 64px; gap: 8px; align-items: center; padding: 5px 12px; border-top: 1px solid var(--border); }
  .main { display: flex; flex-direction: column; min-width: 0; }
  .name { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-mono); }
  .sub { font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); opacity: 0.5; }
  .dot.ok { background: var(--ok); opacity: 1; }
  .dot.warn { background: var(--warn); opacity: 1; }
  .dot.ko { background: var(--ko); opacity: 1; }
  .acts { display: flex; gap: 2px; justify-content: flex-end; }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; padding: 0; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { color: var(--text); border-color: var(--border); }
</style>
