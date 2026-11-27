<script lang="ts">
  import { t } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { mrKey } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { cleanTitle } from '../mr-actions'
  import { initialsOf, personColor } from '../people'
  import { mergePriority } from '../merge-queue'
  import { arcs, waitedFor, waitsOpen } from '../mr-deps'
  import type { MergeRequest } from '../types'

  /**
   * A merge queue as a train: its MRs from left to right in the order its job takes them, and above
   * them an arc from each MR to the MR it waits for (stacked or declared, still open). Forwards is
   * fine; backwards (it would merge first), in red: the alert's case, with its Hold fix.
   */
  let { mrs, prefix }: { mrs: MergeRequest[]; prefix: string | null | undefined } = $props()

  const CW = 176
  const GAP = 14
  const PAD = 10
  const snap = $derived(live.snapshot)
  const list = $derived(snap ? arcs(snap, mrs) : [])
  const backs = $derived(new Set(list.filter((a) => a.back).map((a) => a.from)))
  const top = $derived(list.length ? 30 + list.length * 16 : 12)
  const width = $derived(PAD * 2 + mrs.length * CW + Math.max(0, mrs.length - 1) * GAP)
  const cx = (i: number) => PAD + i * (CW + GAP) + CW / 2
  const shapes = $derived(
    list.map((a, i) => {
      const [x1, x2] = [cx(a.from), cx(a.to)]
      const h = 14 + i * 16
      const y = top - 2
      return { a, path: `M${x1} ${y} C${x1} ${y - h - 10} ${x2} ${y - h - 10} ${x2} ${y}`, lx: (x1 + x2) / 2, ly: y - 0.75 * (h + 10) - 4 }
    }),
  )
  /** What it waits for out of the queue, still open. */
  function outside(m: MergeRequest) {
    if (!snap) return null
    const keys = new Set(mrs.map(mrKey))
    const d = waitsOpen(snap, m).find((x) => x.via !== 'ticket' && !keys.has(mrKey(waitedFor(x))))
    return d ? mrKey(waitedFor(d)) : null
  }
</script>

<div class="track">
  <div class="inner" style:width="{width}px" style:padding-top="{top}px">
    <svg width={width} height={top} aria-hidden="true">
      {#each shapes as s, i (i)}
        <path class:back={s.a.back} d={s.path} />
        <text class:back={s.a.back} x={s.lx} y={s.ly}>{s.a.back ? t('{mr} waits for {on}, but goes first', { mr: mrKey(mrs[s.a.from]), on: mrKey(mrs[s.a.to]) }) : t('{mr} waits for {on}', { mr: mrKey(mrs[s.a.from]), on: mrKey(mrs[s.a.to]) })}</text>
      {/each}
    </svg>
    <div class="cars">
      {#each mrs as m, i (mrKey(m))}
        {@const prio = mergePriority(m, prefix)}
        {@const out = outside(m)}
        <button class="car" class:next={i === 0} class:bad={backs.has(i)} style:width="{CW}px" onclick={() => openMr(m)} title="{mrKey(m)} · {cleanTitle(m)}">
          <span class="pos">{i === 0 ? '▶ 1' : i + 1}{#if prio !== null} · P{prio}{/if}</span>
          <span class="top"><b class="mono">{mrKey(m)}</b><span class="av" style:--c={personColor(m.author)}>{initialsOf(m.author_name, m.author)}</span><span class="ci {m.pipeline ?? 'none'}"></span></span>
          <span class="name">{cleanTitle(m)}</span>
          {#if backs.has(i)}
            {@const a = list.find((x) => x.from === i && x.back)}
            <span class="chip bad">{t('before {mr}', { mr: a ? mrKey(mrs[a.to]) : '' })}</span>
          {:else if out}
            <span class="chip wait">{t('waits for {mr}, out of the queue', { mr: out })}</span>
          {/if}
        </button>
      {/each}
    </div>
  </div>
</div>

<style>
  .track { overflow-x: auto; border-bottom: 1px solid var(--border); }
  .inner { position: relative; padding-bottom: 10px; }
  svg { position: absolute; left: 0; top: 0; overflow: visible; }
  path { fill: none; stroke: var(--text-muted); stroke-width: 1.6; }
  path.back { stroke: var(--ko); }
  text { font-size: var(--fs-tag); fill: var(--text-muted); text-anchor: middle; }
  text.back { fill: var(--ko); }
  .cars { display: flex; gap: 14px; padding: 0 10px; }
  .car { position: relative; display: flex; flex-direction: column; gap: 3px; padding: 9px 9px 7px; border: 1.5px solid var(--border); border-radius: var(--r8); background: var(--surface); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .car:hover { border-color: var(--accent); }
  .car.next { border-color: var(--accent); }
  .car.bad { border-color: var(--ko); background: color-mix(in srgb, var(--ko) 8%, var(--surface)); }
  .pos { position: absolute; top: -9px; left: 8px; padding: 0 4px; background: var(--surface); font: 600 var(--fs-tag) var(--mono); color: var(--text-muted); }
  .car.next .pos { color: var(--accent); }
  .top { display: flex; align-items: center; gap: 6px; font-size: var(--fs-meta); }
  .mono { font-family: var(--mono); }
  .av { display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 16px; border-radius: var(--r-avatar); background: var(--c); color: #fff; font-size: var(--fs-micro); font-weight: 700; }
  .ci { margin-left: auto; width: 8px; height: 8px; border-radius: 50%; background: var(--border); }
  .ci.success { background: var(--ok); }
  .ci.failed { background: var(--ko); }
  .ci.running, .ci.pending { background: var(--info); }
  .name { font-size: var(--fs-body); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip { align-self: flex-start; padding: 0 6px; border-radius: var(--r10); font-size: var(--fs-tag); line-height: 16px; white-space: nowrap; }
  .chip.bad { color: var(--ko); background: color-mix(in srgb, var(--ko) 14%, transparent); }
  .chip.wait { color: color-mix(in srgb, var(--warn) 75%, var(--text)); background: color-mix(in srgb, var(--warn) 14%, transparent); }
</style>
