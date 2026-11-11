<script lang="ts">
  import Icon from './Icon.svelte'
  import { CLIS } from '../agent-cli'
  import { anchored, dismissable, exclusive } from '../popup'
  import { continueWith, installedClis, targetName, targetOf, targetsOf, targetUse } from '../agent-switch.svelte'
  import { t } from '../i18n.svelte'
  import type { Target } from '../handoff'
  import type { Agent } from '../types'

  /**
   * An agent's CLI in its header (its account's name when it has one): a chip whose menu lists the
   * other CLIs found on the computer and the other accounts of each (`[[agents.accounts]]`), each
   * with its plan's use, to continue with (agent-switch.svelte.ts). A prepared agent's is read
   * only: its launch is changed in the launcher.
   */
  let { a }: { a: Agent } = $props()

  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  const found = $derived(open ? installedClis() : null)
  const others = $derived(targetsOf(a))
  const cli = $derived(CLIS[a.provider])
  const used = (t: Target) => targetUse(t)
  const here = $derived(targetOf(a))

  function toBody(node: HTMLElement) {
    document.body.appendChild(node)
    return () => node.remove()
  }
  function pick(target: Target) {
    open = false
    void continueWith(a, target)
  }
</script>

{#if a.state === 'prepared'}
  <span class="chip" style:--c={cli.color} title={cli.cli}><Icon name={cli.icon} />{targetName(here)}</span>
{:else}
  <button bind:this={button} class="chip" class:open style:--c={cli.color} onclick={(e) => { e.stopPropagation(); open = !open }} aria-haspopup="menu" aria-expanded={open} title={a.account ? t('{cli} runs it under {account}: continue with another account or CLI', { cli: cli.cli, account: a.account }) : t('{cli} runs it: continue with another CLI', { cli: cli.cli })}>
    <Icon name={cli.icon} />{targetName(here)}<Icon name="down" size={10} />
  </button>
  {#if open}
    <div class="menu" role="menu" tabindex="-1" {@attach toBody} {@attach anchored(() => button, { align: 'start' })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true })}>
      <div class="lbl">{t('Continue with')}</div>
      <div class="opt cur"><Icon name={cli.icon} /><span class="n">{a.account ?? cli.cli}<small>{t('now')}</small></span>{#if used(here) !== null}<span class="use" class:hot={used(here)! >= 90}>{Math.round(used(here)!)} %</span>{/if}<Icon name="check" size={11} /></div>
      <div class="sep"></div>
      {#each others as target (`${target.provider}:${target.account ?? ''}`)}
        {@const p = target.provider}
        {@const missing = !!found && !found.includes(p)}
        {@const same = p === a.provider}
        <button class="opt" role="menuitem" disabled={missing} onclick={() => pick(target)} title={missing ? t('{program} not found in the PATH', { program: CLIS[p].program }) : same ? t('{account} takes its conversation whole', { account: targetName(target) }) : t('{cli} takes it on, from a relay', { cli: CLIS[p].cli })}>
          <Icon name={CLIS[p].icon} />
          <span class="n">{target.account ?? CLIS[p].cli}{#if missing}<small>{t('not found')}</small>{:else if same}<small>{t('its conversation goes too')}</small>{:else if !CLIS[p].plan}<small>{t('your own API keys')}</small>{/if}</span>
          {#if used(target) !== null}<span class="use" class:hot={used(target)! >= 90}>{Math.round(used(target)!)} %</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .chip { display: inline-flex; align-items: center; gap: 5px; padding: 1px 8px; border: 1px solid color-mix(in srgb, var(--c) 55%, var(--border)); border-radius: var(--r-chip, 8px); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); white-space: nowrap; }
  button.chip { cursor: pointer; }
  button.chip:hover, .chip.open { border-color: var(--c); }
  .menu { position: fixed; z-index: 70; min-width: 250px; max-height: 380px; overflow: auto; padding: 5px; border: 1px solid var(--border); border-radius: var(--r6, 8px); background: var(--surface); box-shadow: var(--shadow-pop); }
  .lbl { padding: 4px 8px 3px; color: var(--text-muted); font-size: var(--fs-tag, 11px); text-transform: uppercase; letter-spacing: .05em; }
  .opt { display: flex; width: 100%; align-items: center; gap: 8px; padding: 6px 8px; border: none; border-radius: var(--r6, 6px); background: none; color: var(--text); font: inherit; text-align: left; }
  button.opt { cursor: pointer; }
  button.opt:hover:not(:disabled), button.opt:focus-visible { background: var(--surface-muted); }
  .opt:disabled { opacity: .45; cursor: default; }
  .opt.cur { color: var(--text-muted); }
  .n { flex: 1; display: flex; flex-direction: column; }
  .n small { color: var(--text-muted); font-size: var(--fs-tag, 11px); }
  .use { color: var(--text-muted); font-size: var(--fs-tag, 11px); }
  .use.hot { color: var(--ko); }
  .sep { height: 1px; margin: 4px 0; background: var(--border); }
</style>
