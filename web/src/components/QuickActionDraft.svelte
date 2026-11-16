<script lang="ts">
  import Icon from './Icon.svelte'
  import Drawer from './ui/Drawer.svelte'
  import State from './ui/State.svelte'
  import { backToFirst, closeQuickActionDraft, qaDraft, saveQuickActionDraft, stopDrafting } from '../quick-action-draft.svelte'
  import { kindNames } from '../agents.svelte'
  import { cliOf, providerOf } from '../agent-cli'
  import { hasWorktrees } from '../repositories'
  import { choices, loadModels } from '../settings.svelte'
  import { ago } from '../time'
  import { t, tn } from '../i18n.svelte'

  /**
   * Create a quick action… (`quick-action-draft.svelte.ts`): the form of a quick action, prefilled
   * from a past session. A drawer on the computer, the whole screen on a phone. While an
   * agent drafts the prompt, the session's first one shows, Keep this one beside it; the draft in,
   * Back to the first message. Esc or ✕ closes it (its drafting agent with it).
   */
  const phone = document.documentElement.classList.contains('remote')
  const MODES: ['' | 'default' | 'acceptEdits' | 'plan', string][] = [['', t('Its CLI’s')], ['default', 'default'], ['acceptEdits', 'acceptEdits'], ['plan', 'plan']]
  const s = $derived(qaDraft.source)
  const provider = $derived(providerOf(qaDraft.repo || undefined))
  const kinds = $derived(kindNames())
  $effect(() => {
    if (qaDraft.open) loadModels(provider)
  })
  const ready = $derived(!!s && !!qaDraft.name.trim() && !!qaDraft.prompt.trim() && !qaDraft.saving)

</script>

