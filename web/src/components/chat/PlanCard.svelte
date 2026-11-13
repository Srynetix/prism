<script lang="ts">
  import { mrScopeOf } from '../../linkify'
  import { live } from '../../live.svelte'
  import Icon from '../Icon.svelte'
  import { t, tn, tc } from '../../i18n.svelte'
  import Markdown from '../Markdown.svelte'
  import CommentableText from './CommentableText.svelte'
  import { answerPlan, type ProposedPlan } from '../../agents.svelte'
  import { compose, lineDiff, passages, quoteOf } from '../../comments'
  import { carryTo, commentsOf, draftsOf, forget, generalOf, markSent, setGeneral } from '../../comments.svelte'
  import { armer } from '../../armed.svelte'
  import { onPhone } from '../../quick-answers'
  import DictateButton from './DictateButton.svelte'
  import DictationBar from './DictationBar.svelte'
  import { filesOfAgent } from '../../dictation.svelte'

  /**
   * A plan the agent proposes (plan mode's `ExitPlanMode`), in its Chat or its Plan tab: the plan,
   * its passages commented on (CommentableText), and while it waits, its answer: **Send comments ·
   * keep planning** (refused, your comments as one message: it plans again), **Approve** (with
   * comments drafted: **Approve with comments**, they go with it), **Approve, ask each edit**. Each
   * new plan is a version: **Changes** shows its lines added and removed since the one before; a
   * comment sent whose passage reads the same in it is a draft again, one whose passage changed is
   * addressed (put away in a click). Approved, its comments are forgotten. Older versions fold.
   */
  let { id, plan, previous = null, latest, acts, typing, wide = false, onwide }: {
    id: string
    plan: ProposedPlan
    /** The version before's text: for Changes. */
    previous?: string | null
    /** The last plan of its conversation: the one answered, the one commented on. */
    latest: boolean
    /** Answered from here (a phone: `[remote] actions`), and with words (`[remote] typing`). */
    acts: boolean
    typing: boolean
    /** In the Plan tab: open, without ⤢. */
    wide?: boolean
    /** Opens it in the Plan tab. */
    onwide?: () => void
  } = $props()
  /** The repositories it works in, whose MRs its `!12` name. */
  const mrScope = $derived(mrScopeOf(id, live.snapshot))

  const waiting = $derived(plan.verdict === 'waiting' && latest)
  let unfolded = $state<boolean | null>(null)
  const open = $derived(unfolded ?? (latest || wide))
  let changes = $state(false)
  let preview = $state(false)
  let busy = $state(false)
  let error = $state<string | null>(null)
  const armed = armer()

  const ps = $derived(passages(plan.text))
  const drafts = $derived(latest ? draftsOf(id, 'plan') : [])
  const general = $derived(latest ? generalOf(id, 'plan') : '')
  let generalField = $state<HTMLTextAreaElement>()
  const addressed = $derived(latest ? commentsOf(id, 'plan').filter((c) => c.status === 'addressed') : [])
  const worded = $derived(drafts.length > 0 || !!general.trim())
  const editable = $derived(waiting && acts && typing)

  // A new version: what was sent carried over. Approved: its comments are done with.
  $effect(() => {
    if (!latest || !plan.text) return
    if (plan.verdict === 'waiting') carryTo(id, plan.call, ps)
    else if (plan.verdict === 'approved' && commentsOf(id, 'plan').length) forget(id, 'plan')
  })

  const VERDICT: Record<ProposedPlan['verdict'], [string, string]> = {
    waiting: [t('waiting for you'), 'warn'],
    approved: [tc('chat', 'approved'), 'ok'],
    changes: [t('sent back'), 'info'],
    refused: [t('refused'), 'muted'],
  }

  async function answer(how: 'approve' | 'approve_each' | 'approve_auto' | 'changes' | 'approve_with') {
    const words = how === 'changes' || how === 'approve_with' ? compose('plan', drafts, general, ps, how === 'changes') : undefined
    busy = true
    error = null
    try {
      await answerPlan(id, plan.call, how, words)
      if (words) markSent(id, 'plan', plan.call)
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }
  function clear() {
    if (armed.confirm('clear')) forget(id, 'plan')
  }
</script>

<div class="plan" class:waiting class:wide>
  <div class="head">
    <button class="t" onclick={() => (unfolded = !open)} aria-expanded={open} title={open ? t('Fold') : t('Unfold')}>
      <Icon name="queue" /><b>{waiting ? t('Plan proposed') : t('Plan')}</b><span class="v">v{plan.version}</span>
    </button>
    <span class="pill {VERDICT[plan.verdict][1]}">{VERDICT[plan.verdict][0]}</span>
    <span class="grow"></span>
    {#if open && previous !== null}
      <button class="seg" class:on={changes} onclick={() => (changes = !changes)} title={t('Its lines added and removed since v{version}', { version: plan.version - 1 })}>{t('Changes')}</button>
    {/if}
    {#if onwide && !wide}<button class="ic" onclick={onwide} title={t('Open in its Plan tab')}><Icon name="expand" size={12} /></button>{/if}
  </div>
  {#if open}
    <div class="body">
      {#if !plan.text}
        <p class="muted">{t('Its plan was written before what Prism read of its conversation.')}</p>
      {:else if changes && previous !== null}
        <div class="diff">
          {#each lineDiff(previous, plan.text) as l, i (i)}<div class={l.kind}>{l.kind === 'add' ? '+ ' : l.kind === 'del' ? '− ' : '  '}{l.text}</div>{/each}
        </div>
      {:else if latest}
        <CommentableText {id} target="plan" text={plan.text} {editable} reading={wide} />
      {:else}
        <Markdown text={plan.text} reading={wide} mrs scope={mrScope} />
      {/if}
      {#if plan.feedback}
        <div class="said"><small>{plan.verdict === 'approved' ? t('Approved with your words') : t('Your words')}</small>{plan.feedback}</div>
      {/if}
      {#if addressed.length}
        <div class="addressed">
          <span>✓ {tn(addressed.length, '{n} comment addressed:', '{n} comments addressed:')} {addressed.map((c) => `“${quoteOf(c.quote, 40)}”`).join(', ')}</span>
          <button class="lnk" onclick={() => forget(id, 'plan', 'addressed')}>{t('Put away')}</button>
        </div>
      {/if}
    </div>
    {#if waiting && acts}
      <div class="dock">
        {#if typing}
          {#if worded}
            <div class="tray">
              <Icon name="comment" size={12} /><b>{tn(drafts.length, '{n} comment', '{n} comments')}</b>{#if general.trim()}<span>{t('+ general words')}</span>{/if}
              <span class="grow"></span>
              <button class="lnk" onclick={() => (preview = !preview)}>{preview ? t('Hide') : t('Preview')}</button>
              <button class="lnk" class:armed={armed.is('clear')} onclick={clear}>{armed.is('clear') ? t('Clear them?') : t('Clear')}</button>
            </div>
            {#if preview}<pre class="pv">{compose('plan', drafts, general, ps, true)}</pre>{/if}
          {/if}
          <div class="gen">
            <textarea bind:this={generalField} value={general} oninput={(e) => setGeneral(id, 'plan', e.currentTarget.value)} rows="1" writingsuggestions="false" placeholder={t('A general comment (optional)…')}></textarea>
            <DictateButton key="plan:{id}" field={() => generalField} text={() => general} write={(v) => setGeneral(id, 'plan', v)} hints={() => filesOfAgent(id)} />
          </div>
          <DictationBar key="plan:{id}" />
        {/if}
        <div class="row">
          {#if typing}
            <button class="b primary" disabled={busy || !worded} onclick={() => answer('changes')} title={t('Refused: your comments go to it as one message, it plans again')}>↺ {t('Send comments · keep planning')}</button>
          {/if}
          <span class="grow"></span>
          {#if worded}
            <button class="b ok" disabled={busy} onclick={() => answer('approve_with')} title={t('Approved: your comments go with it')}>✓ {t('Approve with comments')}</button>
          {:else}
            <button class="b ok" disabled={busy} onclick={() => answer('approve')} title={t('Approved: it goes on, its edits allowed (where its menu offers only auto mode, each edit asks you)')}>✓ {t('Approve')}</button>
            {#if !onPhone()}
              <button class="b" disabled={busy} class:armed={armed.is('auto')} onclick={() => armed.confirm('auto') && answer('approve_auto')} title={t('Approved in auto mode: its commands run without asking you (a second click)')}>{armed.is('auto') ? t('Auto mode?') : t('Approve, auto mode')}</button>
            {/if}
          {/if}
          <button class="b" disabled={busy || worded} onclick={() => answer('approve_each')} title={worded ? t('Your comments go with Approve: clear them to approve each edit') : t('Approved: each of its edits asks you')}>{t('Approve, ask each edit')}</button>
        </div>
        {#if error}<p class="err">{error}</p>{/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .plan { border: 1px solid var(--border); border-radius: 10px; background: var(--surface); overflow: hidden; }
  .plan.waiting { border-color: var(--info); }
  .head { display: flex; align-items: center; gap: 7px; padding: 5px 10px; background: color-mix(in srgb, var(--info) 10%, transparent); font-size: 12px; }
  .t { display: inline-flex; align-items: center; gap: 6px; padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .t b { font-weight: 600; }
  .t .v { color: var(--text-muted); }
  .pill { font-size: 11px; padding: 0 7px; border-radius: 8px; background: var(--surface-muted); color: var(--text-muted); white-space: nowrap; }
  .pill.warn { background: var(--warn-bg, color-mix(in srgb, var(--warn) 15%, transparent)); color: var(--warn); }
  .pill.ok { background: var(--ok-bg); color: var(--ok); }
  .pill.info { background: var(--info-bg); color: var(--info); }
  .grow { flex: 1; }
  .seg { height: 20px; padding: 0 8px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text-muted); font: inherit; font-size: 11px; cursor: pointer; }
  .seg.on { border-color: var(--info); color: var(--info); }
  .ic { display: grid; place-items: center; width: 22px; height: 20px; padding: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .ic:hover { color: var(--text); }
  .body { padding: 8px 12px 10px; font-size: 13.5px; line-height: 1.5; }
  .muted { margin: 0; color: var(--text-muted); font-size: 12.5px; }
  .diff { font: 12px/1.5 ui-monospace, 'SF Mono', Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-muted); }
  .diff .add { background: var(--ok-bg); color: var(--ok); }
  .diff .del { background: color-mix(in srgb, var(--ko) 14%, transparent); color: var(--ko); text-decoration: line-through; }
  .said { margin-top: 8px; padding: 6px 9px; border-left: 2px solid var(--accent); background: var(--surface-muted); border-radius: 0 6px 6px 0; font-size: 12.5px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .said small { display: block; color: var(--text-muted); font-size: 11px; }
  .addressed { display: flex; align-items: center; gap: 8px; margin-top: 8px; font-size: 12px; color: var(--ok); }
  .addressed span { flex: 1; min-width: 0; }
  .dock { display: flex; flex-direction: column; gap: 6px; padding: 8px 12px; border-top: 1px solid var(--border); background: var(--bg, var(--surface-muted)); }
  .tray { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: 7px; background: color-mix(in srgb, var(--warn) 14%, transparent); font-size: 12px; }
  .tray span { color: var(--text-muted); }
  .pv { margin: 0; padding: 7px 9px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); font: 11.5px/1.45 ui-monospace, 'SF Mono', Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 200px; overflow-y: auto; }
  .gen { display: flex; align-items: flex-start; gap: 4px; }
  textarea { width: 100%; resize: vertical; min-height: 30px; border: 1px solid var(--border); border-radius: 7px; padding: 5px 8px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
  .row { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .b { height: 26px; padding: 0 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; white-space: nowrap; cursor: pointer; }
  .b.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
  .b.ok { background: var(--ok); border-color: var(--ok); color: var(--surface); }
  .b:disabled { opacity: 0.5; cursor: default; }
  .lnk { border: none; background: none; padding: 0 2px; color: var(--info); font: inherit; font-size: 12px; cursor: pointer; }
  .lnk.armed { color: var(--ko); font-weight: 600; }
  .b.armed { color: var(--ko); border-color: var(--ko); }
  .err { margin: 0; color: var(--ko); font-size: 12px; }
  :global(html.remote) textarea { font-size: 16px; }
  :global(html.remote) .b { height: 34px; }
</style>
