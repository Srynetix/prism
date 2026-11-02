<script lang="ts">
  import type { Face, Form, Stage } from './growth'

  /**
   * The Pet's body: its stage, its form's accessories, its face. The block draws it alive (eyes
   * following the pointer, hearts, a jump); its friends (the agents), its photos and the status
   * bar draw it still, `color` the agent's CLI's for a friend.
   */
  let {
    stage,
    form = null,
    face,
    look = { x: 0, y: 0 },
    closed = face === 'sleep',
    jumping = false,
    hearts = [],
    color,
    label,
    onpoke,
    el = $bindable(),
  }: {
    stage: Stage
    form?: Form | null
    face: Face
    look?: { x: number; y: number }
    closed?: boolean
    jumping?: boolean
    hearts?: number[]
    color?: string
    /** Clickable (to pet it): its name for a screen reader. */
    label?: string
    onpoke?: () => void
    el?: SVGSVGElement
  } = $props()

  const scale = $derived(stage === 'baby' ? 0.78 : stage === 'young' ? 0.9 : 1)
  /** Its hat (Night Owl, Architect) takes the antenna's place, and lifts the crown. */
  const hat = $derived(form === 'night' || form === 'architect')
</script>

<svg bind:this={el} class="pet {face}" class:jump={jumping} class:poke={!!onpoke} viewBox="-60 -60 120 120" {...onpoke ? { role: 'button', tabindex: 0 } : { role: 'img' }} aria-label={label} aria-hidden={label ? undefined : 'true'} onclick={onpoke} onkeydown={(e) => onpoke && (e.key === 'Enter' || e.key === ' ') && (e.preventDefault(), onpoke())} style:--s={scale} style:--body={color}>
  <g class="whole">
    {#if stage === 'legend'}<path class="crown" d="M-18 -44 L-12 -56 L-4 -46 L0 -58 L4 -46 L12 -56 L18 -44 Z" transform={hat ? 'translate(0 -14)' : undefined} />{/if}
    {#if (stage === 'grown-up' || stage === 'legend') && !hat}<line class="antenna" x1="0" y1="-38" x2="6" y2="-52" /><circle class="tip" cx="6" cy="-53" r="4" />{/if}
    <ellipse class="shadow" cx="0" cy="46" rx="30" ry="5" />
    {#if form === 'shipper'}<path class="fin" d="M-30 24 L-46 40 L-26 38 Z M30 24 L46 40 L26 38 Z" /><path class="flame" d="M-8 43 Q 0 60 8 43 Z" />{/if}
    <path class="blob" d="M0 -40 C 30 -40 44 -14 42 10 C 40 34 22 44 0 44 C -22 44 -40 34 -42 10 C -44 -14 -30 -40 0 -40 Z" />
    {#if stage === 'baby'}<path class="tuft" d="M-4 -40 Q 0 -50 4 -40" />{/if}
    <!-- Eyes -->
    {#each [-14, 14] as ex (ex)}
      {#if closed}
        <path class="lid" d="M{ex - 7} -6 Q {ex} {face === 'sleep' ? -2 : -9} {ex + 7} -6" />
      {:else if face === 'sick'}
        <path class="lid" d="M{ex - 6} -12 L{ex + 6} 0 M{ex + 6} -12 L{ex - 6} 0" />
      {:else}
        <circle class="eye" cx={ex} cy="-6" r="8" />
        <circle class="pupil" cx={ex + look.x} cy={-6 + look.y} r={face === 'wow' ? 5 : 4} />
        <circle class="glint" cx={ex + look.x + 1.5} cy={-8 + look.y} r="1.3" />
      {/if}
    {/each}
    <!-- Cheeks -->
    {#if face === 'happy' || face === 'party'}<circle class="cheek" cx="-24" cy="8" r="5" /><circle class="cheek" cx="24" cy="8" r="5" />{/if}
    <!-- Mouth -->
    {#if face === 'happy'}<path class="mouth" d="M-8 10 Q 0 18 8 10" />
    {:else if face === 'party'}<path class="open" d="M-10 8 Q 0 26 10 8 Z" />
    {:else if face === 'wow'}<ellipse class="open" cx="0" cy="13" rx="5" ry="6" />
    {:else if face === 'sleep'}<path class="mouth" d="M-4 13 Q 0 15 4 13" />
    {:else if face === 'tired'}<path class="mouth" d="M-7 14 L 7 14" />
    {:else}<path class="mouth" d="M-8 16 Q 0 9 8 16" />{/if}
    {#if face === 'sick'}<path class="sweat" d="M30 -18 q 4 7 0 9 q -4 -2 0 -9 Z" />{/if}
    <!-- Its form's accessories (`[pet] forms`). -->
    {#if form === 'reviewer'}
      <circle class="acc" cx="-14" cy="-6" r="11" /><circle class="acc" cx="14" cy="-6" r="11" /><path class="acc" d="M-3 -6 L3 -6 M-25 -8 L-36 -14 M25 -8 L36 -14" />
      <rect class="paper" x="20" y="20" width="22" height="16" rx="2" /><path class="ink-line" d="M31 20 L31 36" />
    {:else if form === 'shipper'}
      <circle class="porthole" cx="0" cy="-30" r="6" />
    {:else if form === 'fixer'}
      <path class="wrench" d="M-40 -10 L-26 4" /><circle class="wrench-head" cx="-42" cy="-12" r="6" />
      <rect class="plaster" x="14" y="-34" width="18" height="7" rx="3" transform="rotate(-25 23 -30)" />
    {:else if form === 'night'}
      <path class="nightcap" d="M-28 -30 Q 0 -70 30 -34 Z" /><circle class="pompom" cx="34" cy="-32" r="5" />
      <path class="moon" d="M-50 -40 a 10 10 0 1 0 12 12 a 8 8 0 1 1 -12 -12 Z" />
    {:else if form === 'architect'}
      <path class="hardhat" d="M-30 -30 Q 0 -60 30 -30 Z" /><rect class="hardhat" x="-34" y="-32" width="68" height="6" rx="3" />
      <rect class="blueprint" x="-46" y="14" width="22" height="26" rx="2" /><path class="blueprint-lines" d="M-42 20 L-28 20 M-42 26 L-32 26 M-42 32 L-30 32" />
    {/if}
  </g>
  {#if face === 'sleep'}<text class="z" x="30" y="-30">z</text><text class="z z2" x="40" y="-42">Z</text>{/if}
  {#each hearts as h (h)}<text class="heart" x="-4" y="-44">♥</text>{/each}
</svg>

<style>
  .pet { width: 100%; height: 100%; overflow: visible; outline: none; }
  .pet.poke { cursor: pointer; }
  .whole { transform: scale(var(--s)); transform-origin: 0 44px; transform-box: view-box; animation: idle 2.6s ease-in-out infinite; }
  .jump .whole { animation: jump 0.45s ease-out 2; }
  .party .whole { animation: dance 0.7s ease-in-out infinite; }
  .sleep .whole { animation: breathe 3.5s ease-in-out infinite; }
  .blob { fill: var(--body, var(--src-claude)); }
  .sick .blob { fill: color-mix(in srgb, var(--ok) 55%, var(--body, var(--src-claude))); }
  .shadow { fill: #000; opacity: 0.12; }
  .tuft, .antenna { fill: none; stroke: var(--body, var(--src-claude)); stroke-width: 3; stroke-linecap: round; }
  .tip { fill: var(--src-alert); }
  .crown { fill: var(--src-alert); }
  .eye { fill: #fff; }
  .pupil { fill: #1d1d1b; transition: cx 0.08s, cy 0.08s; }
  .glint { fill: #fff; }
  .lid, .mouth { fill: none; stroke: #1d1d1b; stroke-width: 2.4; stroke-linecap: round; }
  .open { fill: #1d1d1b; }
  .cheek { fill: #ff7aa2; opacity: 0.45; }
  .sweat { fill: #8fd3ff; }
  .z { fill: var(--text-muted); font: 700 12px var(--sans); animation: float 2.4s ease-in-out infinite; }
  .z2 { font-size: 16px; animation-delay: 0.8s; }
  .heart { fill: var(--ko); font-size: 16px; animation: up 1s ease-out forwards; }
  .fin { fill: #8a8f98; }
  .flame { fill: #ff9a3c; }
  .acc, .ink-line, .blueprint-lines { fill: none; stroke: #1d1d1b; stroke-width: 2.4; stroke-linecap: round; }
  .ink-line { stroke-width: 1.4; }
  .blueprint-lines { stroke: #b5d4f4; stroke-width: 1.4; }
  .paper { fill: #f3ecd9; stroke: #1d1d1b; stroke-width: 2; }
  .porthole { fill: #b5d4f4; stroke: #1d1d1b; stroke-width: 2; }
  .wrench { stroke: #8a8f98; stroke-width: 6; stroke-linecap: round; }
  .wrench-head { fill: none; stroke: #8a8f98; stroke-width: 4; }
  .plaster { fill: #f3ecd9; }
  .nightcap { fill: #3e5c99; }
  .pompom { fill: #fff; }
  .moon, .hardhat { fill: var(--src-alert); }
  .blueprint { fill: #2f5d8c; stroke: #1d1d1b; stroke-width: 2; }
  @keyframes idle { 0%, 100% { transform: scale(var(--s)) translateY(0); } 50% { transform: scale(var(--s)) translateY(-2px) scaleY(1.02); } }
  @keyframes breathe { 0%, 100% { transform: scale(var(--s)) scaleY(1); } 50% { transform: scale(var(--s)) scaleY(0.96); } }
  @keyframes jump { 0%, 100% { transform: scale(var(--s)) translateY(0); } 40% { transform: scale(var(--s)) translateY(-22px) rotate(8deg); } }
  @keyframes dance { 0%, 100% { transform: scale(var(--s)) rotate(-7deg); } 50% { transform: scale(var(--s)) rotate(7deg) translateY(-4px); } }
  @keyframes float { 0% { opacity: 0; transform: translateY(4px); } 50% { opacity: 1; } 100% { opacity: 0; transform: translateY(-8px); } }
  @keyframes up { to { transform: translateY(-20px); opacity: 0; } }
  :global(html[data-reduce-motion]) .whole, :global(html[data-reduce-motion]) .jump .whole, :global(html[data-reduce-motion]) .party .whole, :global(html[data-reduce-motion]) .sleep .whole, :global(html[data-reduce-motion]) .z, :global(html[data-reduce-motion]) .heart { animation: none; }
</style>
