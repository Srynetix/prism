<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { clearProblems, current, openProblems, problems, problemsText, recent, seenAt, type Problem } from '../problems.svelte'
  import { openSettings } from '../settings.svelte'
  import { copy } from '../toast.svelte'
  import { ago } from '../time'
  import type { Snapshot } from '../types'
  import Icon from './Icon.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Prism's problems in the status bar (problems.svelte.ts): shown while something fails now, or
   * failed since the list was last opened; red for an error, amber for warnings only. A click lists
   * them: what fails now, then what failed lately, each copied with a click; the log in Settings.
   */
  let { snap, now }: { snap: Snapshot | null; now: number } = $props()

  let button: HTMLButtonElement | undefined = $state()
  const nowList = $derived(current(snap))
  const lately = $derived(recent(snap))
  const unseen = $derived(lately.filter((p) => p.at! > seenAt()))
  const shown = $derived(nowList.length + unseen.length)
  const ko = $derived(nowList.some((p) => p.level === 'ko') || unseen.some((p) => p.level === 'ko'))

  function toggle(e: MouseEvent) {
    e.stopPropagation()
    openProblems(!problems.open)
  }
  function fix(p: Problem) {
    openProblems(false)
    openSettings(p.settings)
  }
  const when = (p: Problem) => (void now, ago(new Date(p.at!).toISOString()))
</script>

{#if shown || problems.open}
  <button bind:this={button} class="sbi problems" class:ko class:open={problems.open} onclick={toggle} aria-expanded={problems.open} title={t('Prism’s problems: what fails now, and what failed lately · list')}>
    <span class="g"><Icon name="warning" /></span>{#if shown}<b>{shown}</b> {tn(shown, 'problem', 'problems')}{:else}{t('Problems')}{/if}
  </button>
{/if}
{#if problems.open}
  <div class="pop" role="dialog" aria-label={t('Problems')} {@attach anchored(() => button, { align: 'start', gap: 6 })} {@attach exclusive(() => openProblems(false))} {@attach dismissable(() => openProblems(false), { trigger: () => button })}>
    <div class="head">
      <span>{t('Problems')}</span>
      <span class="acts">
        <button onclick={() => void copy(problemsText(nowList, lately), t('Problems copied'))} disabled={!nowList.length && !lately.length} title={t('Copy them all, to paste in an issue or a message')}><Icon name="copy" />{t('Copy')}</button>
        <button onclick={() => void clearProblems()} disabled={!lately.length} title={t('Forget what failed lately; what fails now stays')}><Icon name="trash" />{t('Clear')}</button>
        <button onclick={() => { openProblems(false); openSettings('logs') }} title={t('The whole log: Settings › Logs')}><Icon name="logs" />{t('Log')}</button>
      </span>
    </div>
    <div class="body">
      {#if nowList.length}
        <p class="grp">{t('Failing now')}</p>
        {#each nowList as p, i (i)}
          <div class="prow {p.level}">
            <button class="txt" onclick={() => void copy(`${p.where}: ${p.message}`, t('Copied'))} title={t('Copy')}>
              <small><i></i>{p.where}</small><span class="msg">{p.message}</span>
            </button>
            {#if p.settings}<button class="fix" onclick={() => fix(p)}><Icon name="settings" />{t('Settings')}</button>{/if}
          </div>
        {/each}
      {/if}
      {#if lately.length}
        <p class="grp">{t('Lately')}</p>
        {#each lately as p, i (i)}
          <div class="prow {p.level}" class:new={p.at! > problems.since}>
            <button class="txt" onclick={() => void copy(`${p.where}: ${p.message}`, t('Copied'))} title={t('Copy')}>
              <small><i></i>{p.where} · {when(p)}{#if p.count > 1}{' '}· ×{p.count}{/if}</small><span class="msg">{p.message}</span>
            </button>
          </div>
        {/each}
      {/if}
      {#if !nowList.length && !lately.length}<p class="none">{t('Nothing failed since Prism started.')}</p>{/if}
    </div>
  </div>
{/if}

<style>
  .problems { flex: none; }
  .problems .g { color: var(--warn); }
  .problems.ko .g { color: var(--ko); }
  .problems.open { background: var(--surface-muted); color: var(--text); }
  .pop { position: fixed; z-index: 45; display: flex; flex-direction: column; width: 520px; max-width: calc(100vw - 24px); max-height: 60vh; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); font-size: var(--fs-body); color: var(--text); }
  .head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 4px 6px 8px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .acts { display: flex; gap: 2px; text-transform: none; letter-spacing: 0; }
  button { border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
  .acts button, .fix { display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 6px; border-radius: var(--r5); color: var(--text-muted); font-size: var(--fs-meta); }
  .acts button:hover:not(:disabled), .fix:hover { background: var(--surface-muted); color: var(--text); }
  .acts button:disabled { opacity: 0.4; cursor: default; }
  .body { min-height: 0; overflow-y: auto; }
  .grp { margin: 6px 8px 2px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .prow { display: flex; align-items: flex-start; gap: 4px; border-radius: var(--r6); --sev: var(--warn); }
  .prow.ko { --sev: var(--ko); }
  .prow:hover { background: var(--surface-muted); }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; padding: 5px 8px; text-align: left; }
  .txt small { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-meta); }
  .txt small i { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--sev); }
  .prow.new .txt small { color: var(--text); }
  .msg { font-family: var(--mono); font-size: var(--fs-mono); white-space: pre-wrap; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
  .prow:hover .msg { -webkit-line-clamp: unset; line-clamp: unset; }
  .fix { flex: none; margin: 5px 4px 0 0; }
  .none { margin: 8px; color: var(--text-muted); }
</style>
