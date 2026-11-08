<script lang="ts">
  import Icon from './Icon.svelte'
  import { backToFirst, closeQuickActionDraft, qaDraft, saveQuickActionDraft, stopDrafting } from '../quick-action-draft.svelte'
  import { kindNames } from '../agents.svelte'
  import { cliOf, providerOf } from '../agent-cli'
  import { hasWorktrees } from '../repositories'
  import { choices, loadModels } from '../settings.svelte'
  import { ago } from '../time'
  import { t, tn } from '../i18n.svelte'

  /**
   * Create a quick action… (`quick-action-draft.svelte.ts`): the form of a quick action, prefilled
   * from a past session. A drawer on the computer, a sheet from the bottom on a phone. While an
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

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !qaDraft.open) return
    e.stopImmediatePropagation()
    void closeQuickActionDraft()
  }
</script>

<svelte:window onkeydowncapture={onKey} />

{#if phone}<div class="scrim" role="presentation"></div>{/if}
<aside class="qa" class:phone data-drawer="53" aria-label={t('New quick action')}>
  {#if phone}<div class="grab"></div>{/if}
  <header>
    <b><Icon name="quick" />{t('New quick action')}</b>
    <span class="grow"></span>
    <button class="x" onclick={() => void closeQuickActionDraft()} aria-label={t('Close (Esc)')}><Icon name="close" /></button>
  </header>
  <div class="body">
    {#if qaDraft.loading}
      <p class="muted">{t('Reading the session…')}</p>
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
    {#if !phone}<button onclick={() => void closeQuickActionDraft()}>{t('Cancel')}</button>{/if}
    <button class="primary" disabled={!ready} onclick={() => void saveQuickActionDraft()}>{qaDraft.saving ? t('Saving…') : t('Save')}</button>
  </footer>
</aside>

<style>
  .qa { position: fixed; top: 0; right: 0; bottom: 0; z-index: 53; width: min(520px, 92vw); display: flex; flex-direction: column; background: var(--surface); border-left: 1px solid color-mix(in srgb, var(--src-claude) 45%, var(--border)); box-shadow: var(--shadow-drawer); animation: slide 0.16s ease-out; }
  @keyframes slide { from { transform: translateX(24px); opacity: 0; } }
  :global(html[data-reduce-motion]) .qa { animation: none; }
  header { display: flex; align-items: center; gap: 8px; padding: 9px 12px; border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--src-claude); }
  header b { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; }
  header b :global(svg) { color: var(--src-claude); }
  .grow { flex: 1; min-width: 0; }
  .x { display: inline-flex; padding: 3px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .body { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 10px; padding: 12px; font-size: var(--fs-body, 13px); }
  .from { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--text-muted); font-size: 12px; }
  .from i { flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--c); }
  .f { display: grid; grid-template-columns: 96px minmax(0, 1fr); align-items: center; gap: 8px; }
  .f > span, .lh > span { color: var(--text-muted); font-size: 12px; }
  .f.col { grid-template-columns: 1fr; }
  .lh { display: flex; align-items: baseline; gap: 8px; }
  .lh small { color: var(--text-muted); font-size: 11.5px; }
  input:not([type='checkbox']), select, textarea { min-width: 0; padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--bg); color: var(--text); font: inherit; }
  textarea { resize: vertical; font-family: var(--mono); font-size: 12px; line-height: 1.45; }
  .state { display: flex; align-items: center; gap: 6px; min-height: 20px; color: var(--text-muted); font-size: 12px; }
  .state code { font-family: var(--mono); font-size: 11px; color: var(--info); }
  .lnk { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; }
  .seg { display: inline-flex; flex-wrap: wrap; gap: 4px; }
  .seg button { padding: 3px 10px; border: 1px solid var(--border); border-radius: 12px; background: none; color: var(--text-muted); font: inherit; font-size: 12px; cursor: pointer; }
  .seg button.on { border-color: var(--accent); color: var(--text); font-weight: 600; background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .seg button:disabled { opacity: 0.4; cursor: default; }
  .muted { margin: 0; color: var(--text-muted); font-size: 12px; }
  .err { margin: 0; color: var(--ko); font-size: 12px; overflow-wrap: anywhere; }
  footer { display: flex; align-items: center; gap: 8px; padding: 10px 12px; border-top: 1px solid var(--border); }
  footer button { padding: 5px 14px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: none; color: var(--text); font: inherit; cursor: pointer; }
  footer .primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent, #fff); }
  footer button:disabled { opacity: 0.5; cursor: default; }
  .routine { display: inline-flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: 12px; }
  /* On a phone: a sheet from the bottom, its controls made for a thumb. */
  .scrim { position: fixed; inset: 0; z-index: 60; background: rgb(0 0 0 / 0.45); }
  .qa.phone { top: auto; left: 0; z-index: 61; width: auto; max-height: calc(100dvh - 24px - env(safe-area-inset-top)); border: none; border-radius: 18px 18px 0 0; background: var(--bg); box-shadow: 0 -8px 30px rgb(0 0 0 / 0.25); animation: none; font-size: 15px; }
  .qa.phone .grab { width: 38px; height: 5px; margin: 8px auto 0; border-radius: 3px; background: var(--border); }
  .qa.phone header { border: none; box-shadow: none; padding: 6px 10px 6px 16px; }
  .qa.phone header b { font-size: 17px; }
  .qa.phone .x { padding: 10px; }
  .qa.phone .body { padding: 4px 16px 16px; gap: 12px; font-size: 15px; }
  .qa.phone .f { grid-template-columns: 1fr; gap: 6px; }
  .qa.phone input:not([type='checkbox']), .qa.phone select, .qa.phone textarea { min-height: 42px; padding: 8px 12px; border-radius: 12px; font-size: 16px; }
  .qa.phone .seg button { min-height: 38px; padding: 0 14px; border-radius: 19px; font-size: 14px; }
  .qa.phone footer { padding: 10px 16px calc(10px + env(safe-area-inset-bottom)); }
  .qa.phone footer .grow { display: none; }
  .qa.phone footer .primary { flex: 1; min-height: 50px; border-radius: 14px; font-size: 17px; font-weight: 650; }
</style>
