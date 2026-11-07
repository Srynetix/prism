<script lang="ts">
  import { t } from '../i18n.svelte'
  import { conventions } from '../conventions'
  import { offers } from '../tracker'
  import { openTicket } from '../ticket-view.svelte'
  import { toast } from '../toast.svelte'
  import { keys } from '../platform'
  import { createTicket, ticketBoards } from '../new-ticket.svelte'
  import Icon from './Icon.svelte'

  /**
   * A new ticket: its board (when there are several), its lane (the first one to pick by default),
   * its title, a description in Markdown (folded until asked) and Assign me (the trackers with
   * assignees). Enter in the title creates it, ⌘↵ creates and opens it; Escape closes.
   */
  let { onclose, wide = false }: { onclose: () => void; wide?: boolean } = $props()

  const boards = $derived(ticketBoards())
  let board = $state('')
  let status = $state('')
  let title = $state('')
  let description = $state('')
  let described = $state(false)
  let assignMe = $state(false)
  let saving = $state(false)
  let error = $state('')
  const lanes = $derived((boards.find((b) => b.id === board) ?? boards[0])?.statuses.filter((s) => !s.closed) ?? [])
  $effect(() => {
    if (!boards.some((b) => b.id === board) && boards.length) board = boards[0].id
  })
  $effect(() => {
    if (!lanes.some((s) => s.status === status)) status = (lanes.find((s) => conventions().pick_statuses?.includes(s.status)) ?? lanes[0])?.status ?? ''
  })

  async function create(open: boolean) {
    if (saving || !title.trim()) return
    saving = true
    error = ''
    try {
      const key = await createTicket({ board: board || boards[0].id, status, title, description: described ? description : '', assignMe })
      title = ''
      description = ''
      described = false
      onclose()
      if (key && open) openTicket(key)
      else if (key) toast(t('{key} created', { key }), 4000, { label: t('Open'), run: () => openTicket(key) })
    } catch (e) {
      error = String(e)
    } finally {
      saving = false
    }
  }

  function onkey(e: KeyboardEvent, inTitle: boolean) {
    if (e.key === 'Escape') {
      e.stopPropagation()
      onclose()
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      void create(true)
    } else if (e.key === 'Enter' && inTitle && !e.isComposing) {
      e.preventDefault()
      void create(false)
    }
  }
</script>

<form class="new-ticket" class:wide onsubmit={(e) => { e.preventDefault(); void create(false) }}>
  <div class="row">
    {#if boards.length > 1}<select bind:value={board} aria-label={t('Board')}>{#each boards as b (b.id)}<option value={b.id}>{b.name}</option>{/each}</select>{/if}
    <select bind:value={status} aria-label={t('Lane')}>{#each lanes as s (s.status)}<option value={s.status}>{s.status}</option>{/each}</select>
    <!-- svelte-ignore a11y_autofocus -->
    <input class="title" bind:value={title} placeholder={t('Title of the new ticket')} aria-label={t('Title')} autofocus onkeydown={(e) => onkey(e, true)} />
  </div>
  {#if described}
    <!-- svelte-ignore a11y_autofocus -->
    <textarea bind:value={description} rows={wide ? 8 : 4} placeholder={t('Description (Markdown)')} aria-label={t('Description')} autofocus onkeydown={(e) => onkey(e, false)}></textarea>
  {/if}
  <div class="row foot">
    {#if !described}<button type="button" class="link" onclick={() => (described = true)}><Icon name="plus" size={11} /> {t('Description')}</button>{/if}
    {#if offers('assignees')}<label class="me"><input type="checkbox" bind:checked={assignMe} /> {t('Assign me')}</label>{/if}
    <span class="grow"></span>
    <span class="hint">{keys('⌘↵')} {t('creates and opens')}</span>
    <button type="submit" class="primary" disabled={saving || !title.trim() || !status}>{saving ? '…' : t('Create')}</button>
  </div>
  {#if error}<p class="err" title={error}>{error}</p>{/if}
</form>

<style>
  .new-ticket { display: flex; flex-direction: column; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); }
  .new-ticket.wide { padding: 0; border-bottom: none; }
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .title { flex: 1 1 160px; min-width: 0; }
  textarea { width: 100%; box-sizing: border-box; resize: vertical; font: inherit; font-family: var(--mono); font-size: var(--fs-mono); line-height: 1.45; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); }
  .foot { gap: 10px; }
  .grow { flex: 1; }
  .link { display: inline-flex; align-items: center; gap: 4px; padding: 0; border: none; background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .link:hover { color: var(--text); }
  .me { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); cursor: pointer; }
  .hint { color: var(--text-muted); font-size: var(--fs-tag); }
  .primary { padding: 2px 12px; border: 1px solid var(--accent); border-radius: var(--r6); background: var(--accent); color: var(--on-accent); font: inherit; cursor: pointer; }
  .primary:disabled { opacity: 0.5; cursor: default; }
  .err { margin: 0; color: var(--ko); font-size: var(--fs-tag); overflow-wrap: anywhere; }
</style>
