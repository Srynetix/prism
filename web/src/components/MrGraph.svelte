<script lang="ts">
  import { t } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { mrKey, sameMr, type MrRef } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { isApproved } from '../rules'
  import { forgeAvatar, initialsOf, personColor } from '../people'
  import { cleanTitle } from '../mr-actions'
  import { chains, layout, lineage, waitedFor, waiting, waitsOpen, viaLabel, type DepFilter } from '../mr-deps'
  import { depHover } from '../mr-deps-view.svelte'
  import type { MergeRequest } from '../types'

  /**
   * The dependencies between MRs as a graph: from left to right in the order they can merge (a
   * column per depth), a band per chain. An edge per dependency, its stroke by source (plain:
   * stacked branch, dashes: declared, dots: via tickets; grey once satisfied); a node's border
   * says its state (ready, waiting for another, CI or conflicts, merged). Pointed at, a node lights
   * its lineage; a click opens its MR. `compact`: the MR panel's, numbers only, its own chain.
   */
  let { mrs, filter, compact = false, center = null }: { mrs: MergeRequest[]; filter: DepFilter; compact?: boolean; center?: MrRef | null } = $props()

  const snap = $derived(live.snapshot)
  const found = $derived.by(() => {
    if (!snap) return []
    const all = chains(snap, mrs, filter).chains
    return center ? all.filter((c) => c.mrs.some((m) => sameMr(m, center!))) : all
  })
  const W = $derived(compact ? 66 : 214)
  const H = $derived(compact ? 24 : 70)
  const GX = $derived(compact ? 34 : 56)
  const GY = $derived(compact ? 8 : 14)
  const PAD = 8
  const placed = $derived(layout(found))
  const at = $derived(new Map(placed.placed.map((p) => [mrKey(p.mr), p])))
  const deps = $derived(found.flatMap((c) => c.deps))
  const x = (col: number) => PAD + col * (W + GX)
  const y = (row: number) => PAD + row * (H + GY)
  const width = $derived(PAD * 2 + placed.cols * W + Math.max(0, placed.cols - 1) * GX)
  const height = $derived(PAD * 2 + placed.rows * H + Math.max(0, placed.rows - 1) * GY)

  const edges = $derived(
    deps.flatMap((d) => {
      const a = at.get(mrKey(waitedFor(d)))
      const b = at.get(mrKey(waiting(d)))
      if (!a || !b) return []
      const [x1, y1, x2, y2] = [x(a.col) + W, y(a.row) + H / 2, x(b.col), y(b.row) + H / 2]
      const mx = (x1 + x2) / 2
      return [{ d, key: `${mrKey(waiting(d))}>${mrKey(waitedFor(d))}`, path: `M${x1} ${y1} C${mx} ${y1} ${mx} ${y2} ${x2 - 5} ${y2}`, head: `M${x2 - 7} ${y2 - 4} L${x2} ${y2} L${x2 - 7} ${y2 + 4}`, ends: [mrKey(waiting(d)), mrKey(waitedFor(d))] }]
    }),
  )

  /** Its state, as its border says. */
  function tone(m: MergeRequest): string {
    if (m.state === 'merged') return 'merged'
    if (m.conflicts || m.pipeline === 'failed') return 'failed'
    if (snap && waitsOpen(snap, m).length) return 'blocked'
    if (isApproved(m) && m.approvals_left <= 0 && m.pipeline === 'success' && !m.draft) return 'ready'
    return ''
  }
  function waitsFor(m: MergeRequest) {
    const d = snap ? waitsOpen(snap, m)[0] : undefined
    return d ? mrKey(waitedFor(d)) : null
  }

  let lit = $state<Set<string> | null>(null)
  function point(m: MergeRequest | null) {
    lit = m ? lineage(deps, mrKey(m)) : null
    depHover.lit = lit ?? new Set()
  }
</script>

