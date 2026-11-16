<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../Icon.svelte'
  import State from '../ui/State.svelte'
  import { t, tc } from '../../i18n.svelte'
  import { locale } from '../../locale'
  import { openLink } from '../../links'
  import { feedback, loadFeedback, loadRequests, markSeen, openFeedback, requests, shipped, type FeedbackRequest } from '../../feedback.svelte'

  /**
   * Settings › General › Feedback: where feedback goes (Prism's repository, the packs'), and the
   * issues opened from Prism with their state, read again on demand. Looking at them clears the
   * status bar's "done".
   */
  let filter = $state<'all' | 'open'>('all')
  const shown = $derived(requests.list.filter((r) => filter === 'all' || r.state === 'open'))
  onMount(() => {
    void loadFeedback()
    void loadRequests(true).then(markSeen)
  })
  const status = (r: FeedbackRequest): 'open' | 'done' | 'not planned' => (r.state === 'open' ? 'open' : shipped(r) ? 'done' : 'not planned')
  const day = (iso: string) => new Date(iso).toLocaleDateString(locale(), { day: 'numeric', month: 'short' })
</script>

<div class="card">
  <div class="head">
    <b>{t('Where it goes')}</b>
    <button class="btn primary" onclick={() => openFeedback({ where: 'Settings › Feedback' })} disabled={!feedback.info?.targets.length}><Icon name="comment" />{t('Send feedback…')}</button>
  </div>
  {#each feedback.info?.targets ?? [] as tg (tg.id)}
    <div class="row"><b>{tg.name}</b><code>{tg.repo}</code><span class="muted">{tg.why}</span></div>
  {:else}
    <p class="muted pad">{t('No repository: Prism’s is [updates] source (or Prism’s repository below), a pack’s is the one it was imported from.')}</p>
  {/each}
</div>

<div class="card">
  <div class="head">
    <b>{t('My requests')}</b>
    <span class="seg">
      <button class:on={filter === 'all'} onclick={() => (filter = 'all')}>{t('All')}</button>
      <button class:on={filter === 'open'} onclick={() => (filter = 'open')}>{tc('state', 'Open')}</button>
    </span>
    <button class="btn sm" class:busy={requests.loading} onclick={() => loadRequests(true)} disabled={requests.loading} title={t('Read their state again')}>{#if requests.loading}<span class="spinner"></span>{t('Reading…')}{:else}{t('Refresh')}{/if}</button>
  </div>
  {#if requests.error}<State kind="error" inline title={requests.error} retry={() => loadRequests(true)} />{/if}
  {#each shown as r (r.repo + r.number)}
    <div class="req">
      <code>#{r.number}</code>
      <a href={r.url} onclick={(e) => (e.preventDefault(), openLink(r.url))}>{r.title}</a>
      <span class="muted">{r.name} · {day(r.opened)}{#if r.votes}{' '}· 👍 {r.votes}{/if}</span>
      <span class="pill" class:info={status(r) === 'open'} class:ok={status(r) === 'done'}>{tc('request', status(r))}</span>
    </div>
  {:else}
    <p class="muted pad">{requests.list.length ? t('None open.') : t('None yet: the issues opened from Prism come here.')}</p>
  {/each}
</div>

<style>
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .head { display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .head b { flex: 1; font-weight: 550; }
  .row, .req { display: flex; align-items: baseline; gap: 10px; padding: 7px 14px; border-bottom: 1px solid var(--border); font-size: var(--fs-body); }
  .row:last-child, .req:last-child { border-bottom: 0; }
  code { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); }
  .row .muted { margin-left: auto; }
  .req a { flex: 1; min-width: 0; overflow: hidden; color: var(--text); text-decoration: none; text-overflow: ellipsis; white-space: nowrap; }
  .req a:hover { color: var(--accent); }
  .pad { margin: 0; padding: 10px 14px; }
</style>
