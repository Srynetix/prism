<script lang="ts">
  import { size, squarify, type DiskNode } from './types'

  /**
   * The treemap of `root`: each child a rectangle of its size, three levels deep (a folder big
   * enough shows its children inside, under its name). A click selects, a double click enters a
   * folder, a right click opens its actions.
   */
  let { root, color, selected, onpick, onenter, onhover, oncontext }: {
    root: DiskNode
    /** A node's color; `top`: the index of its top-level folder under `root`, `depth` from 0. */
    color: (n: DiskNode, top: number, depth: number) => string
    selected: number | null
    onpick: (n: DiskNode) => void
    onenter: (n: DiskNode) => void
    onhover: (n: DiskNode | null) => void
    oncontext: (e: MouseEvent, n: DiskNode) => void
  } = $props()

  let w = $state(0)
  let h = $state(0)

  interface Cell {
    n: DiskNode
    x: number
    y: number
    w: number
    h: number
    /** A folder showing its children inside. */
    open: boolean
    top: number
    depth: number
  }

  /** A folder shows its children when it is this big, three levels down at most. */
  const OPEN_W = 46
  const OPEN_H = 34
  const HEAD = 15

  const cells = $derived.by(() => {
    const out: Cell[] = []
    const lay = (nodes: DiskNode[], x: number, y: number, bw: number, bh: number, depth: number, top: number) => {
      squarify(nodes, (n) => n.bytes, x, y, bw, bh).forEach((r, i) => {
        const kids = r.item.children?.filter((c) => c.bytes > 0) ?? []
        const open = r.item.dir && kids.length > 0 && depth < 2 && r.w > OPEN_W && r.h > OPEN_H
        const t = depth === 0 ? i : top
        out.push({ n: r.item, x: r.x, y: r.y, w: r.w, h: r.h, open, top: t, depth })
        if (open) lay(kids, r.x + 2, r.y + HEAD, r.w - 4, r.h - HEAD - 2, depth + 1, t)
      })
    }
    if (w > 0 && h > 0) lay(root.children?.filter((c) => c.bytes > 0) ?? [], 0, 0, w, h, 0, 0)
    return out
  })
</script>

<div class="tm" bind:clientWidth={w} bind:clientHeight={h} role="presentation" onmouseleave={() => onhover(null)}>
  {#each cells as c (c.n.id)}
    <button
      type="button"
      class="cell"
      class:open={c.open}
      class:sel={c.n.id === selected}
      style:left="{c.x}px"
      style:top="{c.y}px"
      style:width="{c.w}px"
      style:height="{c.h}px"
      style:background-color={c.open ? null : color(c.n, c.top, c.depth)}
      title="{c.n.name} · {size(c.n.bytes)}"
      onmouseenter={() => onhover(c.n)}
      onclick={() => onpick(c.n)}
      ondblclick={() => c.n.dir && onenter(c.n)}
      oncontextmenu={(e) => {
        e.preventDefault()
        oncontext(e, c.n)
      }}
    >
      {#if c.w > 34 && c.h > 13}<span class="lbl">{c.n.name}{#if c.w > 90}<small>{size(c.n.bytes)}</small>{/if}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .tm { position: relative; width: 100%; height: 100%; min-height: 200px; }
  .cell {
    position: absolute; display: block; overflow: hidden; padding: 1px 4px; margin: 0;
    border: 1px solid color-mix(in srgb, var(--bg) 70%, transparent); border-radius: 0;
    color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 13px; text-align: left; white-space: nowrap; text-overflow: ellipsis; cursor: pointer;
    /* A cushion, lit from the top left, as WizTree draws them. */
    background-image: radial-gradient(ellipse at 35% 28%, rgb(255 255 255 / 0.22), rgb(255 255 255 / 0) 60%, rgb(0 0 0 / 0.14));
  }
  .cell.open { background-image: none; background-color: color-mix(in srgb, var(--surface-muted) 70%, transparent); border-color: var(--border); font-weight: 600; }
  .cell:hover { filter: brightness(1.08); }
  .cell.sel { outline: 2px solid var(--accent); outline-offset: -2px; }
  /* Above its neighbours; a folder stays under its children, its outline in the gap around them. */
  .cell.sel:not(.open) { z-index: 2; }
  /* A button centers its content: the name goes to the top left, as a folder's header. */
  .lbl { position: absolute; top: 1px; left: 4px; right: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cell small { margin-left: 4px; font-weight: 400; opacity: 0.75; }
</style>
