<script lang="ts">
  import { t } from '../i18n.svelte'
  import { locale } from '../locale'
  import Markdown from './Markdown.svelte'
  import CommentBox from './CommentBox.svelte'
  import Icon from './Icon.svelte'
  import { ago } from '../time'
  import type { Discussion } from '../mr-review.svelte'
  import { keys } from '../platform'
  import { onGithubWeb } from '../forge'
  import { emojiText } from '../emoji.svelte'

  /** An MR discussion thread: its notes, its state, and an optional reply. */
  let {
    discussion: d, me, onreply, onresolve, onlocate, compact = false, forge,
  }: {
    discussion: Discussion
    me: string | null
    onreply: (body: string) => Promise<void>
    /** Resolve the thread (true) or reopen it. */
    onresolve?: (resolved: boolean) => Promise<void>
    /** The project's web address: images and uploaded files in the notes, their emoji shortcodes. */
    forge?: string
    /** Thread placed on the diff: go there. */
    onlocate?: () => void
    /** In the diff: without the file reminder. */
    compact?: boolean
  } = $props()

  let replying = $state(false)
  /** A folded thread's first note, its start, shortcodes as emoji (after the cut: no emoji cut in half). */
  const preview = (body: string) => (forge ? emojiText(body.slice(0, 80), onGithubWeb(forge)) : body.slice(0, 80))
  let resolving = $state(false)
  let resolveError = $state<string | null>(null)
  async function toggleResolved() {
    if (!onresolve || resolving) return
    resolving = true
    resolveError = null
    try {
      await onresolve(!d.resolved)
    } catch (e) {
      resolveError = String(e)
    } finally {
      resolving = false
    }
  }
  // A resolved thread is collapsed on open; after that, the click decides.
  // svelte-ignore state_referenced_locally
  let open = $state(!d.resolved)
</script>

<article class="thread" class:resolved={d.resolved}>
  {#if (d.path && !compact) || d.resolvable || (compact && d.start_line)}
    <div class="ctx">
      {#if d.path && !compact}
        <button class="where mono" onclick={onlocate} disabled={!onlocate} title={t('Show in the changes')}>{d.path}{d.line ? `:${d.start_line ? `${d.start_line}–` : ''}${d.line}` : ''}</button>
      {:else if d.start_line && d.line}
        <!-- Under its last line: the lines it is on. -->
        <span class="where mono">{t('lines {from}–{to}', { from: d.start_line, to: d.line })}</span>
      {/if}
      <span class="grow"></span>
      {#if d.resolvable}<button class="state" class:ok={d.resolved} onclick={() => (open = !open)} title={open ? t('Fold') : t('Unfold')}>{#if d.resolved}<Icon name="check" /> {t('resolved')} <Icon name={open ? 'chevronDown' : 'chevronRight'} />{:else}{t('unresolved')}{/if}</button>{/if}
    </div>
  {/if}
  {#if open}
    {#each d.notes as n (n.id)}
      <div class="note">
        <div class="who"><b class:me={n.author === me}>{n.author === me ? t('you') : n.author_name || n.author}</b><span class="muted" title={new Date(n.created_at).toLocaleString(locale())}>{ago(n.created_at) === t('now') ? t('just now') : t('{ago} ago', { ago: ago(n.created_at) })}</span></div>
        <Markdown text={n.body} {forge} />
      </div>
    {/each}
    {#if replying}
      <CommentBox {forge} label={t('Reply')} placeholder={keys(t('Reply… (⌘↵ to send)'))} autofocus onsend={onreply} oncancel={() => (replying = false)} />
    {:else}
      <div class="acts">
        <button class="reply" onclick={() => (replying = true)}>{t('Reply')}</button>
        {#if d.resolvable && onresolve}
          <button class="reply" class:err={!!resolveError} disabled={resolving} onclick={toggleResolved} title={resolveError ?? ''}>{resolving ? '…' : d.resolved ? t('Reopen thread') : t('Resolve thread')}</button>
        {/if}
      </div>
    {/if}
  {:else}
    <div class="muted folded">{d.notes.length} note{d.notes.length > 1 ? 's' : ''} · {d.notes[0]?.author_name || d.notes[0]?.author}: {preview(d.notes[0]?.body ?? '')}</div>
  {/if}
</article>

<style>
  .thread { display: flex; flex-direction: column; gap: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); }
  .thread.resolved { background: color-mix(in srgb, var(--surface-muted) 50%, var(--surface)); }
  .ctx { display: flex; align-items: center; gap: 8px; font-size: var(--fs-meta); }
  .where { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: none; background: none; padding: 0; color: var(--info); font-size: var(--fs-tag); cursor: pointer; text-align: left; }
  .where:disabled, span.where { color: var(--text-muted); cursor: default; }
  .grow { flex: 1; }
  .state { border: none; background: none; padding: 0; font: inherit; font-size: var(--fs-tag); color: var(--warn); cursor: pointer; }
  .state.ok { color: var(--ok); }
  .note + .note { padding-top: 8px; border-top: 1px dashed var(--border); }
  .who { display: flex; align-items: baseline; gap: 8px; margin-bottom: 2px; font-size: var(--fs-meta); }
  .who b { font-weight: 600; }
  .who b.me { color: var(--accent); }
  .muted { color: var(--text-muted); }
  .folded { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-meta); }
  .mono { font-family: var(--mono); }
  .acts { display: flex; gap: 14px; }
  .reply.err { color: var(--ko); }
  .reply:disabled { cursor: progress; opacity: 0.6; }
  .reply { align-self: flex-start; border: none; background: none; padding: 0; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .reply:hover { text-decoration: underline; }
  .note :global(.md) { font-size: var(--fs-body); }
</style>