<Drawer open={qaDraft.open} key="quick-action" crumb={t('New quick action')} label={t('New quick action')} size={phone ? 'full' : 's'} source="var(--src-claude)" outside={false} onclose={() => void closeQuickActionDraft()} class={phone ? 'qa phone' : 'qa'}>
  {#snippet head()}
    <b><Icon name="quick" />{t('New quick action')}</b>
  {/snippet}
  <div class="body">
    {#if qaDraft.loading}
      <State kind="loading" title={t('Reading the session…')} />
    {:else if s}
      <p class="from" style:--c={cliOf(s.provider).color}><i></i>{t('From {title}', { title: s.title })}{#if qaDraft.repo}{' '}· {qaDraft.repo}{/if}{#if s.at}{' '}· {t('{ago} ago', { ago: ago(s.at) })}{/if}{#if s.prompts.length}{' '}· {tn(s.prompts.length, '{n} prompt', '{n} prompts')}{/if}</p>
      <label class="f"><span>{t('Name')}</span><input bind:value={qaDraft.name} spellcheck="false" /></label>
      <div class="f col">
        <div class="lh"><span>{t('Prompt')}</span>
          <small>{#if qaDraft.drafter}{t('first message, as is')}{:else if qaDraft.drafted && qaDraft.prompt !== qaDraft.first}{t('written from the session’s {n} prompts', { n: s.prompts.length })}{/if}</small>
        </div>
        <textarea bind:value={qaDraft.prompt} rows={phone ? 6 : 8} spellcheck="false" writingsuggestions="false"></textarea>
        <div class="state">
          {#if qaDraft.drafter}
            <span class="spin-ascii" aria-hidden="true"></span><span>{t('{cli} writes a prompt from the session’s {n} prompts…', { cli: cliOf(s.provider).name, n: s.prompts.length })}</span>
            <button class="lnk" onclick={() => void stopDrafting()}>{t('Keep this one')}</button>
          {:else if qaDraft.drafted && qaDraft.prompt !== qaDraft.first}
            <button class="lnk" onclick={backToFirst}>{t('Back to the first message')}</button>
          {:else if qaDraft.noDraft}
            <span>{s.readable ? '' : t('Its conversation cannot be read: its first prompt only.')}</span>
          {/if}
          <span class="grow"></span><code>{'{repo}'}</code> <code>{'{base}'}</code>
        </div>
      </div>
      <div class="f">
        <span>{t('Repositories')}</span>
        <div class="seg">
          {#if qaDraft.repo}<button class:on={!qaDraft.everywhere} onclick={() => (qaDraft.everywhere = false)}>{qaDraft.repo}</button>{/if}
          <button class:on={qaDraft.everywhere || !qaDraft.repo} onclick={() => (qaDraft.everywhere = true)}>{t('all')}</button>
        </div>
      </div>
      <div class="f">
        <span>{t('Where')}</span>
        <div class="seg">
          <button class:on={qaDraft.worktree} disabled={!!qaDraft.repo && !qaDraft.everywhere && !hasWorktrees(qaDraft.repo)} onclick={() => (qaDraft.worktree = true)}>{t('Worktree')}</button>
          <button class:on={!qaDraft.worktree} onclick={() => (qaDraft.worktree = false)}>{t('The clone')}</button>
        </div>
      </div>
      <div class="f">
        <span>{t('Mode')}</span>
        <div class="seg">{#each MODES as [m, label] (m)}<button class:on={qaDraft.mode === m} onclick={() => (qaDraft.mode = m)}>{label}</button>{/each}</div>
      </div>
      {#if kinds.length}
        <label class="f"><span>{t('Kind')}</span>
          <select bind:value={qaDraft.kind}><option value="">{t('Auto')}</option>{#each kinds as k (k)}<option value={k}>{k}</option>{/each}</select>
        </label>
      {/if}
      <label class="f"><span>{t('Model')}</span><input bind:value={qaDraft.model} list="qa-models" placeholder={t('{cli}’s default', { cli: cliOf(provider).name })} spellcheck="false" autocapitalize="off" /></label>
      <datalist id="qa-models">{#each choices.models[provider] ?? [] as m (m)}<option value={m}></option>{/each}</datalist>
      <label class="f"><span>{t('Effort')}</span><input bind:value={qaDraft.effort} placeholder={t('{cli}’s default', { cli: cliOf(provider).name })} spellcheck="false" autocapitalize="off" /></label>
      {#if qaDraft.otherCli}<p class="muted">{t('Its model and effort were {cli}’s: a quick action runs its repository’s CLI ({repo}).', { cli: cliOf(s.provider).name, repo: cliOf(provider).name })}</p>{/if}
    {/if}
    {#if qaDraft.error}<p class="err">{qaDraft.error}</p>{/if}
  </div>
  <footer>
    {#if !phone}<label class="routine"><input type="checkbox" bind:checked={qaDraft.routine} />{t('Make it a routine')}</label>{/if}
    <span class="grow"></span>
    {#if !phone}<button class="btn" onclick={() => void closeQuickActionDraft()}>{t('Cancel')}</button>{/if}
    <button class="btn primary" disabled={!ready} onclick={() => void saveQuickActionDraft()}>{qaDraft.saving ? t('Saving…') : t('Save')}</button>
  </footer>
</Drawer>

<style>
  b { display: inline-flex; align-items: center; gap: var(--sp-2); font-size: var(--fs-title); color: var(--text); }
  b :global(svg) { color: var(--src-claude); }
  .grow { flex: 1; min-width: 0; }
  .body { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 10px; padding: 12px; font-size: var(--fs-body); }
  .from { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--text-muted); font-size: var(--fs-body); }
  .from i { flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--c); }
  .f { display: grid; grid-template-columns: 96px minmax(0, 1fr); align-items: center; gap: 8px; }
  .f > span, .lh > span { color: var(--text-muted); font-size: var(--fs-body); }
  .f.col { grid-template-columns: 1fr; }
  .f .seg { justify-self: start; flex-wrap: wrap; }
  .lh { display: flex; align-items: baseline; gap: 8px; }
  .lh small { color: var(--text-muted); font-size: var(--fs-meta); }
  input:not([type='checkbox']), select, textarea { min-width: 0; padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); color: var(--text); font: inherit; }
  textarea { resize: vertical; font-family: var(--mono); font-size: var(--fs-body); line-height: 1.45; }
  .state { display: flex; align-items: center; gap: 6px; min-height: 20px; color: var(--text-muted); font-size: var(--fs-body); }
  .state code { font-family: var(--mono); font-size: var(--fs-meta); color: var(--info); }
  .lnk { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; }
  .muted { margin: 0; color: var(--text-muted); font-size: var(--fs-body); }
  .body > .err { margin: 0; font-size: var(--fs-body); }
  footer { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-top: 1px solid var(--border); }
  .routine { display: inline-flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-body); }
  /* On a phone: the whole screen, its controls made for a thumb. */
  :global(.qa.phone) b { font-size: var(--fs-head); }
  :global(.qa.phone) .body { padding: 4px 16px 16px; gap: 12px; font-size: var(--fs-title); }
  :global(.qa.phone) .f { grid-template-columns: 1fr; gap: 6px; }
  :global(.qa.phone) :is(input:not([type='checkbox']), select, textarea) { min-height: 42px; padding: 8px 12px; border-radius: var(--r12); font-size: var(--fs-head); }
  :global(.qa.phone) .seg > button { min-height: 38px; padding: 0 14px; font-size: var(--fs-body); }
  :global(.qa.phone) footer { padding: 10px 16px calc(10px + env(safe-area-inset-bottom)); }
  :global(.qa.phone) footer .grow { display: none; }
  :global(.qa.phone) footer .btn.primary { flex: 1; min-height: 50px; border-radius: var(--r14); font-size: var(--fs-head); font-weight: 650; }
</style>
