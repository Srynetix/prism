<script lang="ts">
  import { t, tn, tc } from '../i18n.svelte'
  import { clock } from '../clock.svelte'
  import { locale } from '../locale'
  import { offers } from '../tracker'
  import { untrack } from 'svelte'
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { live } from '../live.svelte'
  import { closeMr, mrView } from '../mr-view.svelte'
  import { claudeClick, mrActions } from '../claude-actions'
  import { nextStep, rebaseHint, securityLabel } from '../rules'
  import { cleanTitle, timeDescription } from '../mr-actions'
  import { pipelineSpan, stageTone } from '../pipeline'
  import { ago, elapsed, span } from '../time'
  import { previewClick } from '../ticket-view.svelte'
  import { canShowLogs, logTarget, logsClick, openPipeline } from '../logs.svelte'
  import { openTimeDraft } from '../timeDraft.svelte'
  import { setDraft } from '../pipeline-actions'
  import { hasMergeQueue } from '../repositories'
  import { pref, setPref } from '../layout.svelte'
  import CancelPipeline from './CancelPipeline.svelte'
  import RetryPipeline from './RetryPipeline.svelte'
  import RunPipeline from './RunPipeline.svelte'
  import RetryJob from './RetryJob.svelte'
  import StageManualJobs from './StageManualJobs.svelte'
  import RebaseButton from './RebaseButton.svelte'
  import AutoMergeButton from './AutoMergeButton.svelte'
  import MergeQueueButton from './MergeQueueButton.svelte'
  import MergePriorityPicker from './MergePriorityPicker.svelte'
  import type { MergeRequest, Ticket } from '../types'
  import Markdown from './Markdown.svelte'
  import MrTitle from './MrTitle.svelte'
  import MrThread from './MrThread.svelte'
  import MrDiffView from './MrDiffView.svelte'
  import MrApprovals from './MrApprovals.svelte'
  import CommentBox from './CommentBox.svelte'
  import Icon from './Icon.svelte'
  import PeoplePicker from './PeoplePicker.svelte'
  import LabelPicker from './LabelPicker.svelte'
  import { labelPick, pickLabels } from '../mr-labels.svelte'
  import { peoplePick, pickPeople, setMrPeople, type PeopleKind } from '../mr-people.svelte'
  import { hasCore } from '../bridge'
  import { approve, canReview, comment, commentLine, loadCommits, loadDetail, loadDiff, mrFile, mrImage, projectWeb, requestChanges, resolveThread, review } from '../mr-review.svelte'
  import { mrKey, sameMr, type MrRef } from '../mr-ref'
  import { isLarge, lines } from '../mr-size'
  import { alertSettings, mrTickets } from '../conventions'
  import { keys } from '../platform'
  import { forgeName, meOn, mrNumber, onGithub } from '../forge'
  import { actorName } from '../agent-cli'
  import ActionSheet from './ActionSheet.svelte'
  import { flatten, mrMenu, type FlatAction } from '../menus'
  import { primaryAction } from '../mr-actions'
  import { armer } from '../armed.svelte'
  import { copy as copyText, toast, writeClipboard } from '../toast.svelte'

  /**
   * An MR's details in a side drawer, like tickets and logs: summary, pipeline
   * (stages, failed jobs, logs, retries), merge (merge queue, auto-merge, rebase) and
   * Claude actions. Read from the snapshot: it follows the MR live. Escape closes.
   * Changes (diff) and Discussion (threads, comments) tabs; description and approval read
   * on demand (mr-review.svelte.ts).
   * `phone` (the remote): the whole screen, Back to the list; the facts as tiles; at the bottom the
   * main action of the moment (its next step's, as on its row) and Actions, a sheet with the rest.
   */
  let { phone = false }: { phone?: boolean } = $props()
  const snap = $derived(live.snapshot)
  const m = $derived(mrView.ref ? snap?.merge_requests.find((x) => sameMr(x, mrView.ref!)) : undefined)
  const queueLabel = $derived(snap?.merge_queue_label)
  /** My login on its forge. */
  const me = $derived(m ? meOn(snap, m) : null)
  const step = $derived(m && snap ? nextStep(m, me, queueLabel) : null)
  /** Every ticket it names, each with what Prism knows of it. */
  const tickets = $derived.by((): { id: string; ticket?: Ticket }[] => {
    if (!m || !snap) return []
    const all = [...(snap.sprint?.tickets ?? []), ...(snap.pickup?.boards.flatMap((b) => b.tickets) ?? []), ...(snap.refinement?.boards.flatMap((b) => b.tickets) ?? [])]
    return mrTickets(m).map((id) => ({ id, ticket: all.find((t) => t.id === id) }))
  })
  const who = (u: string) => (u === me ? 'you' : u)

  /** Assignee or reviewer picker, opened on its row (or from the MR's ⋯ menu). */
  let assignBtn = $state<HTMLButtonElement>()
  let reviewBtn = $state<HTMLButtonElement>()
  const picking = $derived(peoplePick.ref && mrView.ref && sameMr(peoplePick.ref, mrView.ref) ? peoplePick.kind : null)
  const closePicker = () => (peoplePick.kind = null)
  /** Assign myself in one click, on top of the existing assignees. */
  let assigning = $state(false)
  let assignError = $state('')
  async function assignMe(mr: MrRef, current: string[]) {
    if (!me || assigning) return
    assigning = true
    assignError = ''
    try {
      await setMrPeople(mr, 'assignees', [...current, me])
    } catch (e) {
      assignError = String(e)
    } finally {
      assigning = false
    }
  }
  const togglePicker = (kind: PeopleKind) => (picking === kind ? closePicker() : !!mrView.ref && pickPeople(mrView.ref, kind))
  /** Label picker, opened on the Labels row (or from the MR's ⋯ menu). */
  let labelBtn = $state<HTMLButtonElement>()
  const labeling = $derived(labelPick.open && !!labelPick.ref && !!mrView.ref && sameMr(labelPick.ref, mrView.ref))
  const closeLabels = () => (labelPick.open = false)
  const toggleLabels = () => (labeling ? closeLabels() : !!mrView.ref && pickLabels(mrView.ref))

  type Tab = 'overview' | 'changes' | 'commits' | 'discussion'
  let tab = $state<Tab>('overview')
  /** Thread to show in the diff (from the Discussion tab). */
  let focus = $state<{ path: string; line: number | null } | null>(null)
  const detail = $derived(m ? review.detail[mrKey(m)] : undefined)
  const diff = $derived(m ? review.diff[mrKey(m)] : undefined)
  const commits = $derived(m ? review.commits[mrKey(m)] : undefined)
  const stamp = (iso: string) => new Date(iso).toLocaleString(locale(), { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
  const threads = $derived(detail?.data?.discussions ?? [])
  const open = $derived(threads.filter((d) => d.resolvable && !d.resolved).length)
  // Read on display, re-read when the MR changes.
  $effect(() => {
    if (!m) return
    const ref = { iid: m.iid, repo: m.repo }
    const updated_at = m.updated_at
    const withDiff = tab === 'changes'
    const withCommits = tab === 'commits'
    // Loading reads and writes the store: untracked, otherwise an error would retrigger the read in a loop.
    untrack(() => {
      loadDetail(ref, updated_at)
      if (withDiff) loadDiff(ref, updated_at)
      if (withCommits) loadCommits(ref, updated_at)
    })
  })
  const reply = (discussion: string, body: string) => comment(m!, body, m!.updated_at, discussion)
  const resolve = (discussion: string, resolved: boolean) => resolveThread(m!, discussion, resolved, m!.updated_at)
  const web = $derived(m ? projectWeb(m.url) : undefined)
  function locate(path: string, line: number | null) {
    focus = { path, line }
    tab = 'changes'
  }

  let draftBusy = $state(false)
  let draftError = $state<string | null>(null)
  async function toggleDraft() {
    if (!m || draftBusy) return
    draftBusy = true
    draftError = null
    try {
      await setDraft(m, !m.draft)
    } catch (e) {
      draftError = String(e)
    } finally {
      draftBusy = false
    }
  }

  let approving = $state(false)
  let approveError = $state<string | null>(null)
  async function toggleApproval() {
    if (!m || !detail?.data || approving) return
    approving = true
    approveError = null
    try {
      await approve(m, !detail.data.user_has_approved, m.updated_at)
    } catch (e) {
      approveError = String(e)
    } finally {
      approving = false
    }
  }

  /**
   * Request changes: a comment saying which first (GitHub requires one, GitLab posts it before
   * setting my state), then the request; on GitLab, a click on "Changes requested" takes it back.
   */
  let asking = $state(false)
  let withdrawing = $state(false)
  let withdrawError = $state<string | null>(null)
  async function askChanges(body: string) {
    if (!m) return
    await requestChanges(m, true, body, m.updated_at)
  }
  async function toggleChanges() {
    if (!m || !detail?.data || withdrawing) return
    if (!detail.data.user_requested_changes) {
      asking = !asking
      return
    }
    withdrawing = true
    withdrawError = null
    try {
      await requestChanges(m, false, '', m.updated_at)
    } catch (e) {
      withdrawError = String(e)
    } finally {
      withdrawing = false
    }
  }
  // Another MR shown: no form left open.
  $effect(() => {
    void mrView.ref
    asking = false
    withdrawError = null
  })

  const now = $derived(m ? clock(1000).now : Date.now())

  let copied = $state(false)
  async function copy(text: string) {
    try {
      await writeClipboard(text)
      copied = true
      setTimeout(() => (copied = false), 1500)
    } catch {
      copied = false
    }
  }

  /** Changes tab in full screen: ⤢ or "f"; remembered under `diff.full`. */
  let full = $state(pref<boolean>('diff.full', false))
  function toggleFull() {
    full = !full
    setPref('diff.full', full)
  }

  function onKey(e: KeyboardEvent) {
    if (!mrView.ref) return
    const typing = !!(e.target as HTMLElement | null)?.closest?.('textarea, input, select, [contenteditable]')
    if (e.key === 'f' && tab === 'changes' && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      e.stopImmediatePropagation()
      toggleFull()
      return
    }
    if (e.key !== 'Escape') return
    // Escape in a comment in progress cancels it (CommentBox) without closing the drawer.
    if (typing) return
    e.stopImmediatePropagation()
    closeMr()
  }
  const TONE: Record<string, string> = { success: 'ok', failed: 'ko', running: 'run', pending: 'run', created: 'run' }

  /* ---------- The phone's action bar and sheet ---------- */

  const canApprove = $derived(m?.state === 'opened' && !!detail?.data && (detail.data.user_can_approve || detail.data.user_has_approved))
  /** Its next step's action (as on its row); failing that, an approval to give. */
  const main = $derived.by((): { label: string; confirm?: string; run: () => unknown } | null => {
    if (!m || !step) return null
    const p = primaryAction(step.primary, m, queueLabel)
    if (p) return 'href' in p ? { label: p.label, run: () => window.open(p.href, '_blank', 'noopener') } : p
    if (canApprove && !detail?.data?.user_has_approved) return { label: t('Approve'), run: toggleApproval }
    return null
  })
  let sheet = $state(false)
  const sheetActions = $derived.by((): FlatAction[] => {
    if (!m) return []
    const own: FlatAction[] = []
    if (canApprove && !(onGithub(m) && detail?.data?.user_has_approved)) own.push({ key: 'approve', label: detail?.data?.user_has_approved ? t('Revoke my approval') : t('Approve'), group: t('Review'), run: async () => { await toggleApproval(); if (approveError) throw approveError } })
    if (canApprove && !detail?.data?.user_requested_changes) own.push({ key: 'changes', label: t('Request changes…'), group: t('Review'), run: () => { tab = 'overview'; asking = true } })
    // The MR's actions first, its links after them; filtering the dashboard means nothing here.
    const menu = flatten(mrMenu(m, queueLabel, me)).filter((a) => a.key !== 'filter')
    const links = menu.filter((a) => !a.group).map((a) => ({ ...a, group: t('Link') }))
    return [...own, ...menu.filter((a) => a.group), ...links]
  })
  const armedMain = armer()
  let mainBusy = $state(false)
  async function runMain() {
    if (!main || mainBusy) return
    if (main.confirm && !armedMain.confirm()) return
    armedMain.disarm()
    mainBusy = true
    try {
      await main.run()
    } catch (e) {
      toast(String(e), 6000)
    } finally {
      mainBusy = false
    }
  }
  // Another MR: no sheet left open.
  $effect(() => {
    void mrView.ref
    sheet = false
  })

  let drawerEl: HTMLElement | undefined = $state()
  /** A click outside the drawer closes it (see drawers.ts). */
  function onOutside(e: PointerEvent) {
    if (!!mrView.ref && clickedOutside(e, drawerEl)) {
      closeMr()
    }
  }
</script>

{#snippet assignees(m: MergeRequest)}
            {#each m.assignees ?? [] as u, i (u)}{i ? ', ' : ''}<span>{who(u)}</span>{:else}<span class="muted">{t('none')}</span>{/each}
            {#if hasCore() && m.state === 'opened'}
              {#if me && !(m.assignees ?? []).includes(me)}<button class="pick" disabled={assigning} onclick={() => assignMe(m, m.assignees ?? [])} title={assignError ? t('Could not assign: {error}', { error: assignError }) : t('Add me to the assignees')} class:err={!!assignError}>{assigning ? t('Assigning…') : t('Assign me')}</button>{/if}
              <button class="pick" bind:this={assignBtn} class:on={picking === 'assignees'} onclick={() => togglePicker('assignees')} title={t('Choose who is assigned')}>{t('Edit')}</button>
            {/if}
          {/snippet}

{#snippet labels(m: MergeRequest)}
            {#each m.labels ?? [] as l (l)}<span class="label" class:sec-label={l === securityLabel(m)} class:q-label={l === queueLabel}>{l}</span>{' '}{:else}<span class="muted">{t('none')}</span>{/each}
            {#if hasCore() && m.state === 'opened'}<button class="pick" bind:this={labelBtn} class:on={labeling} onclick={toggleLabels} title={t('Add or remove labels')}>{t('Edit')}</button>{/if}
          {/snippet}

{#snippet reviewers(m: MergeRequest)}
            <!-- The reviewers, then those who approved without being asked: one list, "none" only when both are empty. -->
            {#each [...m.reviewers, ...m.approved_by.filter((u) => !m.reviewers.includes(u))] as r, i (r)}{@const ok = m.approved_by.includes(r)}{@const asks = !ok && !!detail?.data?.changes_requested_by.includes(r)}{i ? ', ' : ''}<span class:ok class:ko={asks} title={asks ? t('Requested changes') : undefined}>{who(r)}{#if ok}{' '}<Icon name="check" />{:else if asks}{' '}<Icon name="warning" />{/if}</span>{:else}<span class="muted">{t('none')}</span>{/each}
            {#if m.approvals_left > 0}<span class="muted"> · {m.approvals_left} approval{m.approvals_left > 1 ? 's' : ''} left</span>{/if}
            {#if hasCore() && m.state === 'opened'}<button class="pick" bind:this={reviewBtn} class:on={picking === 'reviewers'} onclick={() => togglePicker('reviewers')} title={t('Choose who reviews')}>{t('Edit')}</button>{/if}
          {/snippet}

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if !!mrView.ref}
  <aside class="drawer" class:phone class:wide={tab === 'changes' && !phone} class:full={tab === 'changes' && full} bind:this={drawerEl} data-drawer="49" data-pinned={isPinned('mr')} aria-label={t('Merge request {mr}', { mr: mrKey(mrView.ref!) })}>
    <header>
      {#if phone}<button class="back" onclick={closeMr}><Icon name="chevronLeft" />MRs</button>{/if}
      <span class="iid">{mrKey(mrView.ref!)}</span>
      {#if step}<span class="next {step.tone}">{step.label}</span>{/if}
      <span class="grow"></span>
      {#if !phone && m?.state === 'opened' && detail?.data && (detail.data.user_can_approve || detail.data.user_has_approved)}
        <button
          class="approve"
          class:on={detail.data.user_has_approved}
          class:error={!!approveError}
          disabled={approving || (onGithub(m) && detail.data.user_has_approved)}
          onclick={toggleApproval}
          title={approveError ? t('Failed: {error}', { error: approveError }) : detail.data.user_has_approved ? (onGithub(m) ? t('You approved · GitHub cannot take it back: dismiss your review there') : t('You approved · click to revoke your approval')) : t('Approve this merge request')}
        >{#if approving}…{:else if detail.data.user_has_approved}<Icon name="check" /> {t('Approved')}{:else}{t('Approve')}{/if}</button>
      {/if}
      {#if !phone && m?.state === 'opened' && detail?.data && (detail.data.user_can_approve || detail.data.user_has_approved || detail.data.user_requested_changes)}
        <button
          class="changes"
          class:on={detail.data.user_requested_changes || asking}
          class:error={!!withdrawError}
          disabled={withdrawing || (onGithub(m) && detail.data.user_requested_changes)}
          onclick={toggleChanges}
          aria-expanded={detail.data.user_requested_changes ? undefined : asking}
          title={withdrawError ? t('Failed: {error}', { error: withdrawError }) : detail.data.user_requested_changes ? (onGithub(m) ? t('You requested changes · GitHub cannot take it back: dismiss your review there') : t('You requested changes · click to take your request back')) : t('Request changes, with a comment saying which')}
        >{#if withdrawing}…{:else if detail.data.user_requested_changes}<Icon name="warning" /> {t('Changes requested')}{:else}{t('Request changes')}{/if}</button>
      {/if}
      {#if m && !phone}<button class="icon" onclick={() => copyText(m.url, t('Link to {name} copied', { name: mrNumber(m.iid, m.forge) }))} title={t('Copy link')} aria-label={t('Copy link')}><Icon name="copy" /></button>{/if}
      {#if m}<a class="icon" href={m.url} target="_blank" rel="noreferrer" title={t('Open in {forge}', { forge: forgeName(m?.forge) })} aria-label={t('Open in {forge}', { forge: forgeName(m?.forge) })}><Icon name="external" /></a>{/if}
      {#if tab === 'changes' && !phone}<button class="icon" onclick={toggleFull} title={full ? t('Exit full screen (f)') : t('Full screen (f)')} aria-label={t('Full screen')} aria-pressed={full}><Icon name={full ? 'shrink' : 'expand'} /></button>{/if}
      {#if !phone}<PinButton kind="mr" />{/if}
      {#if !phone}<button class="icon" onclick={closeMr} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>{/if}
    </header>
    {#if m && asking && detail?.data}
      <div class="ask">
        <CommentBox forge={web} label={t('Request changes')} accent="var(--warn)" autofocus placeholder={keys(t('What should change? (⌘↵ to request, Esc to cancel)'))} onsend={askChanges} oncancel={() => (asking = false)} />
      </div>
    {/if}
    {#if m}
      <nav class="tabs" aria-label={t('Merge request sections')}>
        <button class:on={tab === 'overview'} onclick={() => (tab = 'overview')}>{t('Overview')}</button>
        <button class:on={tab === 'changes'} onclick={() => (tab = 'changes')}>{tc('mrs', 'Changes')}{#if diff?.data}<b>{diff.data.files.length}</b>{/if}</button>
        <button class:on={tab === 'commits'} onclick={() => (tab = 'commits')}>{t('Commits')}{#if commits?.data}<b>{commits.data.length}</b>{/if}</button>
        <button class:on={tab === 'discussion'} onclick={() => (tab = 'discussion')}>{t('Discussion')}{#if threads.length}<b>{threads.length}</b>{/if}{#if open}<span class="open" title={t('Unresolved threads')}>{t('{n} open', { n: open })}</span>{/if}</button>
      </nav>
    {/if}
    <div class="body">
      {#if m && tab !== 'overview' && !canReview()}
        <p class="muted">{t('Changes, commits and discussions are read from {forge} by the app, not in the browser preview.', { forge: forgeName(m?.forge) })}</p>
      {:else if m && tab === 'changes'}
        {#if diff?.data && detail?.data}
          <MrDiffView {phone} iid={mrKey(m)} diff={diff.data} discussions={threads} me={me} onreply={reply} onresolve={resolve} oncomment={(line, body) => commentLine(m, line, body, m.updated_at)} {focus} forge={web} source={(path) => mrFile(m, path)} image={(path, old) => mrImage(m, path, old)} />
        {:else if diff?.error}<p class="error">{diff.error}</p>
        {:else}<p class="muted">{t('Loading the changes…')}</p>{/if}
      {:else if m && tab === 'commits'}
        {#if commits?.data}
          <ul class="commits">
            {#each commits.data as c (c.sha)}
              <li>
                {#if c.url}<a class="mono sha" href={c.url} target="_blank" rel="noreferrer" title={t('Open {sha} in {forge}', { sha: c.sha.slice(0, 8), forge: forgeName(m.forge) })}>{c.sha.slice(0, 8)}</a>{:else}<span class="mono sha">{c.sha.slice(0, 8)}</span>{/if}
                <span class="subject" title={c.title}>{c.title}</span>
                <span class="cmeta" title={c.at ? stamp(c.at) : undefined}>{c.author}{#if c.at}{' '}· {ago(c.at)}{/if}</span>
              </li>
            {:else}
              <li class="muted">{t('No commit.')}</li>
            {/each}
          </ul>
        {:else if commits?.error}<p class="error">{commits.error}</p>
        {:else}<p class="muted">{t('Loading the commits…')}</p>{/if}
      {:else if m && tab === 'discussion'}
        {#if detail?.data}
          <div class="threads">
            {#each threads as d (d.id)}
              <MrThread discussion={d} me={me} forge={web} onreply={(body) => reply(d.id, body)} onresolve={(r) => resolve(d.id, r)} onlocate={d.path ? () => locate(d.path!, d.line) : undefined} />
            {:else}
              <p class="muted">{t('No comment yet.')}</p>
            {/each}
            <div class="compose" class:sticky={phone}><CommentBox forge={web} onsend={(body) => comment(m, body, m.updated_at)} /></div>
          </div>
        {:else if detail?.error}<p class="error">{detail.error}</p>
        {:else}<p class="muted">{t('Loading the discussion…')}</p>{/if}
      {:else if !m}
        <p class="muted">{t('This merge request is no longer in the dashboard.')}</p>
      {:else}
        <h2>
          {#if securityLabel(m)}<span class="sec" title={t('Security MR (label “{label}”)', { label: securityLabel(m) ?? '' })}><Icon name="security" /></span>{/if}
          {#if m.draft}<span class="draft-tag">{t('Draft')}</span>{/if}<MrTitle mr={m} />
        </h2>
        {#if phone}
          <div class="branch"><code>{m.source_branch}</code><span class="muted">→</span><code>{m.target_branch}</code><button class="copy" onclick={() => copy(m.source_branch)} title={t('Copy the branch name')}><Icon name={copied ? 'check' : 'copy'} /></button></div>
          <div class="tiles">
            <div class="tile"><small>{t('Author')}</small><b>{m.author === me ? t('you') : m.author_name || m.author}</b></div>
            {#if m.size}<div class="tile"><small>{t('Size')}</small><b class="mono"><span class="add">+{lines(m.size.additions)}</span> <span class="del">−{lines(m.size.deletions)}</span> · {tn(m.size.files, '{n} file', '{n} files')}</b></div>{/if}
            <div class="tile wide"><small>{t('Reviewers')}</small><div>{@render reviewers(m)}</div></div>
            <div class="tile wide"><small>{t('Assignees')}</small><div>{@render assignees(m)}</div></div>
            <div class="tile"><small>{t('Threads')}</small><b>{m.open_threads ? t('{n} open', { n: m.open_threads }) : t('none open')}</b></div>
            <div class="tile"><small>{m.state === 'merged' ? t('Merged') : t('Updated')}</small><b>{t('{ago} ago', { ago: ago(m.state === 'merged' ? m.merged_at : m.updated_at) })}</b></div>
            {#if tickets.length}<div class="tile wide"><small>{tickets.length > 1 ? t('Tickets') : t('Ticket')}</small>{#each tickets as { id, ticket } (id)}<div><a class="mono" href={ticket?.url ?? '#'} onclick={previewClick(id)}>{id}</a>{#if ticket}&nbsp;{ticket.name}&nbsp;<span class="pill">{ticket.status}</span>{/if}</div>{/each}</div>{/if}
            {#if m.labels?.length || (hasCore() && m.state === 'opened')}<div class="tile wide"><small>{t('Labels')}</small><div>{@render labels(m)}</div></div>{/if}
          </div>
          {#if labeling}<LabelPicker mr={m} current={m.labels ?? []} trigger={() => labelBtn} onclose={closeLabels} />{/if}
          {#if picking}
            {#if picking === 'assignees'}<PeoplePicker mr={m} kind="assignees" current={m.assignees ?? []} me={me} trigger={() => assignBtn} onclose={closePicker} />
            {:else}<PeoplePicker mr={m} kind="reviewers" current={m.reviewers} me={me} trigger={() => reviewBtn} onclose={closePicker} />{/if}
          {/if}
        {:else}
        <dl>
          <dt>{t('Branch')}</dt>
          <dd><code>{m.source_branch}</code><button class="copy" onclick={() => copy(m.source_branch)} title={t('Copy the branch name')}><Icon name={copied ? 'check' : 'copy'} /></button><span class="muted">→</span><code>{m.target_branch}</code></dd>
          {#if m.size}
            <dt>{t('Size')}</dt>
            <dd class="mono"><span class="add">+{lines(m.size.additions)}</span><span class="del">−{lines(m.size.deletions)}</span><span class="muted">· {tn(m.size.files, '{n} file', '{n} files')}</span>{#if isLarge(m.size)}<span class="large" title={t('Over {n} lines changed ([alerts] large_mr_lines)', { n: alertSettings().large_mr_lines })}>{t('large')}</span>{/if}</dd>
          {/if}
          <dt>{t('Author')}</dt>
          <dd>{m.author_name || m.author}{m.author === me ? ` ${t('(you)')}` : ''}</dd>
          <dt>{t('Assignees')}</dt>
          <dd class="text">{@render assignees(m)}</dd>
          <dt>{t('Reviewers')}</dt>
          <dd class="text">{@render reviewers(m)}</dd>
          {#if picking}
            <!-- One instance per list, its props its own: the picker that closes sends its pending choice
                 to its list, not to the one opening (props that followed `picking` mixed them up). -->
            {#if picking === 'assignees'}<PeoplePicker mr={m} kind="assignees" current={m.assignees ?? []} me={me} trigger={() => assignBtn} onclose={closePicker} />
            {:else}<PeoplePicker mr={m} kind="reviewers" current={m.reviewers} me={me} trigger={() => reviewBtn} onclose={closePicker} />{/if}
          {/if}
          {#each tickets as { id, ticket }, i (id)}
            <dt>{i === 0 ? (tickets.length > 1 ? t('Tickets') : t('Ticket')) : ''}</dt>
            <dd><a class="mono" href={ticket?.url ?? '#'} onclick={previewClick(id)}>{id}</a>{#if ticket}<span>{ticket.name}</span><span class="pill">{ticket.status}</span>{/if}</dd>
          {/each}
          <dt>{t('Threads')}</dt>
          <dd>{m.open_threads ? t('{n} open', { n: m.open_threads }) : t('none open')}</dd>
          {#if m.labels?.length || (hasCore() && m.state === 'opened')}
            <dt>{t('Labels')}</dt>
            <dd>{@render labels(m)}</dd>
            {#if labeling}<LabelPicker mr={m} current={m.labels ?? []} trigger={() => labelBtn} onclose={closeLabels} />{/if}
          {/if}
          <dt>{m.state === 'merged' ? t('Merged') : t('Updated')}</dt>
          <dd>{new Date((m.state === 'merged' ? m.merged_at : m.updated_at) ?? m.updated_at).toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' })} <span class="muted">({t('{ago} ago', { ago: ago(m.state === 'merged' ? m.merged_at : m.updated_at) })})</span></dd>
        </dl>
        {/if}

        {#if canReview()}
          <section>
            <h3>{t('Description')}</h3>
            {#if detail?.data}
              {#if detail.data.description.trim()}<div class="desc"><Markdown text={detail.data.description} forge={web} /></div>{:else}<p class="muted">{t('No description.')}</p>{/if}
            {:else if detail?.error}<p class="error">{detail.error}</p>
            {:else}<p class="muted">{t('Loading…')}</p>{/if}
          </section>
          <section>
            <h3>{t('Approvals')}{#if detail?.data?.approval_history.length}<span class="muted">{t('{given} given · {standing} standing', { given: detail.data.approval_history.filter((e) => e.kind === 'approved').length, standing: m.approved_by.length })}</span>{/if}</h3>
            {#if detail?.data}<MrApprovals events={detail.data.approval_history} me={me} />
            {:else if detail?.error}<p class="error">{detail.error}</p>
            {:else}<p class="muted">{t('Loading…')}</p>{/if}
          </section>
        {/if}

        <section>
          {#if m.head_pipeline}
            {@const p = m.head_pipeline}
            {@const total = pipelineSpan(p, now)}
            {@const failed = p.stages.flatMap((s) => s.failed_jobs)}
            <h3>
              {t('Pipeline')} <span class="pst {TONE[p.status] ?? ''}">{p.status}</span>{#if total !== null}<span class="muted mono">{span(total)}</span>{/if}
              <span class="grow"></span>
              {#if canShowLogs()}<button class="btn" onclick={() => openPipeline(p.id, mrKey(m), m.project ?? null)}>{t('Logs')}</button>{/if}
              <RetryPipeline id={p.id} status={m.pipeline} label />
              <RunPipeline mr={m} />
              <CancelPipeline id={p.id} status={m.pipeline} label />
              <a class="btn" href={p.url} target="_blank" rel="noreferrer">{forgeName(m?.forge)} <Icon name="external" /></a>
            </h3>
            <div class="track">
              {#each p.stages as s, i}
                {@const tone = stageTone(s)}
                {@const took = elapsed(s.started_at, s.finished_at, now)}
                <div class="step {tone}" class:reached={i > 0 && stageTone(p.stages[i - 1]) === 'ok'} title="{s.name} · {t('{done}/{total} jobs', { done: s.finished, total: s.jobs })} · {s.status}">
                  <span class="sdot">{#if tone === 'ok'}<Icon name="check" stroke={3} />{:else if tone === 'ko'}<Icon name="fail" stroke={3} />{:else if tone === 'man'}<Icon name="play" />{:else if tone === 'cx'}<Icon name="canceled" />{/if}</span>
                  <span class="n">{s.name}</span>
                  <span class="sm">{tone === 'man' ? t('manual') : tone === 'cx' ? t('canceled') : tone === 'run' ? `${s.finished}/${s.jobs}${took !== null ? ` · ${span(took)}` : ''}` : took !== null ? span(took) : s.status === 'skipped' ? t('skipped') : tn(s.jobs, '{n} job', '{n} jobs')}</span>
                  {#if s.manual_jobs.length}<StageManualJobs stage={s.name} jobs={s.manual_jobs} />{/if}
                </div>
              {/each}
            </div>
            {#each failed as j (j.url)}
              {@const target = logTarget(j.url)}
              <div class="job">
                <span class="pill ko">{tc('mrs', 'failed')}</span>
                <a class="mono n" href={j.url} target="_blank" rel="noreferrer" onclick={logsClick(j.url, mrKey(m), m.project ?? null)} title={keys(t('Click: log · ⌘-click: {forge}', { forge: forgeName(m?.forge) }))}>{j.name}</a>
                {#if target && 'job' in target}<RetryJob job={target.job} name={j.name} label />{/if}
              </div>
            {/each}
          {:else}
            <h3>{t('Pipeline')} <span class="muted">{t('none on this MR')}</span><span class="grow"></span><RunPipeline mr={m} label /></h3>
          {/if}
        </section>

        {#if m.state === 'opened' && !phone}
          <section>
            <h3>{t('Merge')}</h3>
            <div class="row">
              <!-- A conflict first: it blocks the rest (the merge queue's rebase would fail). -->
              {#if m.conflicts}<span class="pill ko" title={t('Conflicts with {branch}: rebase it by hand, or with an agent', { branch: m.target_branch })}>{t('conflicts')}</span>{/if}
              {#if queueLabel && hasMergeQueue(m)}<MergeQueueButton mr={m} label={queueLabel} wide />{/if}
              {#if queueLabel && snap?.merge_priority_prefix && hasMergeQueue(m)}<MergePriorityPicker mr={m} prefix={snap.merge_priority_prefix} />{/if}
              {#if !m.draft && (m.auto_merge || !m.conflicts)}<span class="am" class:on={m.auto_merge}>{#if m.auto_merge}<Icon name="automerge" /> {t('Auto-merge set')}{:else}{t('Auto-merge')}{/if} <AutoMergeButton mr={m} /></span>{/if}
              {#if rebaseHint(m)}<span class="am">{t('Rebase onto {branch}', { branch: m.target_branch })} <RebaseButton mr={m} /></span>{/if}
              <span class="grow"></span>
              <button class="btn" class:err={!!draftError} disabled={draftBusy} onclick={toggleDraft} title={draftError ?? (m.draft ? t('Remove the Draft: prefix: the MR can be merged') : t('Add the Draft: prefix: the MR cannot be merged'))}>{draftBusy ? '…' : m.draft ? t('Mark as ready') : t('Mark as draft')}</button>
            </div>
          </section>
        {/if}

        {#if !phone}
        <section>
          <h3>{actorName()}</h3>
          <div class="row">
            {#each mrActions(m) as a (a.id)}
              {#if a.href}<a class="btn claude" href={a.href} onclick={claudeClick(a)} title={`${a.prompt}\n${t('{key}: launch at once', { key: keys('⇧-click') })}`}>{a.label}</a>{:else}<span class="btn off" title={a.why}>{a.label}</span>{/if}
            {/each}
            {#if m.ticket && offers('time')}<button class="btn" onclick={() => openTimeDraft(m.ticket!, timeDescription(m))}><Icon name="timer" /> {t('Log time')}</button>{/if}
          </div>
        </section>
        {/if}
      {/if}
    </div>
    {#if phone && m && m.state === 'opened' && tab === 'overview'}
      <div class="mainbar">
        {#if main}<button class="primary" class:armed={armedMain.is()} disabled={mainBusy} onclick={runMain}>{mainBusy ? '…' : armedMain.is() ? (main.confirm ?? main.label) : main.label}</button>{/if}
        <button onclick={() => (sheet = true)}>{t('Actions')}</button>
      </div>
    {/if}
    {#if phone && sheet && m}<ActionSheet title="{mrKey(m)} {cleanTitle(m)}" actions={sheetActions} onclose={() => (sheet = false)} />{/if}
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 49; width: min(560px, 92vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-gitlab) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; border-bottom: 1px solid var(--border);
    background: var(--surface); box-shadow: inset 0 2px var(--src-gitlab);
  }
  .drawer.wide { width: min(1320px, 96vw); }
  /* The phone (the remote): the whole screen, larger type and targets. */
  .drawer.phone { left: 0; width: auto; border-left: none; box-shadow: none; font-size: 14px; }
  .phone header { padding: calc(8px + env(safe-area-inset-top)) 12px 8px 4px; }
  .back { display: inline-flex; align-items: center; gap: 2px; padding: 4px 6px; border: 0; background: none; color: var(--accent); font: inherit; font-size: 15px; }
  /* Four tabs on a phone: they share its width, and scroll rather than squeeze when their counts make them wider. */
  .phone .tabs { overflow-x: auto; scrollbar-width: none; }
  .phone .tabs::-webkit-scrollbar { display: none; }
  .phone .tabs button { flex: 1 0 auto; justify-content: center; padding: 10px 6px 8px; font-size: 14px; white-space: nowrap; }
  .phone .body { padding: 14px 14px 96px; }
  .phone h2 { font-size: 18px; }
  .branch { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-bottom: 12px; }
  .branch code { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tiles { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .tile { min-width: 0; padding: 8px 10px; border-radius: var(--r10); background: var(--surface-muted); }
  .tile.wide { grid-column: 1 / -1; }
  .tile small { display: block; margin-bottom: 2px; color: var(--text-muted); font-size: 11px; }
  .tile b { font-weight: 500; }
  .tile .pick { margin-left: 6px; padding: 2px 8px; font-size: 12px; }
  .phone section { margin-top: 18px; }
  .phone .btn { padding: 5px 12px; font-size: 13px; }
  .phone .step .n { font-size: 12px; }
  /* Above what the overview positions as it scrolls under it (the pipeline's step dots). */
  .mainbar { position: absolute; left: 0; right: 0; bottom: 0; z-index: 3; display: flex; gap: 8px; padding: 10px 12px calc(10px + env(safe-area-inset-bottom)); border-top: 1px solid var(--border); background: color-mix(in srgb, var(--surface) 92%, transparent); backdrop-filter: blur(8px); }
  .mainbar button { flex: 1; padding: 12px; border: 1px solid var(--border); border-radius: var(--r12); background: var(--surface); color: var(--text); font: inherit; font-size: 15px; }
  .mainbar .primary { flex: 2; border-color: var(--accent); background: var(--accent); color: var(--on-accent); font-weight: 600; }
  .mainbar .primary.armed { border-color: var(--warn); background: var(--warn); }
  .mainbar button:disabled { opacity: 0.6; }
  .compose.sticky { position: sticky; bottom: -96px; padding: 8px 0; background: var(--surface); }
  /* Full screen (changes tab): the whole window, no shadow or border. */
  .drawer.full { left: 0; width: auto; border-left: none; box-shadow: none; }
  .tabs { display: flex; gap: 2px; padding: 0 12px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
  .tabs button { display: inline-flex; align-items: center; gap: 5px; padding: 7px 10px 6px; border: none; border-bottom: 2px solid transparent; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .tabs button:hover { color: var(--text); }
  .tabs button.on { color: var(--text); font-weight: 500; border-bottom-color: var(--src-gitlab); }
  .tabs b { padding: 0 6px; border-radius: var(--r8); background: var(--surface-muted); font-size: var(--fs-tag); font-weight: 500; }
  .tabs .open { color: var(--warn); font-size: var(--fs-tag); }
  .commits { margin: 0; padding: 0; list-style: none; }
  .commits li { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; gap: 10px; align-items: baseline; padding: 5px 0; border-bottom: 1px solid var(--border); }
  .commits li:last-child { border-bottom: none; }
  .sha { font-size: var(--fs-tag); color: var(--info); text-decoration: none; }
  .sha:hover { text-decoration: underline; }
  .subject { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cmeta { color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  /* A phone: the title on its own line, under the sha and who. */
  .phone .commits li { grid-template-columns: auto minmax(0, 1fr); padding: 8px 0; }
  .phone .commits .subject { grid-column: 1 / -1; grid-row: 2; white-space: normal; }
  .approve { padding: 2px 10px; border: 1px solid color-mix(in srgb, var(--ok) 55%, var(--border)); border-radius: var(--r6); background: none; color: var(--ok); font: inherit; font-size: var(--fs-meta); font-weight: 500; cursor: pointer; }
  .approve:hover { background: var(--ok-bg); }
  .approve.on { background: var(--ok-bg); border-color: transparent; }
  .approve.error { border-color: var(--ko); color: var(--ko); }
  .approve:disabled { cursor: progress; opacity: 0.6; }
  .changes { padding: 2px 10px; border: 1px solid color-mix(in srgb, var(--warn) 55%, var(--border)); border-radius: var(--r6); background: none; color: var(--warn); font: inherit; font-size: var(--fs-meta); font-weight: 500; cursor: pointer; display: inline-flex; align-items: center; gap: 4px; }
  .changes:hover, .changes.on { background: color-mix(in srgb, var(--warn) 12%, transparent); }
  .changes.on { border-color: transparent; }
  .changes.error { border-color: var(--ko); color: var(--ko); }
  .changes:disabled { cursor: default; opacity: 0.6; }
  .ask { padding: 8px 12px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--warn) 5%, transparent); }
  .threads { display: flex; flex-direction: column; gap: 10px; }
  .desc { font-size: var(--fs-body); }
  .error { color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .iid { font-family: var(--mono); font-size: var(--fs-mono); font-weight: 600; color: var(--info); }
  .grow { flex: 1; }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 15px; cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .next { padding: 0 8px; border-radius: var(--r10); font-size: var(--fs-tag); font-weight: 500; line-height: 18px; --c: var(--text-muted); background: color-mix(in srgb, var(--c) 15%, transparent); color: color-mix(in srgb, var(--c) 70%, var(--text)); }
  .next.blocked { --c: var(--ko); } .next.todo { --c: var(--warn); } .next.wait { --c: var(--info); } .next.ready { --c: var(--ok); } .next.auto { --c: var(--accent); }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px 24px; }
  h2 { margin: 0 0 10px; font-size: 15px; font-weight: 600; line-height: 1.35; }
  .sec { color: var(--ko); margin-right: 4px; }
  .draft-tag { display: inline-block; margin-right: 6px; padding: 0 5px; border: 1px dashed currentColor; border-radius: var(--r4); font-size: 9.5px; font-weight: 600; text-transform: uppercase; vertical-align: 2px; color: var(--text-muted); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 14px; margin: 0; font-size: var(--fs-body); }
  dt { color: var(--text-muted); font-size: var(--fs-meta); }
  dd .add { color: var(--ok); }
  dd .del { color: var(--ko); }
  dd .large { padding: 0 5px; border-radius: var(--r4); background: var(--warn-bg); color: var(--warn); font-family: var(--sans); font-size: var(--fs-tag); font-weight: 600; }
  dd { margin: 0; display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; min-width: 0; }
  dd.text { display: block; }
  code { font-family: var(--mono); font-size: var(--fs-mono); padding: 0 4px; border-radius: var(--r3); background: var(--surface-muted); overflow-wrap: anywhere; }
  .copy { border: none; background: none; padding: 0 2px; color: var(--text-muted); font: inherit; cursor: pointer; }
  .pick { margin-left: 8px; padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .pick:hover, .pick.on { color: var(--text); border-color: var(--src-gitlab); }
  .pick + .pick { margin-left: 4px; }
  .pick.err { color: var(--ko); border-color: var(--ko); }
  .pick:disabled { cursor: progress; opacity: 0.7; }
  .label { padding: 0 6px; border-radius: var(--r8); font-size: var(--fs-tag); background: var(--surface-muted); }
  .sec-label { background: var(--ko-bg); color: var(--ko); }
  .q-label { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
  .ok { color: var(--ok); }
  dd .ko { color: var(--warn); }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); }
  section { margin-top: 14px; padding-top: 10px; border-top: 1px solid var(--border); }
  h3 { display: flex; align-items: center; gap: 8px; margin: 0 0 8px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 > :not(:first-child) { text-transform: none; letter-spacing: 0; font-weight: 400; }
  .pst.ok { color: var(--ok); } .pst.ko { color: var(--ko); } .pst.run { color: var(--warn); }
  .btn { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; text-decoration: none; }
  .btn:hover { border-color: var(--src-gitlab); text-decoration: none; }
  .btn.claude { color: var(--src-claude); border-color: color-mix(in srgb, var(--src-claude) 40%, var(--border)); }
  .btn.off { color: var(--text-muted); cursor: not-allowed; }
  .btn.err { border-color: var(--ko); color: var(--ko); }
  .btn:disabled { cursor: progress; opacity: 0.6; }
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: var(--fs-body); }
  .am { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); }
  .am :global(button) { opacity: 1 !important; }
  .am.on { padding: 0 2px 0 8px; border-radius: var(--r10); background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); font-weight: 600; }
  .job { display: flex; align-items: center; gap: 8px; margin-top: 6px; font-size: var(--fs-body); }
  .job .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .track { display: flex; align-items: flex-start; margin: 4px 0 2px; }
  .step { position: relative; flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: center; gap: 3px; text-align: center; }
  .step::before { content: ''; position: absolute; top: 9px; left: -50%; right: 50%; height: 2px; background: var(--border); }
  .step:first-child::before { display: none; }
  .step.reached::before { background: var(--ok); }
  .sdot { position: relative; z-index: 1; display: flex; align-items: center; justify-content: center; width: 20px; height: 20px; border-radius: 50%; border: 2px solid var(--border); background: var(--surface); color: var(--text-muted); font-size: var(--fs-meta); font-weight: 700; }
  .step.ok .sdot { background: var(--ok); border-color: var(--ok); color: var(--surface); }
  .step.ko .sdot { background: var(--ko); border-color: var(--ko); color: var(--surface); }
  .step.run .sdot { border-color: var(--warn); }
  .step.man .sdot { border-style: dashed; font-size: 8px; }
  .step .n { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-meta); font-weight: 500; }
  .step .sm { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .step.ko .sm { color: var(--ko); } .step.run .sm { color: var(--warn); }
</style>
