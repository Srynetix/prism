<script lang="ts">
  import { onMount } from 'svelte'
  import ProfileForm, { type NewProfile } from './ProfileForm.svelte'
  import { PROFILE_COLORS, setupProfile } from '../profiles.svelte'
  import { live } from '../live.svelte'
  import { reducedMotion } from '../fun.svelte'
  import { t } from '../i18n.svelte'

  /**
   * First launch, without config.toml: a ray of light crosses the prism and fans out across the
   * window, Prism says hello, then asks for the profile to start from (empty with the starter pack,
   * a team's pack, or the demo). Creating it writes config.toml and relaunches Prism on it; the
   * pages wizard follows when the profile has no pages. A click or a key skips the animation;
   * nothing moves with "Reduce motion". "Later" leaves the app as it is, without config.
   * Everything is in the flow: the scene and the title shrink and rise as the card unfolds under
   * them, so a taller card (a pack's source) pushes them up instead of over each other.
   */
  const INTRO_MS = 2600
  const RAYS = ['#ff5f6d', '#ffc371', '#fff36e', '#7cf29a', '#5ec8ff', '#a78bfa']

  let skipped = $state(false)
  let ready = $state(false)
  let later = $state(false)
  /** The card has finished unfolding: its shadow may leave its fold. */
  let open = $state(false)
  let form = $state<NewProfile>({ name: '', color: PROFILE_COLORS[1], start: 'empty', pack: '' })
  let busy = $state(false)
  let error = $state('')

  const done = () => {
    skipped = true
    ready = true
  }
  onMount(() => {
    if (reducedMotion()) return done()
    const timer = setTimeout(() => (ready = true), INTRO_MS)
    const skip = () => !ready && done()
    addEventListener('keydown', skip, { once: true })
    return () => {
      clearTimeout(timer)
      removeEventListener('keydown', skip)
    }
  })

  const valid = $derived(!!form.name.trim() && (form.start !== 'pack' || !!form.pack.trim()))
  async function start() {
    if (!valid || busy) return
    busy = true
    error = ''
    try {
      // Prism relaunches on the new config: this call does not come back when it works.
      await setupProfile(form.name.trim(), form.color, form.start === 'pack' ? form.pack.trim() : null, form.start === 'demo')
    } catch (e) {
      error = String(e)
      busy = false
    }
  }
</script>

