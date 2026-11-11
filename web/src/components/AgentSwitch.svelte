<script lang="ts">
  import Icon from './Icon.svelte'
  import { live } from '../live.svelte'
  import { CLIS, nameOf } from '../agent-cli'
  import { isWorking } from '../rules'
  import { handoffs, tokensOf, type Handoff, type Include, type Target } from '../handoff'
  import { askSummary, closeSwitch, compose, confirmSwitch, installedClis, pickHandoff, pickTarget, sameCli, summaryLanded, switching, targetName, targetsOf, targetUse } from '../agent-switch.svelte'
  import { choices, effortsKey, loadEfforts, loadModels } from '../settings.svelte'
  import { suggest } from '../suggest'
  import { agentsSettings } from '../agents.svelte'
  import { t, tn } from '../i18n.svelte'
  import type { AgentProvider } from '../types'

  /**
   * Continue with… (agent-switch.svelte.ts): the CLI (or account of one) the agent continues with,
   * how it is told what was done — its whole conversation to another account of its CLI, Prism's
   * relay (its task, plan, last turns, branch), the former agent's own summary, or the task only —
   * the relay itself, editable, and the new CLI's model, effort and mode. `phone`: the remote's,
   * full screen.
   */
  let { phone = false }: { phone?: boolean } = $props()

  const agent = $derived(live.snapshot?.agents?.find((a) => a.id === switching.id))
  const found = $derived(installedClis())
  const targets = $derived(agent ? targetsOf(agent) : [])
  const readable = $derived(switching.material?.readable ?? true)
  const same = $derived(sameCli())
  const offered = $derived(handoffs(readable, same))
  /** The target picked, as named (its account's name, or its CLI's). */
  const toName = $derived(switching.to ? targetName({ provider: switching.to, account: switching.account }) : '')
  /** The relays shown: its whole conversation first for another account of its CLI. */
  const relays = $derived<Handoff[]>(same ? ['resume', 'prism', 'agent', 'task'] : ['prism', 'agent', 'task'])
  const picked = (t: Target) => switching.to === t.provider && switching.account === t.account
  const running = $derived(agent?.state === 'running')
  const working = $derived(!!agent && running && isWorking(agent.status))
  const to = $derived(switching.to)
  const takesEffort = $derived(!!to && CLIS[to].takes_effort)
  const efforts = $derived(to ? (choices.efforts[effortsKey(to, CLIS[to].efforts_by_model ? switching.model || null : null)] ?? []) : [])
  const cost = $derived(agent?.usage?.cost)

  $effect(() => {
    if (to && switching.open) {
      void loadModels(to)
      void loadEfforts(to, CLIS[to].efforts_by_model ? switching.model || null : null)
    }
  })
  // Its summary asked: read once its turn is over.
  $effect(() => {
    if (agent && switching.asked) void summaryLanded(agent)
  })

  const HANDOFF: Record<Handoff, () => { title: string; hint: string; cost: string }> = {
    resume: () => ({ title: t('Its whole conversation'), hint: t('Copied to {account}’s folder and resumed there: nothing lost, no relay to write. Its next prompt says it changed account.', { account: toName }), cost: t('free · at once') }),
    prism: () => ({ title: t('Relay written by Prism'), hint: t('From its transcript and git, no model called: its task, plan, last turns, the files changed, the ticket. Works even at its limit.'), cost: t('free · at once') }),
    agent: () => ({ title: t('Summary written by {cli}', { cli: agent ? nameOf(agent) : '' }), hint: t('Prism asks it for a handoff note, then gives it to the next one. The best summary, but it has to answer (not at its limit).'), cost: t('one turn of {cli}', { cli: agent ? nameOf(agent) : '' }) }),
    task: () => ({ title: t('The task only'), hint: t('Its first prompt and the ticket; the next one reads the code itself (git diff).'), cost: t('minimal') }),
  }
  /** Why a relay cannot be picked, if it cannot. */
  function blocked(h: Handoff): string | null {
    if (h === 'resume' && !same) return t('for another account of the same CLI only')
    if (!offered.includes(h)) return t('{cli} keeps no conversation Prism reads', { cli: agent ? nameOf(agent) : '' })
    if (h === 'agent' && !running) return t('it has stopped: Resume it first')
    if (h === 'agent' && switching.reason === 'limit') return t('at its limit, it cannot answer')
    return null
  }
  const INCLUDE: [keyof Include, () => string][] = [
    ['prompt', () => t('First prompt')],
    ['plan', () => t('Plan')],
    ['turns', () => tn(agentsSettings().handoff_turns ?? 8, 'last turn', '{n} last turns')],
    ['files', () => t('Files changed')],
    ['commits', () => t('Commits')],
  ]
  function toggle(k: keyof Include) {
    switching.include[k] = !switching.include[k]
    switching.edited = false
    compose()
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      if (!switching.busy) closeSwitch()
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      void confirmSwitch()
    }
  }
  const usage = (target: Target) => {
    const u = targetUse(target)
    return u === null ? '' : `${Math.round(u)} %`
  }
