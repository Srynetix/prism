<script lang="ts">
  import Icon from './Icon.svelte'
  import { STATUS_LABEL, addNote, parseStatus, removeNote, repoNotes, setDone, setStatus, splitTags, updateNote, type Note, type NoteStatus } from '../notes.svelte'
  import { armer } from '../armed.svelte'
  import { isReady, sendNote } from '../note-send'
  import { agentRepo } from '../repositories'
  import { nameOf } from '../agent-cli'
  import { isDesktop } from '../bridge'
  import { setPref } from '../layout.svelte'
  import { openQuickNotes } from '../quick-notes.svelte'
  import { ago } from '../time'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * The Notes tab: the open notes of the repository the agent works in (`repo` of a note, see
   * notes.svelte.ts), reminders first, each sent to it with ➤ (without its tags; a TODO goes
   * DOING); a note written here is filed under that repository. On hover, its status (TODO, DOING,
   * DONE; the one it has, clicked again, takes it off; a reminder: Done), its edit (a
   * double-click on its text, or the pencil: Enter saves, Esc cancels, as in the Notes block) and
   * its deletion, in two clicks. Done, it leaves the list. "All notes" opens the Notes block over the view, filtered on it.
   */
  let { agent }: { agent: Agent } = $props()

  const repo = $derived(agentRepo(agent))
  const list = $derived(repo ? repoNotes(repo) : [])
  const running = $derived(agent.state === 'running')
  const who = $derived(agent.title || nameOf(agent))

  let draft = $state('')
  function submit() {
    const text = draft.trim()
    if (!text || !repo) return
    const parsed = parseStatus(text)
    addNote(parsed.text, 'sticky', { status: parsed.status, repo })
    draft = ''
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      submit()
    }
  }
  function all() {
    if (!repo) return
    setPref('notes.repo', repo)
    openQuickNotes()
  }
  const STATUSES: NoteStatus[] = ['todo', 'doing', 'done']
  const armed = armer(4000)
  function remove(id: string) {
    if (armed.confirm(id)) removeNote(id)
  }
  /** The note being edited, and its text meanwhile. */
  let editing = $state<string | null>(null)
  let editText = $state('')
  function startEdit(n: Note) {
    editing = n.id
    editText = n.text
  }
  function commitEdit() {
    if (!editing) return
    const text = editText.trim()
    if (text) updateNote(editing, text)
    editing = null
  }
  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      commitEdit()
    } else if (e.key === 'Escape') {
      // Its panel stays open: Esc only leaves the edit.
      e.stopPropagation()
      editing = null
    }
  }
  const focus = (el: HTMLTextAreaElement) => {
    el.focus()
    el.setSelectionRange(el.value.length, el.value.length)
  }
  const label = (n: Note) => (n.kind === 'reminder' ? t('Reminder') : n.status ? STATUS_LABEL[n.status] : '')
</script>

