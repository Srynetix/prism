<script lang="ts">
  import { t } from '../i18n.svelte'
  import type { Pipeline } from '../types'
  import { pipelineLabel, stageTone } from '../pipeline'
  import { elapsed, span } from '../time'

  /** Compact bar: one segment per stage, and a label that says what matters. */
  let { pipeline, now, status = null }: { pipeline: Pipeline | null; now: number; status?: string | null } = $props()
  const label = $derived(pipeline ? pipelineLabel(pipeline, now) : null)
  const stageTitle = (s: Pipeline['stages'][number]) => {
    const took = elapsed(s.started_at, s.finished_at, now)
    return `${s.name}: ${s.status} · ${t('{done}/{total} jobs', { done: s.finished, total: s.jobs })}${took !== null ? ` · ${span(took)}` : ''}`
  }
</script>

{#if pipeline && pipeline.stages.length}
  <span class="pipe">
    <span class="segs">
      {#each pipeline.stages as s}<i class={stageTone(s)} title={stageTitle(s)}></i>{/each}
    </span>
    <span class="lbl {label?.tone}" title={label?.text}>{label?.text}</span>
  </span>
{:else if status}
  <span class="pipe"><span class="lbl {status === 'success' ? 'ok' : status === 'failed' ? 'ko' : status === 'canceled' ? 'cx' : 'run'}">{status === 'success' ? t('passed') : status}</span></span>
{:else}
  <span class="pipe"><span class="lbl muted">{t('no pipeline')}</span></span>
{/if}

<style>
  .pipe { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .segs { display: flex; gap: 2px; flex-shrink: 0; }
  .segs i { display: block; width: 13px; height: 8px; border-radius: var(--r2); background: var(--border); }
  .segs i.ok { background: var(--ok); }
  .segs i.ko { background: var(--ko); }
  .segs i.run { background: var(--warn); animation: pulse 1.4s infinite; }
  /* Fun: a sheen sweeps across the running step, a layer moved (transform), which the GPU
     composites: a moving background repainted the window at 60 Hz for as long as a pipeline ran. */
  :global(html:not([data-animations='off']):not([data-reduce-motion])) .segs i.run { position: relative; overflow: hidden; animation: none; }
  :global(html:not([data-animations='off']):not([data-reduce-motion])) .segs i.run::after { content: ''; position: absolute; inset: 0; background: linear-gradient(90deg, transparent 20%, color-mix(in srgb, var(--warn) 35%, #fff) 50%, transparent 80%); animation: shimmer 1.3s linear infinite; will-change: transform; }
  @keyframes shimmer { from { transform: translateX(-100%); } to { transform: translateX(100%); } }
  .segs i.cx { background: repeating-linear-gradient(135deg, var(--text-muted) 0 2px, transparent 2px 4px); }
  .segs i.man { background: transparent; box-shadow: inset 0 0 0 1px var(--text-muted); }
  .lbl { font-size: var(--fs-meta); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--text-muted); }
  .lbl.ok { color: var(--ok); }
  .lbl.ko { color: var(--ko); }
  .lbl.run { color: var(--warn); }
</style>
