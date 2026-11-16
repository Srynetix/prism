<script lang="ts">
  import type { GitState } from '../bindings/GitState'
  import { invoke } from '../bridge'
  import { armer } from '../armed.svelte'
  import { toast } from '../toast.svelte'
  import { t } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import ArmedLabel from './ArmedLabel.svelte'
  import type { Agent } from '../types'

  /**
   * The Git bar of an agent's Diff tab: where its folder stands, and without the agent its changes
   * committed, its branch pushed, brought up to date with its base. None of it on a protected
   * branch (main, a release), none while it works, no hook run. Force push takes two taps.
   * `onchange`: something moved, the diff is read again.
   */
  let { agent, onchange }: { agent: Agent; onchange?: () => void } = $props()

  let git = $state<GitState | null>(null)
  let error = $state('')
  let busy = $state('')
  let message = $state('')
  /** A push refused as origin's branch moved on (after a rebase): Force push offered. */
  let diverged = $state(false)
  const armed = armer()

  const working = $derived(agent.status === 'running' || agent.status === 'waiting_tool')
  const base = $derived(git?.base.replace(/^origin\//, '') ?? '')

  async function load() {
    try {
      git = await invoke<GitState>('agent_git_state', { id: agent.id })
      error = ''
    } catch (e) {
      git = null
      error = String(e)
    }
  }
  $effect(() => {
    void agent.id
    void agent.status
    void load()
  })
  /** Its message to start from: its ticket, then its title; again for another agent, and after a commit. */
  const firstMessage = () => `${agent.ticket && !agent.title.includes(agent.ticket) ? `${agent.ticket} ` : ''}${agent.title.split(' · ').pop() ?? agent.title}`
  let messageOf = ''
  $effect(() => {
    if (agent.id === messageOf) return
    messageOf = agent.id
    message = firstMessage()
  })

  async function run(what: string, command: string, args: Record<string, unknown>, done: (r: unknown) => string) {
    busy = what
    error = ''
    try {
      const r = await invoke(command, { id: agent.id, ...args })
      toast(done(r))
      if (what === 'push' || what === 'force') diverged = false
      onchange?.()
    } catch (e) {
      error = String(e)
      if (what === 'push' && /Force push/.test(error)) diverged = true
    } finally {
      busy = ''
      await load()
    }
  }
  const commit = () => run('commit', 'agent_git_commit', { message }, (h) => ((message = firstMessage()), t('Committed {hash}', { hash: String(h) })))
  const push = () => run('push', 'agent_git_push', {}, () => t('{branch} pushed', { branch: git?.branch ?? '' }))
  const force = () => armed.confirm('force') && run('force', 'agent_git_force_push', {}, () => t('{branch} force-pushed', { branch: git?.branch ?? '' }))
  const sync = () => run('sync', 'agent_git_sync', {}, (m) => String(m))
</script>

<div class="git">
  {#if git}
    <div class="st">
      <Icon name="branch" />
      <b class="mono">{git.branch ?? t('detached')}</b>
      {#if git.protected}<span class="pill warn outline">{t('protected')}</span>{/if}
      <span class="muted">
        {git.changes ? t('{n} changed', { n: git.changes }) : t('clean')}
        · {git.ahead}↑ {git.behind}↓ {base}
        {#if git.upstream}· {git.unpushed ? t('{n} to push', { n: git.unpushed }) : t('pushed')}{git.unpulled ? ` · ${t('{n} on origin only', { n: git.unpulled })}` : ''}{:else if git.ahead}· {t('not on origin')}{/if}
      </span>
      <span class="grow"></span>
      <button class="btn sm icon ghost" onclick={load} disabled={!!busy} aria-label={t('Read again')} title={t('Read again')}><Icon name="refresh" spin={!!busy} /></button>
    </div>
    {#if !git.protected && git.branch}
      {#if git.changes}
        <form class="row" onsubmit={(e) => { e.preventDefault(); void commit() }}>
          <input bind:value={message} placeholder={t('Commit message')} spellcheck="false" aria-label={t('Commit message')} />
          <button class="btn sm primary claude" class:busy={busy === 'commit'} disabled={!!busy || working || !message.trim()} title={working ? t('Once its turn is over') : t('git add -A, then commit (no hook runs)')}>{#if busy === 'commit'}<span class="spinner"></span>{t('Committing…')}{:else}{t('Commit {n}', { n: git.changes })}{/if}</button>
        </form>
      {/if}
      <div class="row">
        {#if git.unpushed || !git.upstream}<button class="btn sm" class:busy={busy === 'push'} onclick={push} disabled={!!busy || (!git.ahead && !git.unpushed)} title={t('git push -u origin {branch} (no hook runs)', { branch: git.branch })}>{#if busy === 'push'}<span class="spinner"></span>{t('Pushing…')}{:else}{t('Push')}{/if}</button>{/if}
        {#if diverged}<button class="btn sm danger" class:armed={armed.is('force')} class:busy={busy === 'force'} onclick={force} disabled={!!busy} title={t('--force-with-lease: what origin has and this folder lacks is lost')}>{#if busy === 'force'}<span class="spinner"></span>{/if}<ArmedLabel armed={armed.is('force')} label={t('Force push')} armedLabel={t('Click again to confirm')} /></button>{/if}
        {#if git.behind}<button class="btn sm" class:busy={busy === 'sync'} onclick={sync} disabled={!!busy || working || git.changes > 0} title={git.changes ? t('Commit first') : t('Rebase onto {base}: a conflict leaves it as it was', { base: git.base ?? '' })}>{#if busy === 'sync'}<span class="spinner"></span>{t('Rebasing…')}{:else}{t('Rebase onto {branch}', { branch: base })}{/if}</button>{/if}
      </div>
    {:else if git.protected && git.behind}
      <div class="row"><button class="btn sm" class:busy={busy === 'sync'} onclick={sync} disabled={!!busy || working || git.changes > 0} title="git pull --ff-only">{#if busy === 'sync'}<span class="spinner"></span>{t('Updating…')}{:else}{t('Pull')}{/if}</button></div>
    {/if}
  {/if}
  {#if error}<p class="err">{error}</p>{/if}
</div>

<style>
  .git { display: flex; flex-direction: column; gap: 5px; padding: 6px 8px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); }
  .st { display: flex; align-items: center; gap: 6px; min-width: 0; flex-wrap: wrap; }
  .mono { font-family: var(--mono); }
  .grow { flex: 1; }
  .row { display: flex; gap: 6px; flex-wrap: wrap; }
  .row input { flex: 1; min-width: 140px; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; outline: none; }
  .row input:focus { border-color: var(--src-claude); }
  .git > .err { margin: 0; }
  :global(html.remote) .git button { min-height: 34px; }
  :global(html.remote) .git input { font-size: var(--fs-head); min-height: 34px; }
</style>
