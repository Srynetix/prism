<script lang="ts">
  import { chainActive, chainOf, forgetChain, goHeld, skipHeld, stopChain, STEP_LABEL, WHEN_LABEL, type RunState } from '../agent-chains.svelte'
  import { openLink } from '../links'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'

  /**
   * An agent's chain, under its header: each step as a chip (done, running, held for you, failed,
   * skipped), Start or Skip for a held step, Stop while it runs, Hide once it is over. On a phone,
   * as the window shares it, its buttons asking the window.
   */
  let { agent }: { agent: Agent } = $props()
  const chain = $derived(chainOf(agent.id))
  const ICON: Record<RunState, IconName | null> = { pending: null, held: 'pause', running: 'running', done: 'check', failed: 'fail', skipped: 'none' }
  /** A step's chip: the tone of its state. */
  const TONE: Record<RunState, string> = { pending: '', held: 'warn', running: 'info', done: 'ok', failed: 'ko', skipped: 'skipped' }
  const short = (kind: string) => ({ task: t('Task'), prompt: t('Prompt'), mr: t('Draft MR'), agent: t('Review agent'), notify: t('Notify'), aside: t('In passing') })[kind] ?? kind
  const held = $derived(chain && chain.runs[chain.current]?.state === 'held')
</script>

{#if chain}
  <div class="chain" aria-label={t('Its chain')}>
    <span class="pill s" class:ok={chain.current >= 0} class:info={chain.current < 0 && chainActive(chain)} title={t('Its own work, from its prompt')}><Icon name={chain.current >= 0 ? 'check' : 'running'} size={10} /> {t('Work')}</span>
    {#each chain.steps as step, i (i)}
      {@const run = chain.runs[i]}
      <span class="arrow">→</span>
      <span class="pill s {TONE[run.state]}" title="{STEP_LABEL[step.kind] ?? step.kind}{step.command ? `: ${step.command}` : ''} · {WHEN_LABEL[step.when] ?? step.when}{run.detail ? ` · ${run.detail}` : ''}">
        {#if ICON[run.state]}<Icon name={ICON[run.state]!} size={10} />{/if} {i + 1} {short(step.kind)}{#if run.url}&nbsp;<button class="link" onclick={() => openLink(run.url!)}>{run.detail?.split(' → ')[0]}</button>{:else if run.detail && run.state !== 'running'}<small> · {run.detail}</small>{/if}
      </span>
    {/each}
    <span class="grow"></span>
    {#if held}
      <button class="btn sm" onclick={() => skipHeld(agent.id)}>{t('Skip')}</button>
      <button class="btn sm primary claude" onclick={() => goHeld(agent.id)}>{t('Start step {n}', { n: chain.current + 1 })}</button>
    {:else if chainActive(chain)}
      <button class="btn sm" onclick={() => stopChain(agent.id)} title={t('The steps left are skipped')}>{t('Stop the chain')}</button>
    {:else}
      <button class="btn sm" onclick={() => forgetChain(agent.id)} title={t('Hide this finished chain')}>{t('Hide')}</button>
    {/if}
  </div>
{/if}

<style>
  .chain { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; padding: 4px 12px 6px; font-size: var(--fs-tag); }
  .s { display: inline-flex; align-items: center; gap: 3px; }
  .s small { color: inherit; opacity: 0.85; }
  .s.skipped { opacity: 0.6; }
  .arrow { color: var(--text-muted); }
  .grow { flex: 1; }
  .link { padding: 0; border: none; background: none; color: inherit; font: inherit; text-decoration: underline; cursor: pointer; }
  :global(html.remote) .chain .btn { min-height: 32px; padding: 4px 12px; }
</style>
