<script lang="ts">
  import Icon from './Icon.svelte'
  import { invoke } from '../bridge'
  import { armer } from '../armed.svelte'
  import { cliOf } from '../agent-cli'
  import { tildePath, baseName } from '../platform'
  import { t } from '../i18n.svelte'
  import type { Agent } from '../types'

  /**
   * What an agent's CLI asks before its session starts (`Agent.gate`): to trust its folder, or
   * (Codex) to approve Prism's hooks. Until it is answered no hook comes: no status, no Chat. The
   * card says which folder and what trusting it lets run, and answers it — Trust (with « All
   * worktrees of … » when its CLI trusts a folder for those inside it) or Refuse (two clicks: the
   * CLI exits). A screen Prism does not know for sure is answered in the terminal only: nothing is
   * sent blind. On a phone, Trust asks for the passkey (`critical`).
   */
  let { agent, onterminal, phone = false, critical = true }: { agent: Agent; onterminal?: () => void; phone?: boolean; critical?: boolean } = $props()

  const gate = $derived(agent.gate!)
  const cli = $derived(cliOf(agent.provider).name)
  const folder = $derived(gate.folder ?? agent.worktree ?? agent.cwd)
  let all = $state(false)
  let busy = $state(false)
  let error = $state('')
  const arm = armer(4000)

  async function run(cmd: string, args: Record<string, unknown>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await invoke(cmd, { id: agent.id, kind: gate.kind, ...args })
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  const trust = () => run('agent_trust', { parent: all && !!gate.parent })
  const refuse = () => arm.confirm('refuse') && run('agent_refuse_gate', {})
</script>

<div class="trust" class:phone role="group" aria-label={t('What {cli} asks before it starts', { cli })}>
  {#if gate.kind === 'folder'}
    <b><Icon name="shield" /> {phone ? t('Trust this folder?') : t('{cli} asks to trust this folder', { cli })}</b>
    <code title={folder}>{tildePath(folder)}</code>
    {#if agent.provider === 'claude'}
      <p>{t('It will read, edit and run commands here (in the mode chosen); the folder’s settings (.claude/), hooks and MCP servers will run.')}</p>
    {:else}
      <p>{t('It will work in this folder and read its AGENTS.md.')}</p>
    {/if}
  {:else if gate.kind === 'hooks'}
    <b><Icon name="shield" /> {t('{cli} asks to approve Prism’s hooks', { cli })}</b>
    <p>{t('Without them Prism knows nothing of this agent: no status, no Chat, no context. They call Prism, which writes what happens in its log.')}</p>
  {:else}
    <b><Icon name="shield" /> {t('{cli} waits for an answer in its terminal', { cli })}</b>
  {/if}
  {#if gate.answerable}
    {#if gate.parent}
      <div class="scope" role="radiogroup">
        <button role="radio" aria-checked={!all} class:on={!all} onclick={() => (all = false)}>{t('This folder only')}</button>
        <button role="radio" aria-checked={all} class:on={all} onclick={() => (all = true)} title={t('Trusts {folder}: the worktrees made there next are not asked', { folder: tildePath(gate.parent) })}>{t('All worktrees of {repo}', { repo: baseName(gate.parent).replace(/\.worktrees$/, '') })}</button>
      </div>
    {/if}
    <div class="pr">
      <button class="yes" disabled={busy || !critical} onclick={trust} title={critical ? '' : t('Trusting a folder from a phone: critical actions on (Settings › Remote › Exposure)')}>{#if phone}<Icon name="key" />{/if}{gate.kind === 'hooks' ? t('Approve') : t('Trust')}</button>
      <button class:armed={arm.is('refuse')} disabled={busy} onclick={refuse} title={t('{cli} exits', { cli })}>{arm.is('refuse') ? t('Again: {cli} exits', { cli }) : t('Refuse')}</button>
      {#if onterminal && !phone}<button class="term" onclick={onterminal}>{t('Open the terminal')}</button>{/if}
    </div>
  {:else}
    <p class="muted">{t('Prism does not know this screen for sure (another version of {cli}?): it sends nothing. Answer it in its terminal.', { cli })}</p>
    {#if onterminal}<div class="pr"><button class="yes" onclick={onterminal}>{t('Open the terminal')}</button></div>{/if}
  {/if}
  {#if error}<p class="err">{error}</p>{/if}
</div>

<style>
  .trust { display: flex; flex-direction: column; gap: 7px; padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--violet, #8b5cf6) 45%, var(--border)); border-radius: var(--r10, 10px); background: color-mix(in srgb, var(--violet, #8b5cf6) 9%, var(--surface)); font-size: 12.5px; }
  b { display: inline-flex; align-items: center; gap: 6px; color: var(--violet, #8b5cf6); }
  code { padding: 5px 8px; border-radius: 7px; background: var(--surface); color: var(--text); font: 12px var(--mono); overflow-wrap: anywhere; }
  p { margin: 0; color: var(--text-muted); line-height: 1.4; }
  .scope { display: flex; flex-wrap: wrap; gap: 6px; }
  .scope button { padding: 3px 10px; border: 1px solid var(--border); border-radius: 12px; background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .scope button.on { border-color: var(--violet, #8b5cf6); color: var(--text); font-weight: 600; }
  .pr { display: flex; flex-wrap: wrap; gap: 6px; }
  .pr button { display: inline-flex; align-items: center; gap: 5px; padding: 4px 12px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  .pr .yes { border-color: var(--violet, #8b5cf6); background: var(--violet, #8b5cf6); color: #fff; font-weight: 600; }
  .pr button.armed { border-color: var(--ko); color: var(--ko); }
  .pr button:disabled { opacity: 0.5; cursor: default; }
  .pr .term { margin-left: auto; }
  .muted { color: var(--text-muted); }
  .err { color: var(--ko); overflow-wrap: anywhere; }
  /* On a phone: a card made for a thumb, as its permission's. */
  .trust.phone { gap: 9px; padding: 12px 14px; border-radius: 14px; font-size: 14px; }
  .trust.phone .scope button { min-height: 36px; padding: 0 14px; border-radius: 18px; font-size: 14px; }
  .trust.phone .pr button { flex: 1; justify-content: center; min-height: 42px; border-radius: 21px; font-size: 14.5px; }
</style>
