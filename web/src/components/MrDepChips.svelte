<script lang="ts">
  import { t, tn } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { mrKey, type MrRef } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { blocking, dependsOn, lineage, viaLabel, waitedFor, type MrDep } from '../mr-deps'
  import { depHover } from '../mr-deps-view.svelte'

  /**
   * An MR's dependencies on its row: what it waits for (`waits for !410`, the first open one,
   * then how many more), what waits for it (`blocks 2`). Pointed at, its lineage lights up in the
   * list; a click on the MR it waits for opens it.
   */
  let { mr }: { mr: MrRef } = $props()

  const snap = $derived(live.snapshot)
  const on = $derived(snap ? dependsOn(snap, mr) : [])
  const open = $derived(on.filter((d) => !d.done))
  const done = $derived(on.filter((d) => d.done))
  const blocks = $derived(snap ? blocking(snap, mr).filter((d) => !d.done) : [])

  const via = (d: MrDep) => viaLabel(d.via)

  function light() {
    if (snap) depHover.lit = lineage(snap.mr_deps ?? [], mrKey(mr))
  }
  const dim = () => (depHover.lit = new Set())
  function open1(e: MouseEvent, d: MrDep) {
    e.stopPropagation()
    openMr(waitedFor(d))
  }
</script>

{#if open.length || done.length || blocks.length}
  <span class="deps" role="group" aria-label={t('Dependencies')} onmouseenter={light} onmouseleave={dim}>
    {#if open.length}
      {@const d = open[0]}
      <button class="chip wait" class:inferred={d.via === 'ticket'} onclick={(e) => open1(e, d)} title={t('Waits for {mr} ({via}), still open · click: open it', { mr: mrKey(waitedFor(d)), via: via(d) })}>
        {t('waits for {mr}', { mr: mrKey(waitedFor(d)) })}{#if open.length > 1}<b>+{open.length - 1}</b>{/if}
      </button>
    {:else if done.length}
      <span class="chip done" title={t('{mr} it waited for is merged', { mr: mrKey(waitedFor(done[0])) })}>✓ {t('{mr} merged', { mr: mrKey(waitedFor(done[0])) })}</span>
    {/if}
    {#if blocks.length}
      <span class="chip blocks" title={t('Waiting for it: {list}', { list: blocks.map((d) => mrKey({ iid: d.iid, repo: d.repo })).join(', ') })}>{tn(blocks.length, 'blocks {n}', 'blocks {n}')}</span>
    {/if}
  </span>
{/if}

<style>
  /* Shrinks with the title on a narrow row, each chip cut short rather than the title gone. */
  .deps { display: inline-flex; gap: 4px; flex: 0 1 auto; min-width: 0; overflow: hidden; }
  .chip { display: inline-flex; align-items: center; gap: 3px; padding: 0 7px; border: none; border-radius: var(--r10); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; min-width: 0; overflow: hidden; text-overflow: ellipsis; --c: var(--text-muted); background: color-mix(in srgb, var(--c) 14%, transparent); color: color-mix(in srgb, var(--c) 75%, var(--text)); }
  .chip b { font-weight: 600; }
  .wait { --c: var(--warn); cursor: pointer; }
  .wait:hover { background: color-mix(in srgb, var(--c) 24%, transparent); }
  .wait.inferred { background: none; border: 1px dashed color-mix(in srgb, var(--c) 55%, transparent); line-height: 16px; }
  .blocks { --c: var(--info); }
  .done { --c: var(--ok); }
</style>
