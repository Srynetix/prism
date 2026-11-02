<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../Icon.svelte'
  import { hasCore, invoke } from '../../bridge'
  import { t } from '../../i18n.svelte'
  import { copy } from '../../toast.svelte'
  import type { ToolVersion } from '../../bindings/ToolVersion'

  /**
   * Settings › General › Dependencies: the tools Prism runs (deps.rs), found or not, and their
   * versions — what a bug report needs, attached by the feedback composer too ("Tools").
   */
  const ROLE: Record<string, string> = {
    tmux: t('Hosts the agents and the shells'),
    git: t('Worktrees, branches, commits'),
    claude: t('Agent CLI · Claude Code'),
    cmd: t('Agent CLI · Command Code'),
    cmdc: t('Agent CLI · Command Code'),
    opencode: t('Agent CLI · OpenCode'),
    codex: t('Agent CLI · Codex'),
    glab: t('GitLab: merge requests, pipelines, releases'),
    gh: t('GitHub: pull requests, checks, releases'),
    kubectl: t('Kubernetes: pods, logs, shells'),
    gcloud: t('Google Cloud: images, metrics'),
    trivy: t('Vulnerability scans'),
    docker: t('Docker block'),
  }
  let list = $state<ToolVersion[]>([])
  let loading = $state(false)
  let error = $state('')

  async function load(fresh: boolean) {
    if (!hasCore()) return
    loading = true
    error = ''
    try {
      list = await invoke<ToolVersion[]>('dependencies', { fresh })
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }
  onMount(() => void load(false))

  const missing = $derived(list.filter((d) => d.needed && !d.path).length)
  const asText = () => list.map((d) => `${d.name}: ${d.raw ?? (d.path ? '?' : 'not found')}`).join('\n')
</script>

<div class="card">
  <div class="head">
    <b>{t('Tools Prism runs')}</b>
    {#if missing}<span class="warn"><Icon name="warning" /> {t('{n} needed, not found', { n: missing })}</span>{/if}
    <button class="btn" onclick={() => copy(asText(), t('Versions copied'))} disabled={!list.length}>{t('Copy')}</button>
    <button class="btn" onclick={() => load(true)} disabled={loading} title={t('Ask each tool its version again')}>{loading ? t('Reading…') : t('Refresh')}</button>
  </div>
  {#if error}<p class="warn pad"><Icon name="warning" /> {error}</p>{/if}
  {#each list as d (d.name)}
    <div class="row" class:absent={!d.path}>
      <code class="name">{d.name}</code>
      <span class="ver mono" title={d.raw ?? ''}>{d.version ?? (d.path ? '?' : '—')}</span>
      <span class="role">{ROLE[d.name] ?? ''}</span>
      {#if d.path}<code class="path" title={d.path}>{d.path}</code>
      {:else if d.needed}<span class="miss"><span class="pill ko">{t('Not found')}</span>{#if d.hint}<code class="path">{d.hint}</code>{/if}</span>
      {:else}<span class="pill">{t('Optional')}</span>{/if}
    </div>
  {:else}
    <p class="muted pad">{loading ? t('Reading…') : hasCore() ? t('None found.') : t('The tools are the app’s.')}</p>
  {/each}
  <p class="muted pad foot">{t('Found in the PATH Prism was started with, an agent CLI at its bin when set. The versions go with a feedback request (Tools), the paths never.')}</p>
</div>

<style>
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .head { display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .head b { flex: 1; font-weight: 550; }
  .row { display: grid; grid-template-columns: 90px 80px minmax(0, 1fr) minmax(0, 1.2fr); align-items: baseline; gap: 10px; padding: 7px 14px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .row.absent .name, .row.absent .ver { color: var(--text-muted); }
  code { font-family: var(--mono); font-size: var(--fs-tag); }
  .name { color: var(--text); }
  .ver { font-size: var(--fs-tag); color: var(--text); }
  .role { color: var(--text-muted); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .path { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); }
  .pad { margin: 0; padding: 10px 14px; }
  .foot { border-top: 0; }
  .warn { display: inline-flex; align-items: center; gap: 4px; color: var(--warn); font-size: var(--fs-meta); }
  .miss { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .pill { justify-self: start; font-size: var(--fs-tag); padding: 1px 7px; border-radius: 9px; background: var(--surface-muted); color: var(--text-muted); white-space: nowrap; }
  .pill.ko { background: var(--warn-bg); color: var(--warn); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 4px 12px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  @media (max-width: 640px) {
    .row { grid-template-columns: 80px 70px minmax(0, 1fr); }
    .role { display: none; }
  }
</style>
