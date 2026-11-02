<script lang="ts">
  import { t } from '../i18n.svelte'
  import { threadAttr } from '../thread.svelte'
  import { armer } from '../armed.svelte'
  import { forgeName, mrNumber } from '../forge'
  import { conventions, isRelease, mrTickets } from '../conventions'
  import PipelineBar from './PipelineBar.svelte'
  import MrMenu from './MrMenu.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'
  import ArmedLabel from './ArmedLabel.svelte'
  import { changesRequested, nextStep, rebaseHint, securityLabel } from '../rules'
  import { cleanTitle, primaryAction } from '../mr-actions'
  import MrTitle from './MrTitle.svelte'
  import { previewClick } from '../ticket-view.svelte'
  import { ticketLink } from '../linkify'
  import { live } from '../live.svelte'
  import { ago } from '../time'
  import { forgeAvatar, initialsOf, personColor, repoColor } from '../people'
  import type { MergeRequest, Ticket } from '../types'
  import { mrKey } from '../mr-ref'
  import { blocks, isLarge, lines, sizeTitle } from '../mr-size'

  /**
   * An MR on two lines of text: the full title, and below it the context (branch,
   * author, ticket, approvals, threads, age). On the right, the pipeline at a glance, the
   * next step in plain words, the unblocking action and the ⋯ menu. A click opens the drawer.
   */
  let {
    mr, me, queueLabel, ticket, now, selected = false, onopen,
  }: {
    mr: MergeRequest
    me: string | null
    queueLabel: string | null | undefined
    ticket: Ticket | undefined
    now: number
    selected?: boolean
    onopen: () => void
  } = $props()

  const step = $derived(nextStep(mr, me, queueLabel))
  const primary = $derived(primaryAction(step.primary, mr, queueLabel))
  const mine = $derived(!!me && mr.author === me)
  const pic = $derived(forgeAvatar(mr.author_avatar))
  const queued = $derived(!!queueLabel && !!mr.labels?.includes(queueLabel))
  /** Auto-merge: always flagged, even when the displayed step is a blocker (conflicts, CI). */
  const auto = $derived(mr.state === 'opened' && mr.auto_merge)
  const target = $derived(isRelease(mr.target_branch) ? 'release' : mr.target_branch)

  let busy = $state(false)
  let error = $state<string | null>(null)
  /** An action that cannot be undone (merge) runs on a second click, within a few seconds. */
  const armed = armer()
  async function run(e: MouseEvent) {
    e.stopPropagation()
    if (!primary || !('run' in primary) || busy) return
    if (primary.confirm && !armed.confirm()) return
    armed.disarm()
    busy = true
    error = null
    try {
      await primary.run()
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }

  function onclick(e: MouseEvent) {
    // Links, buttons and the menu keep their own click.
    if ((e.target as HTMLElement).closest('a, button, [role=menu]')) return
    onopen()
  }
  function onkeydown(e: KeyboardEvent) {
    if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
      e.preventDefault()
      onopen()
    }
  }
</script>

