<script lang="ts">
  import { chainActive, chainOf, forgetChain, goHeld, skipHeld, stopChain, STEP_LABEL, WHEN_LABEL, type RunState } from '../agent-chains.svelte'
  import { openLink } from '../links'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * An agent's chain, under its header: each step as a chip (done, running, held for you, failed,
   * skipped), Start or Skip for a held step, Stop while it runs, Hide once it is over. On a phone,
   * as the window shares it, its buttons asking the window.
   */
  let { agent }: { agent: Agent } = $props()
  const chain = $derived(chainOf(agent.id))
  const ICON: Record<RunState, string> = { pending: '', held: '⏸', running: '◌', done: '✓', failed: '✕', skipped: '–' }
  const short = (kind: string) => ({ task: t('Task'), prompt: t('Prompt'), mr: t('Draft MR'), agent: t('Review agent'), notify: t('Notify') })[kind] ?? kind
  const held = $derived(chain && chain.runs[chain.current]?.state === 'held')
</script>

{#if chain}
  <div class="chain" aria-label={t('Its chain')}>
    <span class="s" class:done={chain.current >= 0} class:now={chain.current < 0 && chainActive(chain)} title={t('Its own work, from its prompt')}>{chain.current >= 0 ? '✓' : '◌'} {t('Work')}</span>
    {#each chain.steps as step, i (i)}
      {@const run = chain.runs[i]}
      <span class="arrow">→</span>
      <span class="s {run.state}" title="{STEP_LABEL[step.kind] ?? step.kind}{step.command ? `: ${step.command}` : ''} · {WHEN_LABEL[step.when] ?? step.when}{run.detail ? ` · ${run.detail}` : ''}">
        {ICON[run.state]} {i + 1} {short(step.kind)}{#if run.url}&nbsp;<button class="link" onclick={() => openLink(run.url!)}>{run.detail?.split(' → ')[0]}</button>{:else if run.detail && run.state !== 'running'}<small> · {run.detail}</small>{/if}
      </span>
    {/each}
    <span class="grow"></span>
    {#if held}
      <button onclick={() => skipHeld(agent.id)}>{t('Skip')}</button>
      <button class="go" onclick={() => goHeld(agent.id)}>{t('Start step {n}', { n: chain.current + 1 })}</button>
    {:else if chainActive(chain)}
      <button onclick={() => stopChain(agent.id)} title={t('The steps left are skipped')}>{t('Stop the chain')}</button>
    {:else}
      <button onclick={() => forgetChain(agent.id)} title={t('Hide this finished chain')}>{t('Hide')}</button>
    {/if}
  </div>
{/if}

<style>
  .chain { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; padding: 4px 12px 6px; font-size: var(--fs-tag); }
  .s { display: inline-flex; align-items: center; gap: 3px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r9); color: var(--text-muted); white-space: nowrap; }
  .s small { color: inherit; opacity: 0.85; }
  .s.done { border-color: color-mix(in srgb, var(--ok) 40%, var(--border)); color: var(--ok); }
  .s.running, .s.now { border-color: var(--info); color: var(--info); background: color-mix(in srgb, var(--info) 10%, transparent); }
  .s.held { border-color: var(--warn); color: var(--warn); }
  .s.failed { border-color: color-mix(in srgb, var(--ko) 50%, var(--border)); color: var(--ko); }
  .s.skipped { opacity: 0.6; }
  .arrow { color: var(--text-muted); }
  .grow { flex: 1; }
  .link { padding: 0; border: none; background: none; color: inherit; font: inherit; text-decoration: underline; cursor: pointer; }
  button:not(.link) { padding: 1px 9px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  :global(html.remote) .chain button:not(.link) { min-height: 32px; padding: 4px 12px; }
  button.go { border-color: transparent; background: color-mix(in srgb, var(--src-claude) 85%, #000); color: #fff; }
</style>
