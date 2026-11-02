<script lang="ts">
  import { onMount } from 'svelte'

  /**
   * A theme's own motion (`[appearance] theme_motion`), in two layers over the whole window: one
   * behind the blocks (Synthwave's sky: stars, a striped sun, a grid rolling toward you; Hacker's
   * pale rain), one over them that no click sees (Hacker's CRT sweep, phosphor and darker edges).
   * Always in the page, shown by the CSS from `<html data-theme data-theme-motion>`: a theme
   * hovered in the Theme menu gets its motion too. The rain is drawn only while it shows.
   */

  /** Stars as box-shadows of three dots, each group twinkling on its own beat. */
  const stars = [0, 1, 2].map(() =>
    Array.from({ length: 22 }, () => `${(Math.random() * 100).toFixed(1)}vw ${(Math.random() * 58).toFixed(1)}vh rgba(255, 255, 255, ${(0.4 + Math.random() * 0.5).toFixed(2)})`).join(', '),
  )

  let canvas: HTMLCanvasElement
  onMount(() => {
    const html = document.documentElement
    const ctx = canvas.getContext('2d')
    const STEP = 14
    let drops: number[] = []
    let frame = 0
    let font = ''
    const size = () => {
      canvas.width = canvas.offsetWidth
      canvas.height = canvas.offsetHeight
      drops = Array.from({ length: Math.ceil(canvas.width / STEP) }, () => Math.random() * -50)
      font = `12px ${getComputedStyle(html).getPropertyValue('--mono') || 'monospace'}`
    }
    // About 14 frames a second: a rain, not a game. A timer, not requestAnimationFrame, which
    // would wake the window 60 times a second to draw 14; nothing drawn while it is hidden.
    const draw = () => {
      frame = window.setTimeout(draw, 70)
      if (!ctx || document.hidden) return
      ctx.fillStyle = 'rgba(5, 8, 5, 0.18)'
      ctx.fillRect(0, 0, canvas.width, canvas.height)
      ctx.fillStyle = '#39ff14'
      ctx.font = font
      drops.forEach((y, i) => {
        ctx.fillText(String.fromCharCode(0x30a0 + Math.floor(Math.random() * 96)), i * STEP, y * STEP)
        drops[i] = y * STEP > canvas.height && Math.random() > 0.975 ? 0 : y + 1
      })
    }
    const raining = () => html.dataset.theme === 'hacker' && html.dataset.themeMotion === 'on'
    const sync = () => {
      if (raining() && !frame) {
        size()
        draw()
      } else if (!raining() && frame) {
        clearTimeout(frame)
        frame = 0
      }
    }
    const watch = new MutationObserver(sync)
    watch.observe(html, { attributes: true, attributeFilter: ['data-theme', 'data-theme-motion'] })
    const resized = () => frame && size()
    addEventListener('resize', resized)
    sync()
    return () => {
      watch.disconnect()
      removeEventListener('resize', resized)
      clearTimeout(frame)
    }
  })
</script>

