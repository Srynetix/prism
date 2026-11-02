<script lang="ts">
  import { STEPS, stepLabel, stepStates, type Journey } from '../journey'
  import { t } from '../i18n.svelte'

  /**
   * A ticket's seven steps, from refined to done: those passed, the current one (red when it
   * failed), those ahead. `mini`: bars only, each step's name in its tooltip (the In progress block).
   */
  let { journey, mini = false }: { journey: Journey; mini?: boolean } = $props()
  const states = $derived(stepStates(journey))
  const STATE: Record<string, string> = { done: t('passed'), now: t('current'), ko: t('failed'), todo: t('ahead') }
</script>

<div class="strip" class:mini role="img" aria-label={STEPS.map((s, i) => `${stepLabel(s)}: ${STATE[states[i]]}`).join(', ')}>
  {#each STEPS as s, i (s)}
    <span class="st {states[i]}" title="{stepLabel(s)} · {STATE[states[i]]}"><i></i>{#if !mini}<span>{stepLabel(s)}</span>{/if}</span>
  {/each}
</div>

<style>
  .strip { display: flex; align-items: flex-start; }
  .st { position: relative; flex: 1; display: flex; flex-direction: column; align-items: center; gap: 3px; min-width: 0; font-size: var(--fs-tag); color: var(--text-muted); }
  .st i { z-index: 1; width: 11px; height: 11px; border-radius: 50%; border: 2px solid var(--border); background: var(--surface); box-sizing: border-box; }
  .st span { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .st::before { content: ''; position: absolute; top: 4.5px; right: 50%; width: 100%; height: 2px; background: var(--border); }
  .st:first-child::before { display: none; }
  .st.done i { border-color: var(--ok); background: var(--ok); }
  .st.now i { border-color: var(--accent); background: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent); }
  .st.ko i { border-color: var(--ko); background: var(--ko); }
  .st.done::before, .st.now::before, .st.ko::before { background: var(--ok); }
  .st.now, .st.ko { color: var(--text); font-weight: 600; }
  /* Bars only. */
  .mini { gap: 3px; align-items: center; }
  .mini .st { flex: 0 0 auto; }
  .mini .st::before { display: none; }
  .mini .st i { width: 16px; height: 5px; border: none; border-radius: 3px; background: var(--border); box-shadow: none; }
  .mini .st.done i { background: var(--ok); }
  .mini .st.now i { background: var(--accent); }
  .mini .st.ko i { background: var(--ko); }
</style>
