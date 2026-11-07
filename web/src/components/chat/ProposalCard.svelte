<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t, tn } from '../../i18n.svelte'
  import { armer } from '../../armed.svelte'
  import { isDesktop } from '../../bridge'
  import { answer, counts, openProposal, title } from '../../mcp-view.svelte'
  import type { Proposal } from '../../bindings/Proposal'
  import type { ConversationTool } from '../../bindings/ConversationTool'

  /**
   * A change of Prism the agent proposed (`prism_propose_config`, `prism_propose_layout`,
   * `prism_create_custom_block`, through `prism mcp`), in its Chat, in place of its call: while it
   * waits for you, its reason, what it changes and what the check says, with See the diff (the
   * drawer), Refuse… (a reason the agent gets) and Apply (two clicks); once answered, what became
   * of it. On a phone, read only: the answer is given on the computer.
   */
  let { call, proposal }: {
    call: ConversationTool
    /** The proposal it waits for (`Proposal.agent`, oldest call ↔ oldest proposal); none yet. */
    proposal: Proposal | undefined
  } = $props()

  /** Its answer, as the call's result says it (`fold::propose_answer`). */
  const done = $derived.by(() => {
    if (call.state === 'running' || !call.output) return null
    try {
      return JSON.parse(call.output) as { status: string; reason?: string }
    } catch {
      return null
    }
  })
  const arm = armer(4000)
  let refusing = $state(false)
  let reason = $state('')
  let busy = $state(false)
  let error = $state('')
  const desk = isDesktop()

  async function act(action: 'apply' | 'refuse') {
    if (!proposal) return
    if (action === 'apply' && !arm.confirm('apply')) return
    busy = true
    error = ''
    try {
      await answer(proposal.id, action, { reason: action === 'refuse' ? reason.trim() : undefined })
      refusing = false
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

{#if done}
  <div class="line" class:ok={done.status !== 'refused'} class:ko={done.status === 'refused'}>
    <Icon name={done.status === 'refused' ? 'close' : 'check'} size={12} />
    {#if done.status === 'refused'}{t('Proposal refused')}{#if done.reason}<span class="why">· « {done.reason} »</span>{/if}
    {:else if done.status === 'applied_modified'}{t('Proposal edited, then applied')}
    {:else}{t('Proposal applied')}{/if}
  </div>
{:else if proposal}
  {@const c = counts(proposal)}
  <div class="card">
    <div class="head">
      <Icon name={proposal.kind === 'custom_block' ? 'blockNormal' : 'settings'} size={13} />
      <b>{title(proposal)}</b>
      <span class="pill">{t('waits for you')}</span>
    </div>
    {#if proposal.why}<p class="why-q">« {proposal.why} »</p>{/if}
    <p class="sum">
      <span class="add">+{c.add}</span>{#if c.del}<span class="del">−{c.del}</span>{/if}
      {#if proposal.touches.length}<span>· {t('touches: {what}', { what: proposal.touches.join(', ') })}</span>{/if}
    </p>
    {#if proposal.stale}<p class="warn"><Icon name="warning" size={11} /> {t('config.toml changed since: computed again on the current version')}</p>{/if}
    {#if proposal.overrides.length}<p class="warn"><Icon name="warning" size={11} /> {tn(proposal.overrides.length, 'Overrides the value a pack gives: {keys} (written in config.toml, never in the pack).', 'Overrides the values a pack gives: {keys} (written in config.toml, never in the pack).', { keys: proposal.overrides.join(', ') })}</p>{/if}
    {#each proposal.warnings as w (w)}<p class="warn"><Icon name="warning" size={11} /> {w}</p>{/each}
    {#if error}<p class="err">{error}</p>{/if}
    {#if !desk}
      <p class="muted">{t('To answer on the computer')}</p>
    {:else if refusing}
      <div class="refuse">
        <!-- svelte-ignore a11y_autofocus -->
        <input bind:value={reason} placeholder={t('Why (sent to the agent): not this repository, rather…')} onkeydown={(e) => { if (e.key === 'Enter') void act('refuse'); if (e.key === 'Escape') refusing = false }} autofocus />
        <button disabled={busy} onclick={() => act('refuse')}>{t('Refuse')}</button>
        <button class="ghost" onclick={() => (refusing = false)}>{t('Cancel')}</button>
      </div>
    {:else}
      <div class="acts">
        <button class="ghost" onclick={() => openProposal(proposal.id)}><Icon name="diff" size={12} /> {t('See the diff')}</button>
        <span class="grow"></span>
        <button disabled={busy} onclick={() => (refusing = true)}>{t('Refuse…')}</button>
        <button class="primary" class:armed={arm.is('apply')} disabled={busy} onclick={() => act('apply')}>
          {#if arm.is('apply')}{proposal.kind === 'custom_block' ? t('Trust and add?') : t('Apply?')}{:else if proposal.kind === 'custom_block'}{t('Trust and add')}{:else if proposal.stale}{t('Apply on the current version')}{:else}{t('Apply')}{/if}
        </button>
      </div>
    {/if}
  </div>
{:else}
  <!-- Waiting, its proposal not in sight (being checked, or answered a moment ago). -->
  <div class="line"><Icon name="settings" size={12} /> {t('A proposal waits for your answer')}{#if desk}<button class="link" onclick={() => openProposal()}>{t('Open')}</button>{/if}</div>
{/if}

<style>
  .card { margin: 6px 0; padding: 9px 12px; border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border)); border-radius: var(--r8); background: color-mix(in srgb, var(--accent) 6%, var(--surface)); font-size: var(--fs-body); }
  .head { display: flex; align-items: center; gap: 6px; color: var(--accent); }
  .head b { color: var(--text); font-weight: 600; }
  .pill { margin-left: auto; padding: 0 7px; border-radius: var(--r9); background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); font-size: var(--fs-tag); font-weight: 600; }
  .why-q { margin: 6px 0 4px; font-style: italic; }
  .sum { display: flex; flex-wrap: wrap; gap: 6px; margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .add { color: var(--ok); font-weight: 600; }
  .del { color: var(--ko); font-weight: 600; }
  .warn { display: flex; align-items: center; gap: 5px; margin: 4px 0 0; color: var(--warn); font-size: var(--fs-meta); }
  .err { margin: 4px 0 0; color: var(--ko); font-size: var(--fs-meta); }
  .muted { margin: 6px 0 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .acts, .refuse { display: flex; align-items: center; gap: 6px; margin-top: 8px; }
  .grow { flex: 1; }
  .refuse input { flex: 1; min-width: 0; font: inherit; font-size: var(--fs-meta); padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); }
  button { font: inherit; font-size: var(--fs-meta); padding: 3px 11px; border-radius: var(--r6); border: 1px solid var(--border); background: var(--surface); color: var(--text); cursor: pointer; display: inline-flex; align-items: center; gap: 5px; }
  button:hover:not(:disabled) { background: var(--surface-muted); }
  button:disabled { opacity: 0.5; cursor: default; }
  button.ghost { border-color: transparent; background: none; }
  button.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, white); }
  button.primary.armed { background: var(--warn-bg); border-color: var(--warn); color: var(--warn); font-weight: 600; }
  button.link { border: none; background: none; padding: 0 0 0 6px; color: var(--accent); }
  .line { display: flex; align-items: center; gap: 6px; margin: 4px 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .line.ok { color: var(--ok); }
  .line.ko { color: var(--ko); }
  .line .why { color: var(--text-muted); font-style: italic; }
</style>