{#if live.snapshot?.profile?.fresh && !later}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="first" class:ready class:skipped onclick={() => !ready && done()}>
    <div class="stage">
      <svg class="scene" viewBox="260 70 480 260" aria-hidden="true">
        <defs>
          <linearGradient id="fl-inner" x1="0" x2="1">
            <stop offset="0" stop-color="#fff" />
            <stop offset="1" stop-color="#ffd6f0" />
          </linearGradient>
          <linearGradient id="fl-wedge" x1="0" y1="0" x2="0" y2="1">
            {#each RAYS as c, i (c)}<stop offset={i / (RAYS.length - 1)} stop-color={c} />{/each}
          </linearGradient>
          <radialGradient id="fl-flash">
            <stop offset="0" stop-color="#fff" stop-opacity="0.9" />
            <stop offset="0.35" stop-color="#c4b5fd" stop-opacity="0.35" />
            <stop offset="1" stop-color="#a78bfa" stop-opacity="0" />
          </radialGradient>
        </defs>
        <!-- The spectrum's glow, behind the rays. -->
        <path class="wedge" d="M547 214 L2200 -90 L2200 520 Z" fill="url(#fl-wedge)" />
        <path class="beam" pathLength="1" d="M-1200 240 L453 216" />
        {#each RAYS as c, i (c)}
          <path class="ray" pathLength="1" style:--i={i} stroke={c} d="M547 {207 + i * 3} L2200 {207 + i * 3 + (i - 2.5) * 0.07 * 1653}" />
        {/each}
        <circle class="flash" cx="500" cy="215" r="120" fill="url(#fl-flash)" />
        <path class="prism" pathLength="1" d="M500 110 L580 290 H420 Z" />
        <path class="inner" pathLength="1" d="M453 216 L547 213" stroke="url(#fl-inner)" />
      </svg>

      <div class="hello">
        <h1>{t('Welcome to')} <b>Prism</b></h1>
        <p>{t('Your merge requests, tickets, pipelines, clusters and agents, split into what needs you.')}</p>
      </div>
    </div>

    <div class="fold" class:open={open || skipped} ontransitionend={(e) => e.target === e.currentTarget && (open = ready)}>
      <div>
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <section class="card" aria-label={t('Your first profile')} onkeydown={(e) => e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT' && start()}>
          <h2>{t('Your profile')}</h2>
          <p class="lead">{t('A profile is a setup of Prism: its config.toml, its pages, its agents and notes. Start with one; Settings › General › Profiles adds others (work, personal, a demo).')}</p>
          {#if ready}<ProfileForm bind:value={form} autofocus />{/if}
          {#if error}<p class="err">{error}</p>{/if}
          <div class="foot">
            <button class="btn ghost" onclick={() => (later = true)} title={t('Leave Prism without config: Settings or a config.toml set it up later')}>{t('Later')}</button>
            <button class="btn primary" onclick={start} disabled={!valid || busy}>{busy ? t('Setting up…') : t('Start Prism')}</button>
          </div>
        </section>
      </div>
    </div>

    {#if !ready}<span class="skip">{t('click or press a key to skip')}</span>{/if}
  </div>
{/if}

<style>
  /* In the flow, centered: the stage alone during the intro, then the stage above the card. */
  .first { position: fixed; inset: 0; z-index: 110; display: flex; flex-direction: column; align-items: center; padding: 3vh 16px; overflow: hidden auto; background: radial-gradient(ellipse at 50% 30%, #17151f 0%, #0b0b0d 65%); color: #cfcfd6; }
  .stage { flex: none; width: calc(100% + 32px); margin: auto -16px 0; display: flex; flex-direction: column; align-items: center; mask-image: linear-gradient(90deg, transparent, #000 12%, #000 88%, transparent); }
  /* The prism keeps its proportions; the beam and the rays run past the box to the window's edges. */
  .scene { display: block; width: 100%; height: min(48vh, 440px); overflow: visible; transition: height 0.9s cubic-bezier(0.65, 0, 0.25, 1); }
  .ready .scene { height: min(26vh, 220px); }
  .scene path { fill: none; stroke-linecap: round; }

  /* The prism draws itself, the ray enters and lights it up, the spectrum fans out across the window. */
  .prism { fill: #a78bfa; fill-opacity: 0.22; stroke: #a78bfa; stroke-width: 2.4; stroke-linejoin: round; stroke-dasharray: 1; transform-origin: 500px 215px; animation: draw 0.8s cubic-bezier(0.45, 0, 0.2, 1) 0.1s both, fill 0.4s ease-out 0.5s backwards, flash 0.6s ease-out 0.95s, breathe 6s ease-in-out 2.6s infinite; }
  .beam { stroke: #fff; stroke-width: 2.4; filter: drop-shadow(0 0 6px #fff); stroke-dasharray: 1; animation: draw 0.7s cubic-bezier(0.5, 0, 0.9, 0.7) 0.25s both; }
  .inner { stroke-width: 3; stroke-dasharray: 1; filter: drop-shadow(0 0 4px #fff); animation: draw 0.12s linear 0.95s both; }
  .flash { opacity: 0; transform-origin: 500px 215px; animation: burst 1.1s ease-out 0.92s; }
  .ray { stroke-width: 3; opacity: 0.9; filter: drop-shadow(0 0 5px currentColor); stroke-dasharray: 1; animation: draw 1.1s cubic-bezier(0.15, 0.7, 0.3, 1) calc(1.05s + var(--i) * 0.06s) both, shimmer 4s ease-in-out calc(2.6s + var(--i) * 0.35s) infinite; }
  .wedge { opacity: 0; animation: glow 1.2s ease-out 1.3s forwards; }
  @keyframes draw { from { stroke-dashoffset: 1; } to { stroke-dashoffset: 0; } }
  @keyframes fill { from { fill-opacity: 0; } }
  @keyframes flash { 35% { fill-opacity: 0.65; transform: scale(1.07); } }
  @keyframes breathe { 50% { fill-opacity: 0.36; } }
  @keyframes burst { 0% { opacity: 0; transform: scale(0.2); } 20% { opacity: 1; } 100% { opacity: 0; transform: scale(1.4); } }
  @keyframes shimmer { 50% { opacity: 0.55; } }
  @keyframes glow { to { opacity: 0.1; } }

  .hello { margin-top: 10px; padding: 0 16px; text-align: center; animation: rise 0.9s cubic-bezier(0.2, 0.8, 0.2, 1) 1.6s both; }
  h1 { margin: 0; font-size: 34px; font-weight: 300; letter-spacing: 0.3px; color: #fff; transition: font-size 0.9s cubic-bezier(0.65, 0, 0.25, 1); }
  .ready h1 { font-size: 28px; }
  h1 b { font-weight: 700; background: linear-gradient(90deg, #ff5f6d, #ffc371, #7cf29a, #5ec8ff, #a78bfa, #ff5f6d); background-size: 200% 100%; -webkit-background-clip: text; background-clip: text; color: transparent; animation: sweep 1.6s ease-out 1.8s both; }
  .hello p { margin: 8px 0 0; font-size: 14px; color: #9c9ba8; }
  @keyframes rise { from { opacity: 0; transform: translateY(12px); filter: blur(6px); letter-spacing: 3px; } }
  @keyframes sweep { from { background-position: 100% 0; } to { background-position: 0 0; } }

  /* The card unfolds under the stage (0fr → 1fr), pushing it up; its shadow is clipped until open. */
  .fold { flex: none; display: grid; grid-template-rows: 0fr; width: min(680px, 100%); margin-bottom: auto; transition: grid-template-rows 0.9s cubic-bezier(0.65, 0, 0.25, 1); }
  .ready .fold { grid-template-rows: 1fr; }
  .fold > div { min-height: 0; overflow: hidden; }
  .fold.open > div { overflow: visible; }
  .card { margin-top: 26px; padding: 18px 20px 16px; border: 1px solid var(--border); border-radius: var(--r14); background: var(--surface); color: var(--text); box-shadow: var(--shadow-modal); opacity: 0; transform: translateY(18px) scale(0.98); pointer-events: none; transition: opacity 0.6s ease 0.35s, transform 0.7s cubic-bezier(0.2, 0.8, 0.2, 1) 0.35s; }
  .ready .card { opacity: 1; transform: none; pointer-events: auto; }
  h2 { margin: 0 0 4px; font-size: 15px; }
  .lead { margin: 0 0 14px; font-size: var(--fs-meta); color: var(--text-muted); line-height: 1.45; }
  .err { margin: 10px 0 0; color: var(--ko); font-size: var(--fs-meta); }
  .foot { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--border); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 5px 14px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.ghost { background: none; color: var(--text-muted); }
  .btn.primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .skip { position: fixed; bottom: 14px; right: 20px; font-size: 11px; color: #55556a; }

  /* Skipped, or "Reduce motion": everything in its final place at once. */
  .skipped :is(.beam, .ray, .inner, .wedge, .hello, h1 b) { animation-duration: 1ms !important; animation-delay: 0s !important; animation-iteration-count: 1 !important; }
  .skipped .prism { animation: none; stroke-dasharray: none; }
  .skipped .flash { animation: none; }
  .skipped :is(.scene, h1, .fold, .card) { transition-duration: 1ms !important; transition-delay: 0s !important; }
  :global(html[data-reduce-motion]) .beam, :global(html[data-reduce-motion]) .ray, :global(html[data-reduce-motion]) .inner, :global(html[data-reduce-motion]) .wedge, :global(html[data-reduce-motion]) .prism, :global(html[data-reduce-motion]) .flash, :global(html[data-reduce-motion]) .hello, :global(html[data-reduce-motion]) h1 b { animation: none !important; }
  :global(html[data-reduce-motion]) .wedge { opacity: 0.1; }
  :global(html[data-reduce-motion]) .prism { stroke-dasharray: none; }
  @media (max-height: 640px) {
    .scene { height: 34vh; }
    .ready .scene { height: 18vh; }
    h1, .ready h1 { font-size: 24px; }
  }
</style>
