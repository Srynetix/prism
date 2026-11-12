<script lang="ts">
  import { onMount } from 'svelte'
  import { ago, BlockControls, call, Icon, isCollapsed, locale, t, toast } from '@prism/sdk'
  import type { Snapshot } from '../../../../web/src/types'

  /** What its back end publishes (`backend/`, a WebAssembly component); none without one. */
  interface State {
    reads: number
    at: number
    commit: string | null
    project: string | null
  }

  let { state, error, snap }: { state: State | undefined; error?: string; snap: Snapshot } = $props()

  let now = $state(new Date())
  let failing = $state(false)
  onMount(() => {
    const timer = setInterval(() => (now = new Date()), 1000)
    return () => clearInterval(timer)
  })
  const agents = $derived(snap.agents?.length ?? 0)
  const failingSources = $derived(Object.keys(snap.errors ?? {}).length)
  const time = $derived.by(() => {
    // A render that throws: the block's boundary catches it.
    if (failing) throw new Error(t('This block failed on purpose.'))
    return now.toLocaleTimeString(locale())
  })
</script>

<section class="panel" class:collapsed={isCollapsed('hello')}>
  <h2>
    <span class="title"><Icon name="star" />{t('Hello from a module')}</span>
    <BlockControls id="hello" />
  </h2>
  <div class="body">
    <p>{t('{n} agents, {m} sources failing', { n: agents, m: failingSources })}</p>
    <p class="time">{t('It is {time}.', { time })}</p>
    {#if state}
      <p>{t('Its back end read {n} times, {when}.', { n: state.reads, when: ago(new Date(state.at).toISOString()) })}</p>
      {#if state.commit}<p class="muted">{t('Last commit of {project}: {commit}', { project: state.project ?? '—', commit: state.commit })}</p>{/if}
    {:else}
      <p class="muted">{t('No back end: build it with just module-example.')}</p>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
    <div class="row">
      <button class="btn" onclick={() => (failing = true)}>{t('Fail')}</button>
      {#if state}
        <button class="btn" onclick={() => call<{ pong: number }>('hello', 'ping').then((r) => toast(t('Pong: {n} reads', { n: r.pong }))).catch((e) => toast(String(e)))}>Ping</button>
        <button class="btn" onclick={() => call('hello', 'panic').catch((e) => toast(String(e)))}>{t('Crash the back end')}</button>
      {/if}
    </div>
  </div>
</section>

<style>
  .body { display: flex; flex-direction: column; gap: 6px; padding: 10px 14px; align-items: flex-start; }
  .body p { margin: 0; }
  .time { font-variant-numeric: tabular-nums; color: var(--text-muted); }
  .muted { color: var(--text-muted); }
  .error { color: var(--ko); overflow-wrap: anywhere; }
  .row { display: flex; gap: 6px; flex-wrap: wrap; }
</style>
