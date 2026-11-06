<script lang="ts">
  import { invoke, hasCore } from '../../bridge'
  import { t, tn } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import { armer } from '../../armed.svelte'
  import { ago } from '../../time'
  import { configurePrism } from '../../prism-agent'
  import { openProposal } from '../../mcp-view.svelte'
  import type { McpInstall } from '../../bindings/McpInstall'

  /**
   * Settings › Agents › Prism MCP, beyond its switches: the agent CLIs outside Prism it is added to
   * (`prism mcp install <cli>`), the agents connected now, and "Configure Prism".
   */
  const NAMES: Record<string, string> = { claude: 'Claude Code', codex: 'Codex', opencode: 'OpenCode' }
  let installs = $state<McpInstall[]>([])
  let busy = $state('')
  let message = $state('')
  const arm = armer()

  async function load() {
    if (!hasCore()) return
    try {
      installs = await invoke<McpInstall[]>('mcp_installs')
    } catch (e) {
      message = String(e)
    }
  }
  $effect(() => void load())

  async function set(cli: string, on: boolean) {
    if (!on && !arm.confirm(cli)) return
    busy = cli
    message = ''
    try {
      message = await invoke<string>('mcp_install', { cli, on })
    } catch (e) {
      message = String(e)
    } finally {
      busy = ''
      void load()
    }
  }

  const clients = $derived(live.snapshot?.mcp?.clients ?? [])
  const proposals = $derived(live.snapshot?.mcp?.proposals ?? [])
  const agentTitle = (id?: string) => (id ? live.snapshot?.agents.find((a) => a.id === id)?.title : undefined)
</script>

<div class="card">
  <div class="head">
    <b>{t('Configure Prism')}</b>
    <span class="grow"></span>
    <button class="btn" onclick={() => configurePrism()}>✦ {t('Ask an agent')}</button>
  </div>
  <p class="hint">{t('An agent in the config’s folder, with Prism’s MCP server and its skill: it reads the docs, the schema and your config, checks its changes and proposes them. You apply them here, in two clicks.')}</p>
</div>

<div class="card">
  <div class="head"><b>{t('Agents outside Prism')}</b></div>
  <p class="hint">{t('prism mcp install <cli>: the MCP server in the CLI’s user config, the skill in its skills’ folder.')}</p>
  {#each installs as i (i.cli)}
    <div class="row">
      <span class="name">{NAMES[i.cli] ?? i.cli}</span>
      {#if i.server}
        <span class="chip ok">{i.skill ? t('MCP + skill') : t('MCP')}</span>
      {:else if !i.present}
        <span class="chip">{t('not found')}</span>
      {:else}
        <span class="chip">{t('not added')}</span>
      {/if}
      <span class="grow"></span>
      {#if i.server}
        <button class="btn ghost" disabled={busy === i.cli} onclick={() => set(i.cli, false)}>{arm.is(i.cli) ? t('Remove?') : t('Remove')}</button>
        <button class="btn ghost" disabled={busy === i.cli} onclick={() => set(i.cli, true)} title={t('Write it again: Prism’s executable may have moved')}>{t('Update')}</button>
      {:else}
        <button class="btn" disabled={busy === i.cli} onclick={() => set(i.cli, true)}>{t('Add')}</button>
      {/if}
    </div>
  {:else}
    <p class="hint">{hasCore() ? t('Reading…') : t('Not in the browser preview.')}</p>
  {/each}
  {#if message}<p class="hint">{message}</p>{/if}
</div>

<div class="card">
  <div class="head"><b>{t('Connected now')}</b></div>
  {#each clients as c (c.id)}
    <div class="row">
      <span class="name">{agentTitle(c.agent) ?? c.name}</span>
      <span class="hint">{#if agentTitle(c.agent)}{c.name} · {/if}{t('since {when}', { when: ago(c.since) })} · {tn(c.reads, '{n} lookup', '{n} lookups')} · {tn(c.proposals, '{n} proposal', '{n} proposals')}</span>
    </div>
  {:else}
    <p class="hint">{t('No agent on Prism’s MCP server.')}</p>
  {/each}
  {#if proposals.length}
    <button class="btn" onclick={() => openProposal()}>{tn(proposals.length, 'See the proposal waiting', 'See the {n} proposals waiting')}</button>
  {/if}
</div>

<style>
  .card { border: 1px solid var(--border); border-radius: var(--r8, 8px); padding: 10px 12px; margin: 0 0 12px; }
  .head { display: flex; align-items: center; gap: 8px; }
  .grow { flex: 1; }
  .hint { color: var(--text-muted); font-size: var(--fs-meta); margin: 4px 0; }
  .row { display: flex; align-items: center; gap: 8px; padding: 5px 0; border-top: 1px solid var(--border); }
  .row:first-of-type { border-top: none; }
  .name { font-weight: 500; }
  .chip { padding: 0 8px; border-radius: var(--r9); background: var(--surface-muted); font-size: var(--fs-tag); line-height: 18px; }
  .chip.ok { color: var(--ok); background: var(--ok-bg); }
  .btn { font: inherit; font-size: var(--fs-meta); padding: 3px 10px; border-radius: var(--r6); border: 1px solid var(--border); background: var(--surface); color: var(--text); cursor: pointer; }
  .btn:hover:not(:disabled) { background: var(--surface-muted); }
  .btn:disabled { opacity: 0.5; }
  .btn.ghost { border-color: transparent; }
</style>
