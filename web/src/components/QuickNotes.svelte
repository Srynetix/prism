<script lang="ts">
  import { t } from '../i18n.svelte'
  import Notes from './Notes.svelte'
  import { live } from '../live.svelte'
  import { closeQuickNotes, quickNotes } from '../quick-notes.svelte'

  /**
   * Quick input (quick-notes.svelte.ts): the Notes block above the current view, whatever
   * it is. Enter saves and keeps the input open for the next one; Escape or a
   * click outside closes. Escape in a note being edited only cancels the edit.
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape') return
    if ((e.target as HTMLElement).closest?.('textarea.edit, [role=menu]')) return
    e.preventDefault()
    e.stopPropagation()
    closeQuickNotes()
  }
</script>

{#if quickNotes.open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions : Escape closes from the keyboard -->
  <div class="backdrop" onclick={(e) => e.target === e.currentTarget && closeQuickNotes()}>
    <div class="sheet" role="dialog" aria-modal="true" aria-label={t('Quick notes')} tabindex="-1" {onkeydown}>
      <Notes snap={live.snapshot} floating opened={quickNotes.opened} />
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; z-index: 65; display: flex; justify-content: center; align-items: flex-start; padding: 9vh 16px 16px; background: color-mix(in srgb, var(--bg) 45%, transparent); backdrop-filter: blur(2px); }
  .sheet { width: min(620px, 100%); max-height: 78vh; display: flex; flex-direction: column; border-radius: var(--r12); box-shadow: var(--shadow-modal); outline: none; }
  .sheet :global(.panel) { flex: 1; min-height: 0; max-height: 78vh; }
</style>
