<script lang="ts">
  /**
   * Prism logo: a beam enters, comes out split into the sources' colours (GitLab, ClickUp,
   * Kubernetes, Claude).
   *
   * `animated`: the beam replays (it enters, then the four rays burst out one after the other)
   * on open, on hover, and on each `pulse` (the time of the last data received) — at most every
   * 30 s, so as not to flicker constantly — and the prism flashes, then breathes twice: never
   * for ever, an endless fill-opacity animation repainting the window at 60 Hz while idle.
   * Nothing moves with "Reduce motion".
   */
  let { size = 26, animated = false, pulse }: { size?: number; animated?: boolean; pulse?: unknown } = $props()

  const REPLAY_MIN_MS = 30_000
  let run = $state(0)
  let last = 0

  function replay(force = false) {
    if (!animated) return
    const now = Date.now()
    if (!force && now - last < REPLAY_MIN_MS) return
    last = now
    run++
  }

  $effect(() => {
    void pulse
    replay()
  })

  const BEAMS = [
    { d: 'M19.69 14.8 L29 9', c: 'var(--src-gitlab)' },
    { d: 'M20.31 16.2 L29 14', c: 'var(--src-clickup)' },
    { d: 'M20.93 17.6 L29 19', c: 'var(--src-k8s)' },
    { d: 'M21.56 19 L29 24', c: 'var(--src-claude)' },
  ]
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg width={size} height={size} viewBox="0 0 32 32" role="img" aria-label="Prism" class:animated onmouseenter={() => replay(true)}>
  <rect width="32" height="32" rx="8" fill="var(--logo-bg)" />
  {#key run}
    <g class:play={run > 0}>
      <path class="in" pathLength="1" d="M3 17.5 L11.7 16.17" stroke="var(--text)" stroke-width="1.8" stroke-linecap="round" />
      {#each BEAMS as b, i (b.d)}
        <path class="out" pathLength="1" style:--i={i} d={b.d} stroke={b.c} stroke-width="1.8" stroke-linecap="round" />
      {/each}
    </g>
    <path class="prism" class:flash={run > 0} d="M16 6.5 L24 24.5 H8 Z" fill="var(--accent)" fill-opacity="0.28" stroke="var(--accent)" stroke-width="1.9" stroke-linejoin="round" />
  {/key}
</svg>

<style>
  /* The beam draws itself (dasharray over a length normalised to 1), then the rays. */
  .play .in { stroke-dasharray: 1; animation: draw 0.35s ease-out both; }
  .play .out { stroke-dasharray: 1; animation: draw 0.4s cubic-bezier(0.2, 0.8, 0.2, 1) both; animation-delay: calc(0.3s + var(--i) * 0.08s); }
  .animated .prism { transform-origin: 16px 16px; }
  .animated .prism.flash { animation: flash 0.7s ease-out 0.2s, breathe 6s ease-in-out 0.9s 2; }
  @keyframes draw { from { stroke-dashoffset: 1; } to { stroke-dashoffset: 0; } }
  @keyframes breathe { 50% { fill-opacity: 0.45; } }
  @keyframes flash { 40% { fill-opacity: 0.7; transform: scale(1.06); } }
  :global(html[data-reduce-motion]) .play .in, :global(html[data-reduce-motion]) .play .out, :global(html[data-reduce-motion]) .animated .prism { animation: none; }
</style>
