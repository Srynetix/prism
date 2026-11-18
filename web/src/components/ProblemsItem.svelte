<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { clearProblems, current, openProblems, problems, problemsText, recent, seenAt, type Problem } from '../problems.svelte'
  import { modulesReady, packsReady, updateReady, updates } from '../updates.svelte'
  import { openSettings } from '../settings.svelte'
  import { copy } from '../toast.svelte'
  import { ago } from '../time'
  import type { Snapshot } from '../types'
  import Icon from './Icon.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * The status bar's `prism` item: Prism itself, in one dot and a few words. Its problems
   * (problems.svelte.ts: what fails now, what failed since the list was last opened), an update of
   * Prism, packs or modules changed at their source (updates.svelte.ts). Red for an error, amber for
   * warnings or something to update, green otherwise. A click lists them all, each with its action:
   * the update in Settings › General › Updates, the packs in Settings › General › Packs, a problem copied or fixed.
   * (It replaces the former `problems`, `update` and `packs` items, whose ids a config still reads.)
   */
  let { snap, now }: { snap: Snapshot | null; now: number } = $props()

  let button: HTMLButtonElement | undefined = $state()
  const nowList = $derived(current(snap))
  const lately = $derived(recent(snap))
  const unseen = $derived(lately.filter((p) => p.at! > seenAt()))
  const shown = $derived(nowList.length + unseen.length)
  const ko = $derived(nowList.some((p) => p.level === 'ko') || unseen.some((p) => p.level === 'ko'))
  const update = $derived(updateReady() ? updates.info!.latest!.version : null)
  const packs = $derived([...packsReady().map((p) => `${p.name}${p.latest ? ` ${p.latest}` : ''}`), ...modulesReady().map((m) => `${m.id}${m.latest ? ` ${m.latest}` : ''}`)])
  const tone = $derived(ko ? 'ko' : shown || update || packs.length ? 'warn' : 'ok')
  const label = $derived(update ? `Prism ${update}` : packs.length ? tn(packs.length, '{n} update', '{n} updates') : 'Prism')
  const title = $derived([
    shown ? tn(shown, '{n} problem: what fails now, or failed lately', '{n} problems: what fails now, or failed lately') : t('No problem'),
    update ? t('Prism {version} is out (this one: {current})', { version: update, current: updates.info!.current }) : '',
    packs.length ? tn(packs.length, '{packs} changed at its source', '{packs} changed at their sources', { packs: packs.join(', ') }) : '',
  ].filter(Boolean).join(' · '))

  function toggle(e: MouseEvent) {
    e.stopPropagation()
    openProblems(!problems.open)
  }
  function go(section: string | undefined) {
    openProblems(false)
    openSettings(section)
  }
  const when = (p: Problem) => (void now, ago(new Date(p.at!).toISOString()))
</script>

<button bind:this={button} class="sbi prism" class:open={problems.open} onclick={toggle} aria-expanded={problems.open} {title}>
  <span class="dot {tone}"></span>{#if shown}<b>{shown}</b> {tn(shown, 'problem', 'problems')}{:else}{label}{/if}
</button>
{#if problems.open}
  <div class="pop popover" role="dialog" aria-label={t('Prism')} {@attach anchored(() => button, { align: 'end', gap: 6 })} {@attach exclusive(() => openProblems(false))} {@attach dismissable(() => openProblems(false), { trigger: () => button })}>
    {#if update || packs.length}
      <p class="hd">{t('Updates')}</p>
      {#if update}
        <button class="mi" onclick={() => go('updates')}><span class="dot warn"></span><span class="grow">{t('Prism {version} is out (this one: {current})', { version: update, current: updates.info!.current })}</span><span class="k">{t('Settings › General › Updates')}</span></button>
      {/if}
      {#if packs.length}
        <button class="mi" onclick={() => go('packs')}><span class="dot warn"></span><span class="grow">{tn(packs.length, '{packs} changed at its source', '{packs} changed at their sources', { packs: packs.join(', ') })}</span><span class="k">{t('Settings › General › Packs')}</span></button>
      {/if}
      <hr />
    {/if}
    <div class="head">
      <span class="hd">{t('Problems')}</span>
      <span class="acts">
        <button class="btn sm ghost" onclick={() => void copy(problemsText(nowList, lately), t('Problems copied'))} disabled={!nowList.length && !lately.length} title={t('Copy them all, to paste in an issue or a message')}><Icon name="copy" />{t('Copy')}</button>
        <button class="btn sm ghost" onclick={() => void clearProblems()} disabled={!lately.length} title={t('Forget what failed lately; what fails now stays')}><Icon name="trash" />{t('Clear')}</button>
        <button class="btn sm ghost" onclick={() => go('logs')} title={t('The whole log: Settings › General › Logs')}><Icon name="logs" />{t('Log')}</button>
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
            {#if p.settings}<button class="btn sm ghost fix" onclick={() => go(p.settings)}><Icon name="settings" />{t('Settings')}</button>{/if}
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
  .prism { flex: none; }
  .prism.open { background: var(--surface-muted); color: var(--text); }
  .pop { position: fixed; display: flex; flex-direction: column; width: 520px; max-width: calc(100vw - 24px); max-height: 60vh; font-size: var(--fs-body); }
  .pop .mi .grow { white-space: normal; }
  .head { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-right: 2px; }
  .acts { display: flex; gap: 2px; }
  .body { min-height: 0; overflow-y: auto; }
  .grp { margin: 6px 8px 2px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .prow { display: flex; align-items: flex-start; gap: 4px; border-radius: var(--r6); --sev: var(--warn); }
  .prow.ko { --sev: var(--ko); }
  .prow:hover { background: var(--surface-muted); }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; padding: 5px 8px; border: none; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .txt small { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-meta); }
  .txt small i { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--sev); }
  .prow.new .txt small { color: var(--text); }
  .msg { font-family: var(--mono); font-size: var(--fs-mono); white-space: pre-wrap; overflow-wrap: anywhere; display: -webkit-box; -webkit-line-clamp: 4; line-clamp: 4; -webkit-box-orient: vertical; overflow: hidden; }
  .prow:hover .msg { -webkit-line-clamp: unset; line-clamp: unset; }
  .fix { margin: 5px 4px 0 0; }
  .none { margin: 8px; color: var(--text-muted); }
</style>