<div class="scene back" aria-hidden="true">
  <div class="sw">
    {#each stars as s, i (i)}<span class="stars" style:box-shadow={s} style:animation-delay="{-i * 1.3}s"></span>{/each}
    <div class="glow"></div>
    <div class="sun"></div>
    <div class="floor"><div class="plane"><div class="grid"></div></div></div>
  </div>
  <canvas class="rain" bind:this={canvas}></canvas>
</div>
<div class="scene front" aria-hidden="true">
  <div class="sweep"></div>
  <div class="phosphor"></div>
</div>

<style>
  .scene { position: fixed; inset: 0; pointer-events: none; overflow: hidden; }
  .back { z-index: -1; }
  .front { z-index: 9998; }
  .sw, .rain, .sweep, .phosphor { display: none; }
  :global(html[data-theme-motion='on'][data-theme='synthwave']) .sw { display: block; }
  :global(html[data-theme-motion='on'][data-theme='hacker']) :is(.rain, .sweep, .phosphor) { display: block; }

  /* Synthwave: the horizon at 62% of the window, as the still sky of app.css. */
  .stars { position: absolute; left: 0; top: 0; width: 2px; height: 2px; border-radius: 50%; animation: twinkle 4s ease-in-out infinite; }
  .stars:nth-child(2) { animation-duration: 5.5s; }
  .stars:nth-child(3) { animation-duration: 3.2s; }
  @keyframes twinkle { 50% { opacity: 0.15; } }
  .glow { position: absolute; left: 50%; top: 62%; width: min(90vw, 1100px); height: 46vh; transform: translate(-50%, -62%); background: radial-gradient(ellipse at 50% 62%, rgba(255, 126, 219, 0.32), transparent 62%); animation: breathe 8s ease-in-out infinite; }
  .sun {
    position: absolute; left: 50%; top: 62%; width: min(30vw, 380px); aspect-ratio: 2; transform: translate(-50%, -100%);
    border-radius: 999px 999px 0 0; background: linear-gradient(180deg, #fede5d 0%, #ff8b39 45%, #ff7edb 100%); opacity: 0.42;
    -webkit-mask: repeating-linear-gradient(180deg, #000 0 10px, transparent 10px 14px); mask: repeating-linear-gradient(180deg, #000 0 10px, transparent 10px 14px);
    /* 28 px in 7 s: a pixel at a time, four times a second, is what the eye sees of it anyway; a
       moving mask repaints the sun, at 60 Hz when continuous. */
    animation: bands 7s steps(28) infinite, breathe 8s ease-in-out infinite;
  }
  @keyframes bands { to { -webkit-mask-position: 0 28px; mask-position: 0 28px; } }
  @keyframes breathe { 50% { opacity: 0.62; } }
  .floor { position: absolute; left: 0; right: 0; top: 62%; bottom: 0; overflow: hidden; perspective: 320px; perspective-origin: 50% 0; -webkit-mask: linear-gradient(transparent, #000 35%); mask: linear-gradient(transparent, #000 35%); }
  .plane { position: absolute; left: -100%; right: -100%; top: 0; height: 260%; transform-origin: 50% 0; transform: rotateX(72deg); overflow: hidden; }
  /* One cell taller than the plane, slid by one cell and back: the grid rolls on, composited. */
  .grid {
    position: absolute; left: 0; right: 0; top: -64px; bottom: 0;
    background: repeating-linear-gradient(90deg, rgba(255, 126, 219, 0.4) 0 1px, transparent 1px 64px), repeating-linear-gradient(0deg, rgba(255, 126, 219, 0.4) 0 1px, transparent 1px 64px);
    animation: roll 2.6s linear infinite;
  }
  @keyframes roll { to { transform: translateY(64px); } }

  /* Hacker. */
  .rain { position: absolute; inset: 0; width: 100%; height: 100%; opacity: 0.14; }
  .sweep { position: absolute; left: 0; right: 0; top: 0; height: 120px; background: linear-gradient(180deg, transparent, rgba(57, 255, 20, 0.05) 45%, rgba(57, 255, 20, 0.08) 50%, transparent); transform: translateY(-120px); animation: sweep 7s linear infinite; }
  @keyframes sweep { to { transform: translateY(100vh); } }
  .phosphor { position: absolute; inset: 0; background: radial-gradient(ellipse at center, transparent 62%, rgba(0, 0, 0, 0.45)); }
  /* Its flicker: a black layer's opacity, which the GPU composites (a background colour repaints). */
  .phosphor::after { content: ''; position: absolute; inset: 0; background: #000; opacity: 0; animation: phosphor 4s steps(1) infinite; }
  @keyframes phosphor { 0%, 100% { opacity: 0; } 33% { opacity: 0.04; } 34% { opacity: 0; } 71% { opacity: 0.03; } 72% { opacity: 0; } }
</style>
