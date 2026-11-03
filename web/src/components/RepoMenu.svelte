<script lang="ts">
  import { forgeLabel, forgeName, forgeUrl } from '../forge'
  import { t, tn } from '../i18n.svelte'
  import { actorName } from '../agent-cli'
  import { dismissable } from '../popup'
  import { invoke, isDesktop, isLocal } from '../bridge'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import { claudeClick, repoAction } from '../claude-actions'
  import type { Repo, Snapshot } from '../types'
  import { FILE_MANAGER, keys } from '../platform'
  import { openSettings } from '../settings.svelte'
  import { repoAdd, type AddMode } from '../clones.svelte'
  import { repoNotes } from '../notes.svelte'
  import { setPref } from '../layout.svelte'
  import { openQuickNotes } from '../quick-notes.svelte'

  /**
   * Status bar Repos menu: the repositories with a clone (`[[repos]]`), the primary one first, with their
   * branch, their local changes and their gap with the default branch; for each,
   * a Claude session in its folder, the editor, the terminal, Finder and GitLab, and its notes (the
   * Notes block over the view, filtered on it: a note written there is filed under it).
   */
  let { snap }: { snap: Snapshot | null } = $props()

  const repos = $derived(snap?.repos ?? [])
  const editor = $derived(snap?.open_with?.editor ?? null)
  /** Off the default branch, with changes or behind: worth a look. */
  const attention = (r: Repo) => !!r.error || r.dirty || r.behind > 0 || (!!r.branch && r.branch !== r.base)
  const flagged = $derived(repos.filter((r) => !r.is_main && attention(r)).length)

  let open = $state(false)
  let error = $state<string | null>(null)
  async function openWith(r: Repo, target: 'editor' | 'terminal' | 'finder') {
    error = null
    try {
      await invoke('open_repo', { path: r.path, with: target })
      open = false
    } catch (e) {
      error = String(e)
    }
  }
  function add(mode: AddMode, publish = '') {
    open = false
    repoAdd.publish = publish
    repoAdd.mode = mode
    openSettings('repos')
  }
  function showNotes(name: string) {
    open = false
    setPref('notes.repo', name)
    openQuickNotes()
  }
  const forgePage = (r: Repo) => (r.project ? forgeUrl(r.project, r.forge) : null)
</script>