<div class="notes">
  {#if !repo}
    <p class="none">{t('{who} works outside the repositories of the config (Settings › Repositories): its folder has no notes.', { who })}</p>
  {:else}
    <div class="sec">
      <span class="cap"><Icon name="branch" />{repo}<span class="grow"></span>{#if isDesktop()}<button class="mini" onclick={all} title={t('The Notes block over the view, filtered on {repo}', { repo })}>{t('All notes')}</button>{/if}</span>
      <textarea bind:value={draft} onkeydown={onKey} rows="2" placeholder={t('A note about {repo}… (todo / doing first: a task)', { repo })}></textarea>
    </div>
    {#each list as n (n.id)}
      <div class="note">
        <div class="body">
          {#if label(n)}<span class="st {n.kind === 'reminder' ? 'rem' : n.status}">{label(n)}</span>{/if}
          {#if editing === n.id}
            <textarea class="edit" bind:value={editText} onkeydown={onEditKey} onblur={commitEdit} use:focus rows="3" aria-label={t('The note’s text')}></textarea>
          {:else}
            <!-- Selectable with the mouse; a double-click edits it, Enter too from the keyboard. -->
            <span class="text" role="button" tabindex="0" title={t('Double-click to edit')} ondblclick={() => (window.getSelection()?.removeAllRanges(), startEdit(n))} onkeydown={(e) => e.key === 'Enter' && e.target === e.currentTarget && startEdit(n)}>{#each splitTags(n.text) as part}{#if 'tag' in part}<span class="tag">{part.text}</span>{:else}{part.text}{/if}{/each}</span>
          {/if}
        </div>
        <div class="meta">
          <span class="muted">{n.kind === 'reminder' && n.remind_at ? new Date(n.remind_at).toLocaleString(undefined, { weekday: 'short', hour: '2-digit', minute: '2-digit' }) : ago(n.updated_at)}</span>
          <span class="grow"></span>
          {#if n.kind === 'reminder'}
            <button class="mini act" onclick={() => setDone(n.id, true)} title={t('Mark the reminder as done')}><Icon name="check" />{t('Done')}</button>
          {:else}
            <span class="seg act" role="group" aria-label={t('Status of the note')}>
              {#each STATUSES as st (st)}
                <button class:on={n.status === st} onclick={() => setStatus(n.id, n.status === st ? null : st)} aria-pressed={n.status === st} title={n.status === st ? t('Take the status off: a plain note') : t('Mark {status}', { status: STATUS_LABEL[st] })}>{STATUS_LABEL[st]}</button>
              {/each}
            </span>
          {/if}
          <!-- A phone has no double-click to speak of: its pencil edits. -->
          <button class="mini act" onclick={() => startEdit(n)} aria-label={t('Edit the note')} title={t('Edit (or double-click its text)')}><Icon name="edit" /></button>
          <button class="mini act del" class:armed={armed.is(n.id)} onclick={() => remove(n.id)} onblur={() => armed.is(n.id) && armed.disarm()} aria-label={armed.is(n.id) ? t('Click again to delete the note') : t('Delete the note')} title={armed.is(n.id) ? t('Click again to delete') : t('Delete')}>{#if armed.is(n.id)}{t('Delete?')}{:else}<Icon name="close" />{/if}</button>
          {#if running}
            <button class="mini send" onclick={() => sendNote(n, agent)} title={isReady(agent) ? t('To {who}, at once', { who }) : t('To {who}, at once: it reads it once its turn is over', { who })}><Icon name="send" />{t('Send')}</button>
          {/if}
        </div>
      </div>
    {:else}
      <p class="none">{t('No open note about {repo}.', { repo })}</p>
    {/each}
  {/if}
</div>

<style>
  .notes { flex: 1; min-height: 0; overflow: auto; display: flex; flex-direction: column; font-size: var(--fs-meta); }
  .sec { display: flex; flex-direction: column; gap: 6px; padding: 9px 12px; border-bottom: 1px solid var(--border); }
  .cap { display: flex; align-items: center; gap: 5px; font-size: var(--fs-tag); font-weight: 600; color: var(--text-muted); }
  .grow { flex: 1; }
  textarea { padding: 5px 7px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); resize: vertical; }
  textarea:focus { outline: none; border-color: var(--accent); }
  .note { display: flex; flex-direction: column; gap: 3px; padding: 7px 12px; border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent); }
  .note:hover { background: var(--surface-muted); }
  .body { display: flex; align-items: baseline; gap: 6px; }
  .text { flex: 1; min-width: 0; font-size: var(--fs-body); white-space: pre-wrap; overflow-wrap: anywhere; }
  .text { cursor: text; user-select: text; }
  .text:focus-visible { outline: 1px solid var(--accent); outline-offset: 2px; border-radius: var(--r4); }
  .edit { flex: 1; min-width: 0; border-color: var(--accent); }
  .tag { color: var(--info); }
  .st { flex-shrink: 0; padding: 0 5px; border-radius: var(--r4); font-size: var(--fs-tag); font-weight: 600; background: var(--surface-muted); color: var(--text-muted); }
  .st.todo { color: var(--warn); }
  .st.doing { color: var(--info); background: var(--info-bg); }
  .st.rem { color: var(--src-alert); }
  .meta { display: flex; align-items: center; gap: 6px; font-size: var(--fs-tag); }
  .muted { color: var(--text-muted); }
  .mini { display: inline-flex; align-items: center; gap: 4px; padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .mini:hover { background: var(--surface-muted); }
  .send, .act { opacity: 0; }
  .note:hover .send, .send:focus-visible, .note:hover .act, .act:focus-within, .act:focus-visible, .del.armed { opacity: 1; }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .seg button { padding: 0 6px; border: none; border-left: 1px solid var(--border); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); font-weight: 600; line-height: 18px; cursor: pointer; }
  .seg button:first-child { border-left: none; }
  .seg button:hover { color: var(--text); background: var(--surface-muted); }
  .seg button.on { color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, var(--surface)); }
  .del:hover { border-color: var(--ko); color: var(--ko); }
  .del.armed { border-color: var(--ko); background: var(--ko-bg); color: var(--ko); }
  .send:hover { border-color: var(--src-claude); color: var(--src-claude); }
  .none { margin: 0; padding: 10px 12px; color: var(--text-muted); }
  :global(html.remote) .send, :global(html.remote) .act { opacity: 1; min-height: 30px; }
  :global(html.remote) textarea { font-size: 16px; }
</style>
