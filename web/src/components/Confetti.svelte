<script lang="ts">
  import { bursts, type Burst } from '../fun.svelte'

  /**
   * Confetti (fun.svelte.ts `celebrate`) on a canvas that covers the window without capturing the
   * mouse: each burst lives ~1.5 s, then the canvas empties and draws nothing more.
   */
  const COLORS = ['#a78bfa', '#7ee0a8', '#ffc978', '#5ec8ff', '#ff8fa3', '#fc6d26']
  const LIFE = 90

  interface Bit { x: number; y: number; vx: number; vy: number; s: number; c: string; r: number; vr: number; t: number }
  let canvas: HTMLCanvasElement | undefined = $state()
  let bits: Bit[] = []
  let running = false

  function add(b: Burst) {
    const colors = b.colors ?? COLORS
    for (let i = 0; i < b.count; i++) {
      const a = -Math.PI / 2 + (Math.random() - 0.5) * b.spread
      const v = 4 + Math.random() * 5
      bits.push({ x: b.x, y: b.y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, s: 3 + Math.random() * 4, c: colors[(Math.random() * colors.length) | 0], r: Math.random() * 6, vr: (Math.random() - 0.5) * 0.4, t: 0 })
    }
  }

  function tick() {
    const g = canvas?.getContext('2d')
    if (!canvas || !g) return void (running = false)
    g.clearRect(0, 0, canvas.width, canvas.height)
    g.save()
    g.scale(devicePixelRatio, devicePixelRatio)
    for (const p of bits) {
      p.vy += 0.18
      p.vx *= 0.99
      p.x += p.vx
      p.y += p.vy
      p.r += p.vr
      p.t++
      g.save()
      g.translate(p.x, p.y)
      g.rotate(p.r)
      g.globalAlpha = Math.max(0, 1 - p.t / LIFE)
      g.fillStyle = p.c
      g.fillRect(-p.s / 2, -p.s / 4, p.s, p.s / 2)
      g.restore()
    }
    g.restore()
    bits = bits.filter((p) => p.t < LIFE)
    if (bits.length) requestAnimationFrame(tick)
    else running = false
  }

  $effect(() => {
    if (!bursts.length || !canvas) return
    canvas.width = innerWidth * devicePixelRatio
    canvas.height = innerHeight * devicePixelRatio
    for (const b of bursts.splice(0)) add(b)
    if (!running) {
      running = true
      requestAnimationFrame(tick)
    }
  })
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
  canvas { position: fixed; inset: 0; width: 100vw; height: 100vh; z-index: 90; pointer-events: none; }
</style>
