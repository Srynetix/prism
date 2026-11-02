<script lang="ts">
  import { t } from '../i18n.svelte'

  /** Resize handle between two neighboring elements (previous and next sibling). */
  let { dir, onresize, onreset }: {
    dir: 'x' | 'y'
    onresize: (delta: number, pixels: [number, number]) => void
    onreset: () => void
  } = $props()

  let el: HTMLDivElement
  let dragging = $state(false)

  function down(e: PointerEvent) {
    const prev = el.previousElementSibling as HTMLElement | null
    const next = el.nextElementSibling as HTMLElement | null
    if (!prev || !next) return
    e.preventDefault()
    el.setPointerCapture(e.pointerId)
    dragging = true
    const size = (n: HTMLElement) => (dir === 'x' ? n.getBoundingClientRect().width : n.getBoundingClientRect().height)
    const start = dir === 'x' ? e.clientX : e.clientY
    const pixels: [number, number] = [size(prev), size(next)]

    const move = (ev: PointerEvent) => onresize((dir === 'x' ? ev.clientX : ev.clientY) - start, pixels)
    const up = () => {
      dragging = false
      el.removeEventListener('pointermove', move)
      el.removeEventListener('pointerup', up)
    }
    el.addEventListener('pointermove', move)
    el.addEventListener('pointerup', up)
  }
</script>

<div
  bind:this={el}
  class="gutter {dir}"
  class:dragging
  role="separator"
  aria-orientation={dir === 'x' ? 'vertical' : 'horizontal'}
  title={t('Drag to resize · double-click to reset')}
  onpointerdown={down}
  ondblclick={onreset}
></div>

<style>
  .gutter { flex: 0 0 10px; position: relative; touch-action: none; }
  .gutter.x { cursor: col-resize; }
  .gutter.y { cursor: row-resize; }
  .gutter::after { content: ''; position: absolute; border-radius: var(--r2); background: transparent; transition: background 0.15s; }
  .gutter.x::after { top: 0; bottom: 0; left: 4px; width: 2px; }
  .gutter.y::after { left: 0; right: 0; top: 4px; height: 2px; }
  .gutter:hover::after, .gutter.dragging::after { background: var(--info); }
</style>