<div class="mr {step.tone}" data-mr={mrKey(mr)} data-thread={threadAttr({ mr: { repo: mr.repo ?? null, iid: mr.iid } })} class:selected class:merged={mr.state === 'merged'} class:draft={mr.draft} class:automerge={auto} role="button" tabindex="0" {onclick} {onkeydown} title="{mrKey(mr)} · {cleanTitle(mr)}">
  <span class="dot" aria-hidden="true"></span>
  <div class="l1">
    <span class="av" class:pic={!!pic} style:--c={personColor(mr.author)} title="{mine ? t('You') : mr.author_name || mr.author} · @{mr.author}">{#if pic}<img src={pic} alt="" />{:else}{initialsOf(mr.author_name, mr.author)}{/if}</span>
    {#if mr.repo}<span class="repo" class:elsewhere={!!mr.forge} style:--c={repoColor(mr.repo)} title={mr.forge ? t('Repository {repo} ({project}) on {forge}', { repo: mr.repo, project: mr.project ?? '', forge: forgeName(mr.forge) }) : t('Repository {repo} ({project})', { repo: mr.repo, project: mr.project ?? '' })}>{#if mr.forge}<Logo name={mr.forge} size={10} />{/if}{mr.repo}</span>{/if}
    <span class="iid">{mrNumber(mr.iid, mr.forge)}</span>
    {#if securityLabel(mr)}<span class="sec" title={t('Security MR (label “{label}”)', { label: securityLabel(mr) ?? '' })} role="img" aria-label={t('Security')}><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M6 .8 1.6 2.4v3.3c0 2.7 1.9 4.8 4.4 5.5 2.5-.7 4.4-2.8 4.4-5.5V2.4Z" /></svg></span>{/if}
    {#if mr.draft}<span class="draft-tag">{t('Draft')}</span>{/if}
    <span class="t"><MrTitle {mr} /></span>
    {#if auto}<span class="chip am" title={step.tone === 'blocked' ? t('Auto-merge is set, but blocked: {why}', { why: step.label.toLowerCase() }) : t('Auto-merge is set: {forge} merges as soon as everything is green', { forge: forgeName(mr.forge) })}><Icon name="automerge" /> {t('auto-merge')}</span>{/if}
    <!-- Behind its target: flagged even when another step shows (merge queue, auto-merge, review). -->
    {#if rebaseHint(mr) && step.label !== t('Rebase needed')}<span class="rb" title={queued ? t('Behind {branch}: to rebase (the merge queue’s job rebases it when its turn comes)', { branch: mr.target_branch }) : t('Behind {branch}: to rebase', { branch: mr.target_branch })} role="img" aria-label={t('To rebase')}><Icon name="rebase" /></span>{/if}
    {#if queued && step.label !== t('In merge queue')}<span class="chip q" title={t('Label “{label}”: the CI job rebases, then auto-merges', { label: queueLabel ?? '' })}>{t('queue')}</span>{/if}
  </div>
  <div class="l2">
    <span class="mono branch">{mr.source_branch}</span>
    <span class="sep">→</span>
    <span class="into" class:main={target === conventions().main_branch} title={mr.target_branch}>{target}</span>
    <span class="sep">·</span>
    <span class:you={mine} title={mr.author_name || mr.author}>{mine ? t('you') : mr.author_name || mr.author}</span>
    {#if mr.ticket}
      <span class="sep">·</span>
      <a class="mono" href={ticket?.url ?? '#'} onclick={previewClick(mr.ticket)} title={ticket ? `${mr.ticket} · ${ticket.name} (${ticket.status})` : t('Preview {ticket}', { ticket: mr.ticket })}>{mr.ticket}</a>
      {#each mrTickets(mr).slice(1) as id (id)}{@const link = ticketLink(id, live.snapshot)}{' '}<a class="mono" href={link?.href ?? '#'} onclick={previewClick(id)} title={link?.title ?? t('Preview {ticket}', { ticket: id })}>{id}</a>{/each}
    {/if}
    {#if !mr.draft && mr.state === 'opened'}
      <span class="sep">·</span>
      <span class:ok={mr.approvals_left <= 0 && !changesRequested(mr)} title={mr.approved_by.length ? t('Approvals: {who}', { who: mr.approved_by.join(', ') }) : t('Approvals')}><Icon name="check" /> {mr.approved_by.length}/{mr.approved_by.length + Math.max(0, mr.approvals_left)}</span>
      {#if changesRequested(mr)}<span class="sep">·</span><span class="asks" title={t('Changes requested by {who}', { who: mr.changes_requested_by.join(', ') })}><Icon name="warning" /> {t('changes requested')}</span>{/if}
    {/if}
    {#if mr.open_threads}<span class="sep">·</span><span class:warn={mr.open_threads > 5} title={t('Open threads')}><Icon name="comment" /> {mr.open_threads}</span>{/if}
    {#if mr.size}
      <span class="sep">·</span>
      <span class="size mono" class:large={isLarge(mr.size)} title={sizeTitle(mr.size)}><span class="add">+{lines(mr.size.additions)}</span><span class="del">−{lines(mr.size.deletions)}</span><span class="blocks" aria-hidden="true">{#each blocks(mr.size) as b, i (i)}<i class={b}></i>{/each}</span></span>
    {/if}
    <span class="sep">·</span>
    <span>{ago(mr.state === 'merged' ? mr.merged_at : mr.updated_at)}</span>
  </div>
  <div class="pipe"><PipelineBar pipeline={mr.head_pipeline} status={mr.pipeline} {now} /></div>
  <div class="right">
    <span class="next" title={step.label}>{step.label}</span>
    {#if primary}
      {#if 'href' in primary}
        <a class="act" href={primary.href} title={primary.title}><Logo name="agent" size={10} />{primary.label}</a>
      {:else}
        <button class="act" class:error={!!error} class:armed={armed.is()} disabled={busy} onclick={run} title={error ? t('Failed: {error}', { error }) : armed.is() ? t('Click again to confirm') : primary.title}>{#if busy}…{:else if primary.confirm}<ArmedLabel label={primary.label} armedLabel={primary.confirm} armed={armed.is()} />{:else}{primary.label}{/if}</button>
      {/if}
    {/if}
    <MrMenu {mr} {queueLabel} {me} />
  </div>
</div>

<style>
  /* The author at a glance: their initials on their colour. */
  .av { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 18px; height: 18px; border-radius: var(--r-avatar); background: var(--c); color: #fff; font-size: 8.5px; font-weight: 700; letter-spacing: 0.2px; }
  /* Their profile picture, when the forge has one. */
  .av.pic { overflow: hidden; }
  .av.pic img { width: 100%; height: 100%; object-fit: cover; }
  .mr {
    display: grid; grid-template-columns: 10px minmax(0, 1fr) 190px auto; gap: 0 10px; align-items: center;
    grid-template-areas: 'dot l1 pipe right' 'dot l2 pipe right';
    padding: calc(3px * var(--space)) 10px; border-bottom: 1px solid var(--border); line-height: 1.35; cursor: pointer;
    --c: var(--text-muted);
  }
  /* A phone (the remote): the pipeline under the title, the next step beside it. */
  @media (max-width: 560px) {
    .mr { grid-template-columns: 10px minmax(0, 1fr) auto; grid-template-areas: 'dot l1 right' 'dot l2 l2' 'dot pipe pipe'; gap: 2px 8px; }
  }
  .mr:hover { background: rgba(127, 127, 127, 0.06); }
  .mr.selected { background: color-mix(in srgb, var(--src-gitlab) 9%, transparent); }
  .mr:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  /* The colour only tells the step: red blocked, orange to do, blue waiting, green ready, purple automatic. */
  .mr.blocked { --c: var(--ko); } .mr.todo { --c: var(--warn); } .mr.wait { --c: var(--info); } .mr.ready { --c: var(--ok); } .mr.auto { --c: var(--accent); }
  .mr.merged, .mr.draft { opacity: 0.72; }
  .dot { grid-area: dot; width: 8px; height: 8px; border-radius: var(--r-dot); background: color-mix(in srgb, var(--c) var(--dot-fill), transparent); box-shadow: inset 0 0 0 var(--dot-ring) var(--c), 0 0 var(--dot-glow) var(--c); }
  .l1 { grid-area: l1; display: flex; align-items: baseline; gap: 6px; min-width: 0; font-size: var(--fs-body); }
  /* Non-primary repo: its badge, in its colour, before the number. */
  .repo { flex-shrink: 0; padding: 0 5px; border-radius: var(--r4); background: color-mix(in srgb, var(--c) 16%, transparent); color: var(--c); font-size: var(--fs-tag); font-weight: 600; }
  /* On the other forge than the profile's: its logo first. */
  .repo.elsewhere { display: inline-flex; align-items: center; gap: 3px; }
  .iid { flex-shrink: 0; font-family: var(--mono); font-size: var(--fs-meta); color: var(--info); }
  .t { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }
  .sec { display: inline-flex; align-self: center; color: var(--ko); }
  .sec svg { width: 11px; height: 11px; fill: currentColor; }
  .draft-tag { flex-shrink: 0; padding: 0 4px; border: 1px dashed currentColor; border-radius: var(--r4); font-size: 9px; font-weight: 600; letter-spacing: 0.4px; text-transform: uppercase; color: var(--text-muted); }
  .chip { flex-shrink: 0; padding: 0 5px; border-radius: var(--r4); font-size: var(--fs-tag); }
  .chip.am { background: var(--accent); color: var(--surface); font-weight: 600; }
  .mr.automerge { box-shadow: inset 3px 0 0 var(--accent); }
  .rb { display: inline-flex; flex-shrink: 0; align-self: center; color: var(--warn); }
  .chip.q { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
  .l2 { grid-area: l2; display: flex; align-items: center; gap: 5px; min-width: 0; overflow: hidden; white-space: nowrap; font-size: var(--fs-tag); color: var(--text-muted); }
  .l2 > * { flex-shrink: 0; }
  .l2 .branch { flex-shrink: 1; min-width: 40px; overflow: hidden; text-overflow: ellipsis; font-size: var(--fs-tag); }
  .l2 .sep { opacity: 0.45; }
  .l2 a { color: var(--info); font-size: var(--fs-tag); }
  .into.main { color: var(--warn); font-weight: 600; }
  .you { color: var(--text); }
  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .asks { display: inline-flex; align-items: center; gap: 3px; color: var(--warn); font-weight: 500; }
  .mono { font-family: var(--mono); }
  /* Diff size: added and removed lines, and their share in five cells. */
  .size { display: inline-flex; align-items: center; gap: 3px; font-size: 9.5px; font-variant-numeric: tabular-nums; }
  /* Beyond `[alerts] large_mr_lines`: the size stands out in orange. */
  .size.large { padding: 0 4px; border-radius: var(--r4); background: var(--warn-bg); }
  .size .add { color: var(--ok); }
  .size .del { color: var(--ko); }
  .blocks { display: inline-flex; gap: 1px; }
  .blocks i { width: 5px; height: 5px; border-radius: var(--r1); background: var(--surface-muted); }
  .blocks i.add { background: var(--ok); }
  .blocks i.del { background: var(--ko); }
  .pipe { grid-area: pipe; min-width: 0; }
  .right { grid-area: right; display: flex; align-items: center; justify-content: flex-end; gap: 6px; }
  .next { padding: 0 7px; border-radius: var(--r10); font-size: var(--fs-tag); font-weight: 500; line-height: 18px; white-space: nowrap; background: color-mix(in srgb, var(--c) 15%, transparent); color: color-mix(in srgb, var(--c) 70%, var(--text)); }
  .mr.idle .next { background: var(--surface-muted); color: var(--text-muted); }
  .act { display: inline-flex; align-items: center; gap: 4px; padding: 0 8px; border: 1px solid color-mix(in srgb, var(--c) 60%, var(--border)); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; white-space: nowrap; cursor: pointer; }
  .act:hover { background: color-mix(in srgb, var(--c) 12%, var(--surface)); text-decoration: none; }
  .act.error { border-color: var(--ko); color: var(--ko); }
  .act.armed { background: var(--warn-bg); border-color: var(--warn); color: var(--warn); }
  .act:disabled { cursor: progress; opacity: 0.6; }
</style>