</script>

{#if switching.open && agent}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions : Escape closes from the keyboard -->
  <div class="scrim" class:phone onclick={(e) => e.target === e.currentTarget && !switching.busy && closeSwitch()}>
    <div class="sheet" role="dialog" aria-modal="true" aria-label={t('Continue with…')} tabindex="-1" {onkeydown}>
      <header>
        <Icon name="retry" />
        <b>{to ? t('Continue with {cli}', { cli: switching.account ?? CLIS[to].cli }) : t('Continue with…')}</b>
        <button class="x" onclick={closeSwitch} aria-label={t('Close')} disabled={switching.busy}><Icon name="close" /></button>
      </header>

      <div class="body">
        <p class="sub">
          {t('Same agent, folder, branch and links: {from} stops, the next CLI starts there from the relay below.', { from: nameOf(agent) })}
          {#if working}<b class="warn">{t('It is working: its turn is cut short.')}</b>{/if}
        </p>

        <div class="relay">
          <span class="box"><Icon name={CLIS[agent.provider].icon} />{nameOf(agent)}{#if agent.usage}<small>{tn(agent.usage.replies, '{n} reply', '{n} replies')}{#if cost != null} · ${cost.toFixed(2)}{/if}</small>{/if}</span>
          <span class="arrow">→</span>
          <div class="targets" role="radiogroup" aria-label={t('Continue with')}>
            {#each targets as target (`${target.provider}:${target.account ?? ''}`)}
              {@const p = target.provider}
              {@const missing = !!found && !found.includes(p)}
              <button class="cli" class:on={picked(target)} role="radio" aria-checked={picked(target)} disabled={missing} onclick={() => pickTarget(target)} title={missing ? t('{program} not found in the PATH', { program: CLIS[p].program }) : target.account ? `${CLIS[p].cli} · ${target.account}` : CLIS[p].cli}>
                <Icon name={CLIS[p].icon} />{targetName(target)}{#if usage(target)}<small class:hot={(targetUse(target) ?? 0) >= 90}>{usage(target)}</small>{/if}
              </button>
            {/each}
          </div>
        </div>

        <div class="lab">{t('What it is told')}</div>
        <div class="choices">
          {#each relays as h (h)}
            {@const why = blocked(h)}
            {@const d = HANDOFF[h]()}
            <button class="choice" class:on={switching.handoff === h} disabled={!!why} onclick={() => pickHandoff(h)} role="radio" aria-checked={switching.handoff === h}>
              <i></i>
              <span class="what"><b>{d.title}</b><small>{why ?? d.hint}</small></span>
              <span class="cost">{d.cost}</span>
            </button>
          {/each}
        </div>

        {#if switching.handoff === 'prism'}
          <div class="incl">
            {#each INCLUDE as [k, label] (k)}
              <button class:off={!switching.include[k]} onclick={() => toggle(k)} aria-pressed={switching.include[k]}><Icon name={switching.include[k] ? 'check' : 'close'} size={10} />{label()}</button>
            {/each}
          </div>
        {:else if switching.handoff === 'agent'}
          <div class="ask">
            {#if switching.asked}
              <span class="spin-ascii" aria-hidden="true"></span><span>{t('{cli} writes its summary…', { cli: nameOf(agent) })}</span>
            {:else}
              <span>{switching.material?.last_reply && switching.text ? t('Its last answer is below: ask again for a fresh summary.') : t('Ask it for its summary: its answer becomes the relay.')}</span>
              <button class="btn" onclick={askSummary} disabled={!running}>{t('Ask {cli}', { cli: nameOf(agent) })}</button>
            {/if}
          </div>
        {/if}

        <div class="lab">{switching.handoff === 'resume' ? t('Its next prompt, to {cli}', { cli: toName || '…' }) : t('Relay sent to {cli}', { cli: toName || '…' })}{#if switching.text}<span class="muted"> · {t('~{n} tokens', { n: tokensOf(switching.text).toLocaleString() })}</span>{/if}{#if switching.edited}<button class="link" onclick={() => ((switching.edited = false), compose())}>{t('Write it again')}</button>{/if}</div>
        {#if !switching.material && !switching.error}
          <p class="muted">{t('Reading its conversation and its branch…')}</p>
        {:else}
          <textarea bind:value={switching.text} oninput={() => (switching.edited = true)} spellcheck="false" rows={phone ? 12 : 14}></textarea>
        {/if}

        {#if to}
          <div class="opts">
            <label><span>{t('Model')}</span><input bind:value={switching.model} placeholder={agentsSettings()[to]?.model ?? t('{cli}’s default', { cli: CLIS[to].name })} spellcheck="false" autocapitalize="off" {@attach suggest(() => choices.models[to] ?? [])} /></label>
            {#if takesEffort}
              <label><span>{t('Effort')}</span>
                <select bind:value={switching.effort}>
                  <option value="">{agentsSettings()[to]?.effort ?? t('{cli}’s default', { cli: CLIS[to].name })}</option>
                  {#each efforts as e (e)}<option value={e}>{e}</option>{/each}
                </select>
              </label>
            {/if}
            <label><span>{t('Mode')}</span>
              <select bind:value={switching.mode}>
                <option value="">{t('as in Settings')}</option>
                <option value="default">{t('Ask each edit')}</option>
                <option value="acceptEdits">{t('Accept edits')}</option>
                <option value="plan">{t('Plan')}</option>
              </select>
            </label>
          </div>
        {/if}
        {#if switching.error}<p class="error">{switching.error}</p>{/if}
      </div>

      <footer>
        <span class="note">{t('{cli} can be taken back: its Activity tab, Back to {cli}.', { cli: nameOf(agent) })}</span>
        <button class="btn quiet" onclick={closeSwitch} disabled={switching.busy}>{t('Cancel')}</button>
        <button class="btn primary" onclick={confirmSwitch} disabled={switching.busy || !to || !switching.text.trim() || !!switching.asked}>{switching.busy ? t('Handing over…') : to ? t('Continue with {cli}', { cli: toName }) : t('Continue')}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; z-index: 66; display: flex; align-items: flex-start; justify-content: center; padding: 6vh 16px 16px; background: color-mix(in srgb, var(--bg) 45%, transparent); backdrop-filter: blur(2px); }
  .scrim.phone { padding: 0; align-items: stretch; }
  .sheet { width: min(720px, 100%); max-height: 88vh; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--r12, 12px); background: var(--surface); box-shadow: var(--shadow-modal, var(--shadow-pop)); outline: none; overflow: hidden; }
  .phone .sheet { max-height: none; border-radius: 0; }
  header { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); color: var(--text-muted); }
  header b { flex: 1; color: var(--text); font-size: var(--fs-title, 15px); }
  .x { display: inline-flex; padding: 3px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .body { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px; overflow: auto; }
  .sub { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .sub .warn { display: block; margin-top: 3px; color: var(--warn); font-weight: 500; }
  .relay { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .box { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); }
  .box small, .cli small { color: var(--text-muted); font-size: var(--fs-tag, 11px); }
  .cli small.hot { color: var(--ko); }
  .arrow { color: var(--text-muted); }
  .targets { display: flex; gap: 4px; flex-wrap: wrap; }
  .cli { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  .cli.on { border-color: var(--accent); color: var(--accent); }
  .cli:disabled { opacity: .45; cursor: default; }
  .lab { display: flex; align-items: baseline; gap: 4px; color: var(--text-muted); font-size: var(--fs-meta); }
  .lab .link { margin-left: auto; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; padding: 0; }
  .choices { display: flex; flex-direction: column; gap: 5px; }
  .choice { display: flex; align-items: flex-start; gap: 9px; padding: 7px 10px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .choice i { flex: none; width: 13px; height: 13px; margin-top: 2px; border: 2px solid var(--border); border-radius: 50%; }
  .choice.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 7%, var(--bg)); }
  .choice.on i { border-color: var(--accent); background: radial-gradient(var(--accent) 40%, transparent 46%); }
  .choice:disabled { opacity: .5; cursor: default; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .what small { color: var(--text-muted); font-size: var(--fs-meta); }
  .cost { flex: none; color: var(--text-muted); font-size: var(--fs-tag, 11px); white-space: nowrap; }
  .incl { display: flex; gap: 5px; flex-wrap: wrap; }
  .incl button { display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .incl button.off { color: var(--text-muted); text-decoration: line-through; }
  .ask { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border: 1px dashed var(--border); border-radius: var(--r6, 6px); font-size: var(--fs-meta); }
  .ask > span:not(.spin-ascii) { flex: 1; }
  textarea { width: 100%; min-height: 200px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); color: var(--text); font-family: var(--mono); font-size: var(--fs-meta); line-height: 1.5; resize: vertical; }
  .opts { display: flex; gap: 10px; flex-wrap: wrap; }
  .opts label { display: flex; flex-direction: column; gap: 3px; min-width: 140px; flex: 1; color: var(--text-muted); font-size: var(--fs-meta); }
  .opts input, .opts select { padding: 4px 7px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); color: var(--text); font: inherit; }
  .error { margin: 0; color: var(--ko); font-size: var(--fs-meta); }
  .muted { color: var(--text-muted); }
  p.muted { margin: 0; font-size: var(--fs-meta); }
  footer { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-top: 1px solid var(--border); }
  footer .note { flex: 1; color: var(--text-muted); font-size: var(--fs-tag, 11px); }
  .btn { padding: 5px 12px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; white-space: nowrap; }
  .btn.primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent); font-weight: 600; }
  .btn.quiet { background: none; }
  .btn:disabled { opacity: .5; cursor: default; }
  .phone footer { flex-wrap: wrap; }
  .phone footer .note { flex-basis: 100%; }
</style>
