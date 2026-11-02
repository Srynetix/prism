<script lang="ts">
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import type { BlockProps } from '../block'
  import { bytes, diskTone, rate, spark, tones, type SystemState, type Tone } from './types'
  import { t } from '../../i18n.svelte'

  /**
   * System: four gauges (CPU, memory, disk, network), CPU, memory and network with their last 10
   * minutes, who consumes them, grouped by what Prism knows, the disks, and the network's hosts.
   * Short: a line each. Narrow (a phone, a thin column): the gauges two by two.
   */
  let { state: sys, error }: BlockProps<SystemState> = $props()

  const tone = $derived(sys ? tones(sys) : null)
  const memPct = $derived(sys?.memory.total ? (100 * sys.memory.used) / sys.memory.total : 0)
  const COLOR: Record<Tone, string> = { ok: 'var(--ok)', warn: 'var(--warn)', ko: 'var(--ko)' }
  const PRESSURE = { unknown: '', normal: t('pressure normal'), warn: t('memory under pressure'), critical: t('memory pressure critical') }
  const GROUP_COLOR: Record<string, string> = { agents: 'var(--src-claude)', docker: 'var(--violet)', shells: 'var(--info)', prism: 'var(--accent)', app: 'var(--text-muted)' }
  /** Latency bars: their height against the slowest of the host's last handshakes. */
  const bar = (v: number | null, max: number) => (v === null ? 100 : Math.max(12, (100 * v) / max))
  const ms = (v: number | null) => (v === null ? '—' : `${Math.round(v)} ms`)
  const latMax = $derived(Math.max(60, ...(sys?.history.latency.filter((v): v is number => v !== null) ?? [])))
  const short = $derived(isShort('system'))
  const disk = $derived(sys?.disks?.[0])
  const usedPct = (d: { total: number; available: number }) => (d.total ? (100 * (d.total - d.available)) / d.total : 0)
</script>

