<script lang="ts">
  import Icon from './Icon.svelte'
  import { agentsView, launchAgain, openAgents, openLauncher, spawnAgent } from '../agents.svelte'
  import { STEP_LABEL, WHEN_LABEL, type Step } from '../agent-chains.svelte'
  import { cliOf } from '../agent-cli'
  import { primaryRepo } from '../repositories'
  import { live } from '../live.svelte'
  import { tildePath } from '../platform'
  import { copy, toast } from '../toast.svelte'
  import { ago } from '../time'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * The Launch tab: what the agent was launched with, as it was then (a title, kind or tags
   * changed since are flagged, not rewritten), and Launch again: a new agent, a new conversation,
   * with the same; the launcher opens prefilled, ⇧-click launches at once. Restart and Resume stay
   * what picks the same conversation back up.
   */
  let { agent }: { agent: Agent } = $props()

  const l = $derived(agent.launch)
  const MODE: Record<string, string> = { acceptEdits: t('Accept edits'), plan: t('Plan first'), auto: t('Auto'), bypassPermissions: t('Bypass permissions') }
  let full = $state(false)
  const lines = $derived(l?.prompt?.split('\n').length ?? 0)
  const sameTags = $derived(!!l && l.tags.length === agent.tags.length && l.tags.every((tag) => agent.tags.includes(tag)))

  async function again(e: MouseEvent) {
    const launch = launchAgain(agent)
    if (!e.shiftKey) {
      openLauncher(launch)
      agentsView.sameAs = agent.title
      return
    }
    try {
      const id = await spawnAgent(launch)
      openAgents(id)
    } catch (err) {
      toast(String(err), 6000)
    }
  }
</script>

<div class="launch">
  {#if l}
    <div class="sec">
      <span class="cap">{t('Launched')} <span class="when" title={new Date(agent.created_at).toLocaleString()}>{t('{when} ago', { when: ago(agent.created_at) })}</span></span>
      <div class="kv">
        {#if l.from}<span>{t('From')}</span><span>{t('Action')} <b>{l.from}</b>{#if agent.ticket}{' '}· {agent.ticket}{/if}</span>{/if}
        <span>{t('Title')}</span><span>{l.title || t('Agent')}{#if l.title !== agent.title}<i class="changed"> · {t('renamed since')}</i>{/if}</span>
        <span>{t('Agent')}</span><span>{cliOf(l.provider ?? agent.provider).cli}<i class="muted"> · {l.provider ? t('picked') : t('the default then')}</i></span>
        <span>{t('Where')}</span>
        <span>
          {#if l.worktree}{t('New worktree in')} <b>{l.worktree.repo ?? primaryRepo(live.snapshot)?.name ?? t('the repository')}</b>{#if l.worktree.branch}{t(', on')} <code>{l.worktree.branch}</code>{/if}{#if agent.worktree}<br /><span class="muted mono">{tildePath(agent.worktree)}</span>{/if}
          {:else if l.cwd}<span class="mono">{tildePath(l.cwd)}</span>{/if}
        </span>
        <span>{t('Permissions')}</span><span>{l.permission_mode ? (MODE[l.permission_mode] ?? l.permission_mode) : t('Default')}</span>
        {#if l.category || agent.category}<span>{t('Kind')}</span><span>{l.category ?? t('none')}{#if (l.category ?? null) !== (agent.category ?? null)}<i class="changed"> · {t('changed since')}</i>{/if}</span>{/if}
        {#if l.tags.length || agent.tags.length}<span>{t('Tags')}</span><span class="tags">{#each l.tags as tag (tag)}<span class="pill">{tag}</span>{:else}<span class="muted">{t('none')}</span>{/each}{#if !sameTags}<i class="changed"> · {t('changed since')}</i>{/if}</span>{/if}
      </div>
    </div>
    <div class="sec">
      <span class="cap">{t('Prompt')}<span class="grow"></span>{#if l.prompt}<button class="mini" onclick={() => copy(l.prompt!, t('Prompt copied'))}><Icon name="copy" />{t('Copy')}</button>{/if}</span>
      {#if l.prompt}
        <pre class="prompt" class:clip={!full && lines > 8}>{l.prompt}</pre>
        {#if lines > 8}<button class="link" onclick={() => (full = !full)}>{full ? t('Show less') : t('Show all ({n} lines)', { n: lines })}</button>{/if}
      {:else}<span class="muted">{t('None: it started at its prompt.')}</span>{/if}
    </div>
    {#if l.then.length}
      <div class="sec">
        <span class="cap">{t('Then')}</span>
        {#each l.then as s, i (i)}
          <div class="step"><span class="n">{i + 1}</span>{STEP_LABEL[s.kind as Step['kind']] ?? s.kind}{#if s.command} <code>{s.command}</code>{:else if s.text} <span class="muted">“{s.text}”</span>{/if}<span class="muted"> · {WHEN_LABEL[s.when as Step['when']] ?? s.when}</span></div>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="old">{t('Launched before Prism kept its launch: only its prompt and folder are known.')}</p>
    {#if agent.prompt}<div class="sec"><span class="cap">{t('Prompt')}</span><pre class="prompt">{agent.prompt}</pre></div>{/if}
  {/if}
  <div class="acts">
    <button class="primary" onclick={again} title={t('A new agent, a new conversation, launched the same: the launcher opens prefilled (⇧-click: launch at once)')}><Icon name="retry" />{t('Launch again…')}</button>
    <span class="muted">{t('⇧-click: at once')}</span>
  </div>
</div>

<style>
  .launch { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; font-size: var(--fs-meta); }
  .sec { display: flex; flex-direction: column; gap: 5px; padding: 9px 12px; border-bottom: 1px solid var(--border); }
  .cap { display: flex; align-items: center; gap: 6px; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .cap .when { text-transform: none; letter-spacing: 0; }
  .grow { flex: 1; }
  .kv { display: grid; grid-template-columns: 82px minmax(0, 1fr); gap: 4px 8px; }
  .kv > span { overflow-wrap: anywhere; }
  .kv > span:nth-child(odd) { color: var(--text-muted); }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .changed { font-style: normal; color: var(--warn); }
  .muted { color: var(--text-muted); }
  .mono, code { font-family: var(--mono); }
  i.muted { font-style: normal; }
  .prompt { margin: 0; padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); font: 11px/1.5 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .prompt.clip { max-height: 132px; overflow: hidden; mask-image: linear-gradient(black 70%, transparent); }
  .link { align-self: flex-start; padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .mini { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); text-transform: none; letter-spacing: 0; cursor: pointer; }
  .step { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; }
  .n { display: inline-flex; align-items: center; justify-content: center; width: 15px; height: 15px; border-radius: 50%; background: var(--surface-muted); font-size: 9.5px; }
  .old { margin: 0; padding: 10px 12px; color: var(--text-muted); }
  .acts { display: flex; align-items: center; gap: 8px; margin-top: auto; padding: 10px 12px; border-top: 1px solid var(--border); background: var(--bg); }
  .acts .primary { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--src-claude); border-radius: var(--r6); background: var(--src-claude); color: #fff; font: inherit; cursor: pointer; }
</style>