{#if found.length}
  <div class="wrap" class:compact>
    <div class="graph" class:focus={!!lit} style:width="{width}px" style:height="{height}px">
      <svg width={width} height={height} aria-hidden="true">
        {#each edges as e (e.key)}
          {@const on = !lit || (lit.has(e.ends[0]) && lit.has(e.ends[1]))}
          <g class="e {e.d.via}" class:done={e.d.done} class:off={!on}>
            <path d={e.path} /><path class="head" d={e.head} />
            <title>{t('{mr} waits for {on} ({via})', { mr: e.ends[0], on: e.ends[1], via: viaLabel(e.d.via) })}{e.d.done ? ` · ${t('merged')}` : ''}</title>
          </g>
        {/each}
      </svg>
      {#each placed.placed as p (mrKey(p.mr))}
        {@const m = p.mr}
        {@const k = mrKey(m)}
        {@const pic = forgeAvatar(m.author_avatar)}
        {@const w = waitsFor(m)}
        <button
          class="node {tone(m)}"
          class:off={!!lit && !lit.has(k)}
          class:center={!!center && sameMr(m, center)}
          style:left="{x(p.col)}px"
          style:top="{y(p.row)}px"
          style:width="{W}px"
          style:height="{H}px"
          onmouseenter={() => point(m)}
          onmouseleave={() => point(null)}
          onfocus={() => point(m)}
          onblur={() => point(null)}
          onclick={() => openMr(m)}
          title="{k} · {cleanTitle(m)}{w ? ` · ${t('waits for {mr}', { mr: w })}` : ''}"
        >
          {#if compact}
            <span class="iid mono">{k}</span>
          {:else}
            <span class="top">
              <span class="av" class:pic={!!pic} style:--c={personColor(m.author)}>{#if pic}<img src={pic} alt="" />{:else}{initialsOf(m.author_name, m.author)}{/if}</span>
              <span class="iid mono">{k}</span>
              {#if m.draft}<span class="tag">{t('Draft')}</span>{/if}
              <span class="ci {m.pipeline ?? 'none'}"></span>
            </span>
            <span class="name">{cleanTitle(m)}</span>
            <span class="meta">
              {#if m.state === 'merged'}<span class="chip">{t('merged')}</span>
              {:else}
                <span class="chip" class:ok={m.approvals_left <= 0 && isApproved(m)}>✓ {m.approved_by.length}/{m.approved_by.length + Math.max(0, m.approvals_left)}</span>
                {#if w}<span class="chip wait">{t('waits for {mr}', { mr: w })}</span>{:else if tone(m) === 'ready'}<span class="chip ok">{t('ready')}</span>{/if}
              {/if}
            </span>
          {/if}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .wrap { overflow: auto; padding: 4px 0; }
  .graph { position: relative; }
  svg { position: absolute; inset: 0; pointer-events: auto; overflow: visible; }
  .e path { fill: none; stroke-width: 1.6; stroke: var(--text); transition: opacity 0.15s; }
  .e.declared path { stroke: var(--info); }
  .e.declared path:not(.head) { stroke-dasharray: 6 4; }
  .e.ticket path { stroke: var(--text-muted); }
  .e.ticket path:not(.head) { stroke-dasharray: 1.5 4; stroke-linecap: round; }
  .e.done path { stroke: var(--text-muted); opacity: 0.5; }
  .e.off path { opacity: 0.15; }
  .node { position: absolute; display: flex; flex-direction: column; gap: 3px; padding: 6px 8px; border: 1.5px solid var(--border); border-radius: var(--r8); background: var(--surface); color: var(--text); font: inherit; text-align: left; cursor: pointer; transition: opacity 0.15s, border-color 0.15s; overflow: hidden; }
  .node:hover, .node:focus-visible { border-color: var(--accent); outline: none; }
  .node.ready { border-color: var(--ok); }
  .node.blocked { border-color: var(--warn); }
  .node.failed { border-color: var(--ko); }
  .node.merged { border-style: dashed; opacity: 0.6; }
  .node.center { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent); border-color: var(--accent); }
  .node.off { opacity: 0.25; }
  .compact .node { justify-content: center; align-items: center; padding: 0 4px; }
  .top { display: flex; align-items: center; gap: 6px; font-size: var(--fs-meta); }
  .iid { font-weight: 600; font-size: var(--fs-meta); white-space: nowrap; }
  .mono { font-family: var(--mono); }
  .tag { font-size: var(--fs-micro); color: var(--text-muted); border: 1px solid var(--border); border-radius: var(--r10); padding: 0 5px; }
  .ci { margin-left: auto; width: 8px; height: 8px; border-radius: 50%; background: var(--border); flex-shrink: 0; }
  .ci.success { background: var(--ok); }
  .ci.failed { background: var(--ko); }
  .ci.running, .ci.pending { background: var(--info); }
  .name { font-size: var(--fs-body); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { display: flex; gap: 4px; overflow: hidden; }
  .chip { padding: 0 6px; border-radius: var(--r10); font-size: var(--fs-tag); line-height: 16px; white-space: nowrap; background: var(--surface-muted); color: var(--text-muted); }
  .chip.ok { color: var(--ok); }
  .chip.wait { color: color-mix(in srgb, var(--warn) 75%, var(--text)); background: color-mix(in srgb, var(--warn) 14%, transparent); }
  .av { display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 16px; border-radius: var(--r-avatar); background: var(--c); color: #fff; font-size: var(--fs-micro); font-weight: 700; flex-shrink: 0; overflow: hidden; }
  .av img { width: 100%; height: 100%; object-fit: cover; }
</style>