{#if repos.length > 1}
  <button class="sbi" class:on={open} onclick={(e) => { e.stopPropagation(); open = !open }} title={t('Repositories: branch, local changes, {agent}, editor, terminal', { agent: actorName() })} aria-expanded={open}>
    <span class="g"><Icon name="branch" /></span>{tn(repos.length, '{n} repo', '{n} repos')}{#if flagged}<span class="flag" title={tn(flagged, '{n} repo off its default branch, with local changes or behind', '{n} repos off their default branch, with local changes or behind')}>{flagged}</span>{/if}
  </button>
{/if}

{#if open}
  <div class="pop" role="dialog" aria-label={t('Repositories')} tabindex="-1" {@attach dismissable(() => (open = false))}>
    <p class="head">Repositories</p>
    {#each repos as r (r.path)}
      {@const link = forgePage(r)}
      {@const count = repoNotes(r.name).length}
      <div class="repo" class:main={r.is_main}>
        <div class="info">
          <b>{r.name}</b>
          {#if r.error}
            <span class="err" title={r.error}>{r.error}</span>
          {:else}
            <span class="branch mono" class:off={r.branch !== r.base} title={r.branch ?? t('detached HEAD')}>{r.branch ?? t('detached')}</span>
            {#if r.dirty}<span class="dirty" title={t('Uncommitted changes')}><Icon name="running" /></span>{/if}
            {#if r.behind}<span class="behind" title={tn(r.behind, '{n} commit behind origin/{base}', '{n} commits behind origin/{base}', { base: r.base ?? '' })}><Icon name="down" />{r.behind}</span>{/if}
            {#if r.ahead}<span class="ahead" title={tn(r.ahead, '{n} commit ahead of origin/{base}', '{n} commits ahead of origin/{base}', { base: r.base ?? '' })}><Icon name="up" />{r.ahead}</span>{/if}
          {/if}
        </div>
        <div class="acts">
          <button class="act" class:has={count > 0} onclick={() => showNotes(r.name)} title={count ? tn(count, 'Its {n} open note, and a new one', 'Its {n} open notes, and a new one') : t('A note about it')}><Icon name="pin" />{count || ''}</button>
          <a class="act claude" href={repoAction(r.name, r.path).href} onclick={claudeClick(repoAction(r.name, r.path))} title={t('New {agent} agent in {path}', { agent: actorName(r.name), path: r.path })}><Logo name="agent" size={10} repo={r.name} />{actorName(r.name)}</a>
          {#if isDesktop()}
            {#if editor}<button class="act" onclick={() => openWith(r, 'editor')} title={t('Open in {app}', { app: editor })}>{t('Editor')}</button>{/if}
          {/if}
          {#if isLocal()}
            <button class="act" onclick={() => openWith(r, 'terminal')} title={t('Open in {app}', { app: snap?.open_with?.terminal ?? 'Terminal' })}>{t('Terminal')}</button>
            <button class="act" onclick={() => openWith(r, 'finder')} title={keys(t('Show in the Finder'))}>{FILE_MANAGER}</button>
          {/if}
          {#if link}<a class="act" href={link} title={r.project}>{forgeLabel(r.forge)}</a>
          {:else if isDesktop() && !r.error}<button class="act" onclick={() => add('publish', r.name)} title={t('Kept local: create its project on {forge} and push', { forge: forgeName() })}>{t('Publish…')}</button>{/if}
        </div>
      </div>
    {/each}
    {#if error}<p class="error">{error}</p>{/if}
    {#if isDesktop()}
      <div class="foot">
        <button class="act" onclick={() => add('folder')} title={t('Settings › Repositories: an existing folder, or a clone of a project')}><Icon name="folder" />{t('Add a repository…')}</button>
        <button class="act" onclick={() => add('new')} title={t('Settings › Repositories: a new folder with a git repository')}><Icon name="plus" />{t('New project…')}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .sbi { display: flex; align-items: center; gap: 5px; height: 20px; padding: 0 7px; border: none; border-radius: var(--r5); background: none; color: inherit; font: inherit; cursor: pointer; }
  .sbi:hover, .sbi.on { background: var(--surface-muted); color: var(--text); }
  .g { color: var(--src-gitlab); }
  .flag { min-width: 14px; padding: 0 4px; border-radius: var(--r7); background: var(--warn-bg); color: var(--warn); font-size: var(--fs-tag); font-weight: 600; text-align: center; }
  .pop { position: fixed; left: 8px; bottom: 30px; z-index: 50; width: min(460px, calc(100vw - 16px)); padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); font-size: var(--fs-body); color: var(--text); white-space: normal; }
  .head { margin: 2px 8px 4px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .repo { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border-radius: var(--r6); }
  .repo:hover { background: var(--surface-muted); }
  .repo.main { border-bottom: 1px solid var(--border); border-radius: var(--r6) var(--r6) 0 0; margin-bottom: 2px; }
  .info { flex: 1; display: flex; align-items: baseline; gap: 6px; min-width: 0; white-space: nowrap; }
  .info b { font-weight: 600; }
  .branch { min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--text-muted); font-size: var(--fs-tag); }
  .branch.off { color: var(--info); }
  .dirty { align-self: center; display: inline-flex; color: var(--warn); font-size: 11px; }
  .behind { color: var(--warn); font-family: var(--mono); font-size: var(--fs-tag); }
  .ahead { color: var(--ok); font-family: var(--mono); font-size: var(--fs-tag); }
  .err { min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--ko); font-size: var(--fs-tag); }
  .mono { font-family: var(--mono); }
  .acts { display: flex; gap: 4px; flex-shrink: 0; }
  .act { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; cursor: pointer; }
  .act:hover { background: var(--surface-muted); text-decoration: none; }
  .act.has { border-color: color-mix(in srgb, var(--accent) 60%, var(--border)); }
  .act.claude { border-color: color-mix(in srgb, var(--src-claude) 60%, var(--border)); }
  .foot { display: flex; gap: 4px; padding: 6px 8px 3px; border-top: 1px solid var(--border); margin-top: 2px; }
  .error { margin: 4px 8px; color: var(--ko); font-size: var(--fs-tag); }
</style>
