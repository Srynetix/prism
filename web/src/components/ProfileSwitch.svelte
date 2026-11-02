<script lang="ts">
  import { activeProfile, initial, profileColor, profiles } from '../profiles.svelte'
  import { live } from '../live.svelte'
  import { t, tn } from '../i18n.svelte'

  /** While Prism relaunches on another profile: where it comes from, where it goes. */
  const from = $derived(activeProfile())
  const to = $derived(profiles.switching)
  const running = $derived((live.snapshot?.agents ?? []).filter((a) => a.state === 'running').length)
</script>

{#if to}
  <div class="switch" role="status">
    <div class="pair">
      <span class="pf" style:--c={profileColor(from.color)}>{initial(from.name)}</span>→<span class="pf" style:--c={profileColor(to.color)}>{initial(to.name)}</span>
    </div>
    <b>{t('Switching to {name}', { name: to.name })}</b>
    <div class="bar" style:--c={profileColor(to.color)}><i></i></div>
    {#if running}<small>{tn(running, '{n} {name} agent keeps running; it comes back when you switch to {name} again.', '{n} {name} agents keep running; they come back when you switch to {name} again.', { name: from.name })}</small>{/if}
  </div>
{/if}

<style>
  .switch { position: fixed; inset: 0; z-index: 100; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; background: color-mix(in srgb, var(--bg) 90%, transparent); backdrop-filter: blur(4px); text-align: center; }
  .pair { display: flex; align-items: center; gap: 14px; font-size: 20px; color: var(--text-muted); }
  .pf { --c: var(--accent); display: inline-flex; align-items: center; justify-content: center; width: 48px; height: 48px; border-radius: var(--r13); background: color-mix(in srgb, var(--c) 18%, transparent); color: var(--c); border: 2px solid color-mix(in srgb, var(--c) 55%, transparent); font-weight: 700; font-size: 20px; }
  b { font-size: 15px; color: var(--text); }
  small { max-width: 360px; color: var(--text-muted); }
  .bar { width: 180px; height: 3px; border-radius: var(--r2); background: var(--surface-muted); overflow: hidden; }
  .bar i { display: block; width: 40%; height: 100%; background: var(--c); animation: slide 1s ease-in-out infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(250%); } }
  :global(html[data-reduce-motion]) .bar i { animation: none; width: 100%; }
</style>
