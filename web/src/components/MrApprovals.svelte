<script lang="ts">
  import { t, tn } from '../i18n.svelte'
  import Icon from './Icon.svelte'
  import type { IconName } from '../icons'
  import { ago } from '../time'
  import { locale } from '../locale'
  import type { ApprovalEvent } from '../mr-review.svelte'

  /**
   * An MR's approval history, oldest first: approvals, approvals taken back or dismissed, requests
   * for changes, and the pushes between them. An approval given before a later push is marked: the
   * code it approved has changed since. Only the last steps show until "earlier" is asked for.
   */
  let { events, me }: { events: ApprovalEvent[]; me: string | null } = $props()

  const SHOWN = 6
  let all = $state(false)
  const lastPush = $derived(events.findLastIndex((e) => e.kind === 'pushed'))
  const start = $derived(all ? 0 : Math.max(0, events.length - SHOWN))

  const who = (login: string, name: string) => (!login ? t('Someone') : login === me ? t('You') : name || login)
  function line(e: ApprovalEvent): { icon: IconName; tone: string; text: string } {
    const by = who(e.author, e.author_name)
    const mine = !!e.author && e.author === me
    switch (e.kind) {
      case 'approved': return { icon: 'check', tone: 'ok', text: mine ? t('You approved') : t('{who} approved', { who: by }) }
      case 'unapproved': return { icon: 'undo', tone: 'muted', text: mine ? t('You took back your approval') : t('{who} took back their approval', { who: by }) }
      case 'changes_requested': return { icon: 'warning', tone: 'warn', text: mine ? t('You requested changes') : t('{who} requested changes', { who: by }) }
      case 'dismissed': return { icon: 'close', tone: 'muted', text: mine ? t('You dismissed a review') : !e.whom ? t('{who} dismissed a review', { who: by }) : e.whom === me ? t('{who} dismissed your review', { who: by }) : t('{who} dismissed {whom}’s review', { who: by, whom: e.whom.replace(/^@/, '') }) }
      case 'reset': return { icon: 'retry', tone: 'warn', text: e.whom ? t('Approvals reset by a push ({whom})', { whom: e.whom }) : t('Approvals reset by a push') }
      case 'pushed': return { icon: 'branch', tone: 'muted', text: e.commits ? (mine ? tn(e.commits, 'You pushed {n} commit', 'You pushed {n} commits') : tn(e.commits, '{who} pushed {n} commit', '{who} pushed {n} commits', { who: by })) : mine ? t('You force-pushed') : t('{who} force-pushed', { who: by }) }
    }
  }
  const when = (at: string) => new Date(at).toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' })
</script>

{#if events.length}
  <ol class="history">
    {#if start > 0}<li class="more"><button onclick={() => (all = true)}>{tn(start, '{n} earlier step', '{n} earlier steps')}</button></li>{/if}
    {#each events.slice(start) as e, n (start + n)}
      {@const l = line(e)}
      {@const stale = e.kind === 'approved' && start + n < lastPush}
      <li class={l.tone} class:push={e.kind === 'pushed'}>
        <span class="ic"><Icon name={l.icon} /></span>
        <span class="tx">{l.text}{#if stale}<span class="tag" title={t('Commits were pushed after this approval')}>{t('before the last push')}</span>{/if}</span>
        <time datetime={e.at} title={when(e.at)}>{t('{ago} ago', { ago: ago(e.at) })}</time>
      </li>
    {/each}
  </ol>
{:else}
  <p class="muted">{t('No approval yet.')}</p>
{/if}

<style>
  .history { list-style: none; margin: 0; padding: 0 0 0 4px; font-size: var(--fs-body); }
  li { position: relative; display: flex; align-items: center; gap: 8px; padding: 3px 0 3px 18px; }
  /* The thread joining the steps. */
  li::before { content: ''; position: absolute; left: 6px; top: 0; bottom: 0; width: 1px; background: var(--border); }
  li:first-child::before { top: 50%; }
  li:last-child::before { bottom: 50%; }
  .ic { position: absolute; left: 0; display: inline-flex; width: 13px; height: 13px; align-items: center; justify-content: center; border-radius: 50%; background: var(--surface); }
  .ic :global(svg) { width: 11px; height: 11px; }
  .ok .ic { color: var(--ok); }
  .warn .ic { color: var(--warn); }
  .muted .ic, .muted .tx { color: var(--text-muted); }
  .push .tx { font-size: var(--fs-meta); }
  .tx { flex: 1; min-width: 0; }
  .tag { margin-left: 6px; padding: 0 5px; border-radius: var(--r4); background: var(--warn-bg); color: var(--warn); font-size: var(--fs-tag); }
  time { flex-shrink: 0; color: var(--text-muted); font-size: var(--fs-meta); white-space: nowrap; }
  .more button { padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .more::before { border-left: 1px dashed var(--border); background: none; }
  p.muted { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
