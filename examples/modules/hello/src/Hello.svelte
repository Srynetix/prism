<script lang="ts">
  import { onMount } from 'svelte'
  import { BlockControls, Icon, isCollapsed, locale, t } from '@prism/sdk'
  import type { Snapshot } from '../../../../web/src/types'

  let { snap }: { snap: Snapshot } = $props()

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
    <button class="btn" onclick={() => (failing = true)}>{t('Fail')}</button>
  </div>
</section>

<style>
  .body { display: flex; flex-direction: column; gap: 6px; padding: 10px 14px; align-items: flex-start; }
  .body p { margin: 0; }
  .time { font-variant-numeric: tabular-nums; color: var(--text-muted); }
</style>