<section class="panel" style="--src: var(--info)" class:collapsed={isCollapsed('system')}>
  <h2>
    <span class="title"><Icon name="cpu" />{t('System')}</span>
    {#if sys?.machine.chip}<small>{sys.machine.chip} · {t('{n} cores', { n: sys.machine.cores })} · {bytes(sys.machine.memory)}</small>{/if}
    {#if sys && tone && (sys.memory.pressure === 'warn' || sys.memory.pressure === 'critical')}<span class="pill {tone.memory}">{PRESSURE[sys.memory.pressure]}</span>{/if}
    <BlockControls id="system" />
  </h2>
  {#if !sys}
    <div class="empty">{t('The system is not read. Turn on Settings › Infra › System, then restart Prism.')}</div>
  {:else if !sys.updated_at}
    <div class="empty">{error ?? t('Reading the system…')}</div>
  {:else if short && tone}
    <div class="short">
      <div class="srow"><span class="k">CPU</span><span class="bar"><i style:width="{Math.min(100, sys.cpu.usage)}%" style:background={COLOR[tone.cpu]}></i></span><span class="n">{Math.round(sys.cpu.usage)} % · {t('load {n}', { n: sys.cpu.load[0].toFixed(1) })}</span></div>
      <div class="srow"><span class="k">{t('Memory')}</span><span class="bar"><i style:width="{memPct}%" style:background={COLOR[tone.memory]}></i></span><span class="n">{bytes(sys.memory.used)} / {bytes(sys.memory.total)}</span></div>
      {#if disk}<div class="srow"><span class="k">{t('Disk')}</span><span class="bar"><i style:width="{usedPct(disk)}%" style:background={COLOR[tone.disk]}></i></span><span class="n">{t('{size} free', { size: bytes(disk.available) })}</span></div>{/if}
      <div class="srow"><span class="k">{t('Network')}</span><span class="bar"><i style:width="{sys.network.latency_ms === null ? 0 : Math.max(8, 100 - sys.network.latency_ms / 3)}%" style:background={COLOR[tone.network]}></i></span><span class="n">{ms(sys.network.latency_ms)} · {Math.round(sys.network.loss)} %</span></div>
    </div>
  {:else if tone}
    <div class="scroll">
      <div class="gauges">
        <div class="g">
          <div class="l"><span>CPU</span><span>10 min</span></div>
          <div class="v">{Math.round(sys.cpu.usage)}<small> %</small></div>
          <div class="bar"><i style:width="{Math.min(100, sys.cpu.usage)}%" style:background={COLOR[tone.cpu]}></i></div>
          <div class="d" title={t('Load average over 1, 5 and 15 minutes')}>{t('load {n}', { n: sys.cpu.load.map((l) => l.toFixed(1)).join(' · ') })}</div>
          <svg class="spark" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true"><polyline points={spark(sys.history.cpu, 100, 30, 100)} style:stroke={COLOR[tone.cpu]} /></svg>
        </div>
        <div class="g">
          <div class="l"><span>{t('Memory')}</span><span>{sys.memory.pressure === 'unknown' ? t('used') : t('pressure')}</span></div>
          <div class="v">{bytes(sys.memory.used).replace(/ \S+$/, '')}<small> / {bytes(sys.memory.total)}</small></div>
          <div class="bar">
            {#if sys.memory.app !== null && sys.memory.wired !== null && sys.memory.compressed !== null}
              <i style:width="{(100 * sys.memory.app) / sys.memory.total}%" style:background="var(--info)" title={t('Apps {size}', { size: bytes(sys.memory.app) })}></i>
              <i style:width="{(100 * sys.memory.wired) / sys.memory.total}%" style:background="var(--violet)" title={t('Wired {size}', { size: bytes(sys.memory.wired) })}></i>
              <i style:width="{(100 * sys.memory.compressed) / sys.memory.total}%" style:background={COLOR[tone.memory]} title={t('Compressed {size}', { size: bytes(sys.memory.compressed) })}></i>
            {:else}<i style:width="{memPct}%" style:background={COLOR[tone.memory]}></i>{/if}
          </div>
          <div class="d">{#if sys.memory.app !== null}{t('apps {apps} · wired {wired} · compr. {compressed}', { apps: bytes(sys.memory.app), wired: bytes(sys.memory.wired ?? 0), compressed: bytes(sys.memory.compressed ?? 0) })}{:else}swap {bytes(sys.memory.swap)}{/if}</div>
          <svg class="spark" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true"><polyline points={spark(sys.history.memory, 100, 30, 100)} style:stroke={COLOR[tone.memory]} /></svg>
        </div>
        {#if disk}
          <div class="g">
            <div class="l"><span>{t('Disk')}</span><span>{disk.name || disk.mount}</span></div>
            <div class="v">{bytes(disk.available).replace(/ \S+$/, '')}<small>&nbsp;{t('{unit} free', { unit: bytes(disk.available).split(' ').at(-1) ?? '' })}</small></div>
            <div class="bar"><i style:width="{usedPct(disk)}%" style:background={COLOR[tone.disk]}></i></div>
            <div class="d">{t('{used} used of {total}', { used: bytes(disk.total - disk.available), total: bytes(disk.total) })}</div>
          </div>
        {/if}
        <div class="g">
          <div class="l"><span>{t('Network')}</span><span>{sys.network.wifi ? 'Wi-Fi' : t('latency')}</span></div>
          <div class="v">{sys.network.latency_ms === null ? '—' : Math.round(sys.network.latency_ms)}<small> ms</small></div>
          <div class="bar"><i style:width="{100 - Math.min(100, sys.network.loss * 10)}%" style:background={COLOR[tone.network]}></i></div>
          <div class="d">{t('{n} % lost', { n: Math.round(sys.network.loss) })}{#if sys.network.jitter_ms !== null}{' '}· {t('jitter {n} ms', { n: Math.round(sys.network.jitter_ms) })}{/if} · ↓ {rate(sys.network.rx)} ↑ {rate(sys.network.tx)}</div>
          <svg class="spark" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true"><polyline points={spark(sys.history.latency, 100, 30, latMax)} style:stroke={COLOR[tone.network]} /></svg>
        </div>
      </div>
      <div class="sec">
        <h3>{t('Who consumes')} <span>{t('CPU · memory')}</span></h3>
        {#each sys.groups as g (g.kind + g.name)}
          <div class="proc" class:app={g.kind === 'app'}>
            <span class="dot" style:background={GROUP_COLOR[g.kind]}></span>
            <span class="nm" title="{g.name} · {g.detail}">{#if g.kind === 'app'}{g.name}{:else}<b>{g.name}</b>{/if} <span class="muted">· {g.detail}</span></span>
            <span class="n" class:hot={g.cpu >= 50} title={t('% of one core, summed over its processes')}>{Math.round(g.cpu)} %</span>
            <span class="n" class:hot={g.memory >= sys.memory.total / 5}>{bytes(g.memory)}</span>
          </div>
        {/each}
      </div>
      {#if (sys.disks?.length ?? 0) > 1}
        <div class="sec">
          <h3>{t('Disks')} <span>{t('free space')}</span></h3>
          {#each sys.disks as d (d.mount)}
            <div class="disk" title="{d.name} · {d.mount}">
              <span class="nm"><b>{d.name || d.mount}</b> <span class="muted">· {d.mount}</span></span>
              <span class="bar"><i style:width="{usedPct(d)}%" style:background={COLOR[diskTone(d, sys.disk_limits)]}></i></span>
              <span class="n">{bytes(d.available)} / {bytes(d.total)}</span>
            </div>
          {/each}
        </div>
      {/if}
      <div class="sec">
        <h3>{t('Network')} <span>{t('a handshake every 15 s')}</span></h3>
        <div class="net">
          {#if sys.network.wifi}<div><span class="muted">Wi-Fi</span><b>{sys.network.wifi.rssi} dBm · {Math.round(sys.network.wifi.rate)} Mb/s</b></div>{/if}
          <div><span class="muted">{t('Gateway')}</span><b>{ms(sys.network.gateway_ms)}</b></div>
          <div><span class="muted">{t('Lost (5 min)')}</span><b>{Math.round(sys.network.loss)} %</b></div>
          {#if sys.network.jitter_ms !== null}<div><span class="muted">{t('Jitter')}</span><b>{Math.round(sys.network.jitter_ms)} ms</b></div>{/if}
        </div>
        {#each sys.network.hosts as h (h.name)}
          {@const max = Math.max(1, ...h.recent.filter((v): v is number => v !== null))}
          <div class="host" title={t('{name}: {n} % lost over 5 min', { name: h.name, n: Math.round(h.loss) })}>
            <span class="hn">{h.name}</span>
            <span class="n" class:ko={h.ms === null}>{ms(h.ms)}</span>
            <span class="lat">{#each h.recent as v, i (i)}<i class:k={v === null} class:w={v !== null && v > max * 0.8 && v > 1.5 * (h.ms ?? v)} style:height="{bar(v, max)}%"></i>{/each}</span>
          </div>
        {:else}
          <p class="muted">{t('No host to measure yet: the forge and tracker are known once read.')}</p>
        {/each}
      </div>
    </div>
  {/if}
</section>

<style>
  /* The machine, beside the title: one line, cut when the block is narrow. */
  h2 small { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 14px 16px; color: var(--text-muted); font-size: var(--fs-body); }
  .muted { color: var(--text-muted); }
  p.muted { margin: 4px 0 0; font-size: var(--fs-meta); }
  .pill { padding: 0 7px; border-radius: 9px; font-size: var(--fs-tag); font-weight: 500; white-space: nowrap; }
  .pill.warn { background: var(--warn-bg); color: var(--warn); }
  .pill.ko { background: var(--ko-bg); color: var(--ko); }
  .scroll { container-type: inline-size; }
  .gauges { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); border-bottom: 1px solid var(--border); }
  .disk { display: grid; grid-template-columns: minmax(0, 1fr) 70px 110px; gap: 8px; align-items: center; padding: 2px 0; font-size: var(--fs-meta); }
  .disk .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .disk b { font-weight: 550; }
  .disk .bar { margin: 0; }
  /* Narrow (a phone, a thin column): the gauges two by two, their borders a grid. */
  @container (max-width: 520px) {
    .gauges { grid-template-columns: 1fr 1fr; }
    .g { border-bottom: 1px solid var(--border); }
    .g:nth-child(2n) { border-right: none; }
    .g:nth-last-child(-n + 2):nth-child(odd), .g:last-child { border-bottom: none; }
    .disk { grid-template-columns: minmax(0, 1fr) 90px; }
    .disk .bar { grid-column: 1 / -1; grid-row: 2; }
  }
  .g { min-width: 0; padding: 8px 12px; border-right: 1px solid var(--border); }
  .g:last-child { border-right: none; }
  .l { display: flex; justify-content: space-between; font-size: var(--fs-tag); color: var(--text-muted); }
  .v { font-size: 19px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .v small { font-size: var(--fs-meta); font-weight: 400; color: var(--text-muted); }
  .d { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .bar { display: flex; height: 5px; margin: 4px 0 3px; overflow: hidden; border-radius: 3px; background: var(--surface-muted); }
  .bar i { display: block; height: 100%; }
  .spark { display: block; width: 100%; height: 30px; margin-top: 4px; }
  .spark polyline { fill: none; stroke-width: 1.4; vector-effect: non-scaling-stroke; }
  .sec { padding: 8px 12px 10px; border-bottom: 1px solid var(--border); }
  .sec:last-child { border-bottom: none; }
  h3 { display: flex; gap: 6px; margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 span { margin-left: auto; font-weight: 400; text-transform: none; letter-spacing: 0; }
  .proc { display: grid; grid-template-columns: 10px minmax(0, 1fr) 46px 58px; gap: 6px; align-items: center; padding: 2px 0; font-size: var(--fs-meta); }
  .proc .nm { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .proc b { font-weight: 550; }
  .dot { width: 8px; height: 8px; border-radius: 50%; }
  .n { font: var(--fs-tag) var(--mono); text-align: right; color: var(--text-muted); white-space: nowrap; }
  .n.hot { color: var(--warn); }
  .n.ko { color: var(--ko); }
  .net { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 3px 14px; margin-bottom: 6px; font-size: var(--fs-meta); }
  .net div { display: flex; justify-content: space-between; gap: 8px; }
  .net b { font: 500 var(--fs-meta) var(--mono); }
  .host { display: grid; grid-template-columns: minmax(0, 1fr) 54px 70px; gap: 6px; align-items: center; padding: 1px 0; font-size: var(--fs-meta); }
  .hn { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lat { display: flex; align-items: flex-end; gap: 1px; height: 14px; }
  .lat i { flex: 1; border-radius: 1px; background: var(--ok); }
  .lat i.w { background: var(--warn); }
  .lat i.k { background: var(--ko); opacity: 0.6; }
  .short { display: flex; flex-direction: column; }
  .srow { display: flex; align-items: center; gap: 8px; padding: 5px 12px; border-top: 1px solid var(--border); font-size: var(--fs-body); white-space: nowrap; }
  .srow:first-child { border-top: none; }
  .srow .k { width: 54px; color: var(--text-muted); font-size: var(--fs-meta); }
  .srow .bar { flex: 1; margin: 0; }
  .srow .n { width: 110px; font-size: var(--fs-meta); color: var(--text); }
</style>
