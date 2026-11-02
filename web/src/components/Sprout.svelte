<script lang="ts">
  import { featureOn } from '../fun.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { todayKey } from '../filter.svelte'

  /**
   * Fun: a sprout when nothing is waiting any more (empty Alerts block). It gains a leaf for each
   * day this happens (up to 7), and grows on display. With `[appearance] celebrations`.
   */
  const MAX = 7
  let days = $state<string[]>(pref<string[]>('fun.sprout', []))
  $effect(() => {
    if (!featureOn('celebrations') || days.includes(todayKey())) return
    days = [...days, todayKey()].slice(-MAX)
    setPref('fun.sprout', days)
  })
  /** Leaves from bottom to top, alternating left / right along the stem. */
  const leaves = $derived(Array.from({ length: Math.max(1, days.length) }, (_, i) => ({ y: 34 - i * 4, left: i % 2 === 0 })))
</script>

{#if featureOn('celebrations')}
  <svg class="sprout" viewBox="0 0 40 44" aria-hidden="true">
    <path class="stem" d="M20 42 V{30 - leaves.length * 4}" pathLength="1" />
    {#each leaves as l, i (i)}
      <path class="leaf" style:--i={i} d={l.left ? `M20 ${l.y}c-6 0-9-4-9-8 5 0 9 3 9 8Z` : `M20 ${l.y}c6 0 9-4 9-8-5 0-9 3-9 8Z`} />
    {/each}
    <rect x="11" y="40" width="18" height="3" rx="1.5" />
  </svg>
{/if}

<style>
  .sprout { width: 34px; height: 38px; flex-shrink: 0; }
  .stem { fill: none; stroke: var(--ok); stroke-width: 2; stroke-linecap: round; }
  .leaf { fill: var(--ok); transform-box: fill-box; transform-origin: bottom center; }
  rect { fill: var(--text-muted); opacity: 0.6; }
  :global(html:not([data-reduce-motion])) .stem { stroke-dasharray: 1; stroke-dashoffset: 1; animation: grow 0.9s ease-out forwards; }
  :global(html:not([data-reduce-motion])) .leaf { transform: scale(0); animation: leaf 0.5s cubic-bezier(0.2, 0.8, 0.3, 1.3) forwards; animation-delay: calc(0.5s + var(--i) * 0.12s); }
  @keyframes grow { to { stroke-dashoffset: 0; } }
  @keyframes leaf { to { transform: scale(1); } }
</style>
