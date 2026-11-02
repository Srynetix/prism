<script lang="ts">
  import { locale } from '../locale'
  import { t } from '../i18n.svelte'
  import { mrNumber } from '../forge'
  import { cycleSaver, saver } from '../fun.svelte'
  import { CANVAS_SAVERS, type CanvasSaver } from '../savers'
  import { pref, setPref } from '../layout.svelte'
  import { live } from '../live.svelte'
  import PrismLogo from './PrismLogo.svelte'

  /**
   * Prism's screensaver, started on demand (console `screensaver` command), never
   * on its own. A click or a key wakes it up, without losing anything.
   * - Bounce: the logo alone bounces and changes colour at each edge; an exact corner counts (record kept).
   * - Matrix, Stars, Life, Spectrum (savers.ts): code rain written with the current tickets, MRs and
   *   branches, hyperspace, game of life, the prism and its rainbow.
   * ← / → switch to the previous / next without waking up.
   */
  const HUES = ['#a78bfa', '#7ee0a8', '#ffc978', '#5ec8ff', '#ff8fa3', '#fc6d26']
  let canvas: HTMLCanvasElement | undefined = $state()
  let g: CanvasRenderingContext2D | null = null
  let clock = $state('')

  /* ---------- Bounce ---------- */
  let logo: HTMLDivElement | undefined = $state()
  let bx = 40
  let by = 40
  let vx = 1.6
  let vy = 1.3
  let hue = $state(0)
  let corners = $state(0)
  let record = $state(pref<number>('fun.corners', 0))

  function frameBounce(dt: number) {
    if (!logo) return
    const W = innerWidth - logo.offsetWidth
    const H = innerHeight - logo.offsetHeight
    // Speeds in px per frame at 60 fps, scaled to the elapsed time.
    const k = Math.min(dt, 100) / 16.7
    bx += vx * k
    by += vy * k
    let hx = false
    let hy = false
    if (bx <= 0 || bx >= W) { vx = -vx; bx = Math.max(0, Math.min(W, bx)); hx = true }
    if (by <= 0 || by >= H) { vy = -vy; by = Math.max(0, Math.min(H, by)); hy = true }
    if (hx || hy) hue = (hue + 1) % HUES.length
    if (hx && hy) {
      corners++
      if (corners > record) {
        record = corners
        setPref('fun.corners', record)
      }
    }
    logo.style.transform = `translate(${bx}px, ${by}px)`
  }

  /* ---------- Canvas savers (savers.ts) ---------- */
  let drawn: CanvasSaver | null = null
  /** The current words, for Matrix: sprint tickets, open MRs, their branches. */
  function words(): string[] {
    const s = live.snapshot
    if (!s) return []
    return [
      ...(s.sprint?.tickets ?? []).map((t) => t.id),
      ...s.merge_requests.filter((m) => m.state === 'opened').flatMap((m) => [mrNumber(m.iid), m.source_branch]),
    ]
  }
  function resetCanvas(mode: keyof typeof CANVAS_SAVERS) {
    if (!canvas) return
    canvas.width = innerWidth * devicePixelRatio
    canvas.height = innerHeight * devicePixelRatio
    g = canvas.getContext('2d')
    g?.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0)
    if (g) {
      g.fillStyle = '#050507'
      g.fillRect(0, 0, innerWidth, innerHeight)
    }
    drawn = CANVAS_SAVERS[mode]()
    drawn.reset(innerWidth, innerHeight, words())
  }
  const isCanvas = (mode: string): mode is keyof typeof CANVAS_SAVERS => mode in CANVAS_SAVERS

  /* ---------- Loop, wake-up ---------- */
  $effect(() => {
    if (!saver.on) return
    const mode = saver.mode
    corners = 0
    let raf = 0
    let last = performance.now()
    if (isCanvas(mode)) queueMicrotask(() => resetCanvas(mode))
    const loop = (t: number) => {
      const dt = t - last
      last = t
      if (mode === 'bounce') frameBounce(dt)
      else if (g && drawn) drawn.frame(g, Math.min(dt, 100))
      clock = new Date().toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
      raf = requestAnimationFrame(loop)
    }
    raf = requestAnimationFrame(loop)
    const wake = (e: Event) => {
      e.preventDefault()
      e.stopPropagation()
      if (e instanceof KeyboardEvent && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) return cycleSaver(e.key === 'ArrowRight' ? 1 : -1)
      saver.on = false
    }
    const resize = () => isCanvas(mode) && resetCanvas(mode)
    addEventListener('resize', resize)
    // After the keystroke that started it, so that it does not wake it immediately.
    const arm = setTimeout(() => {
      addEventListener('keydown', wake, true)
      addEventListener('mousedown', wake, true)
    }, 200)
    return () => {
      cancelAnimationFrame(raf)
      clearTimeout(arm)
      removeEventListener('keydown', wake, true)
      removeEventListener('mousedown', wake, true)
      removeEventListener('resize', resize)
    }
  })
</script>

{#if saver.on}
  <div class="saver" role="presentation">
    {#if isCanvas(saver.mode)}
      <canvas bind:this={canvas}></canvas>
      <span class="corners">{saver.mode} · ← →</span>
    {:else}
      <div class="logo" bind:this={logo} style:color={HUES[hue]} style:filter="drop-shadow(0 0 14px {HUES[hue]})"><PrismLogo size={96} /></div>
      <span class="corners">{t('corners {corners} · record {record}', { corners, record })}</span>
    {/if}
    <span class="clock">{clock}</span>
  </div>
{/if}

<style>
  .saver { position: fixed; inset: 0; z-index: 95; background: #050507; cursor: none; overflow: hidden; animation: in 0.6s ease-out; }
  @keyframes in { from { opacity: 0; } }
  canvas { position: absolute; inset: 0; width: 100vw; height: 100vh; }
  .logo { position: absolute; left: 0; top: 0; display: flex; }
  .clock, .corners { position: absolute; bottom: 14px; font: 11px var(--mono); color: #55556a; }
  .clock { left: 18px; }
  .corners { right: 18px; }
</style>
