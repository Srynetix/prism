<script lang="ts">
  import { showBlock } from '../../layout.svelte'
  import { stateOf, type StatusProps } from '../block'
  import { spark, tones, type SystemState } from './types'
  import { t } from '../../i18n.svelte'

  /** The status bar's item: CPU with its last minutes, RAM, latency; a click shows the block. */
  let { snap }: StatusProps = $props()

  const sys = $derived(stateOf<SystemState>({ id: 'system' }, snap))
  const tone = $derived(sys?.updated_at ? tones(sys) : null)
  const ram = $derived(sys?.memory.total ? Math.round((100 * sys.memory.used) / sys.memory.total) : 0)
</script>

{#if sys && tone}
  <button class="sbi sys" onclick={() => showBlock('system')} title={t('CPU {cpu} % · memory {ram} %{pressure} · latency {latency}, {loss} % lost · open System', { cpu: Math.round(sys.cpu.usage), ram, pressure: sys.memory.pressure !== 'unknown' ? ` (${t('pressure {level}', { level: t(sys.memory.pressure) })})` : '', latency: sys.network.latency_ms === null ? '—' : `${Math.round(sys.network.latency_ms)} ms`, loss: Math.round(sys.network.loss) })}>
    <span class={tone.cpu}>CPU <svg class="mini" viewBox="0 0 34 12" aria-hidden="true"><polyline points={spark(sys.history.cpu.slice(-18), 34, 12, 100)} /></svg> {Math.round(sys.cpu.usage)}%</span>
    <span class={tone.memory}>RAM {ram}%</span>
    {#if sys.network.hosts.length}<span class={tone.network}>↯ {sys.network.latency_ms === null ? '—' : `${Math.round(sys.network.latency_ms)} ms`}</span>{/if}
  </button>
{/if}

<style>
  .sys { gap: 10px; }
  .sys span { display: inline-flex; align-items: center; gap: 4px; }
  .sys .warn { color: var(--warn); }
  .sys .ko { color: var(--ko); }
  .mini { width: 34px; height: 12px; }
  .mini polyline { fill: none; stroke: currentColor; stroke-width: 1.2; opacity: 0.8; }
</style>
