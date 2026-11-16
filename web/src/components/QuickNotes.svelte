<script lang="ts">
  import { t } from '../i18n.svelte'
  import Notes from './Notes.svelte'
  import Modal from './ui/Modal.svelte'
  import { live } from '../live.svelte'
  import { pushLayer } from '../layers.svelte'
  import { closeQuickNotes, quickNotes } from '../quick-notes.svelte'

  /**
   * Quick input (quick-notes.svelte.ts): the Notes block above the current view, whatever
   * it is. Enter saves and keeps the input open for the next one; Escape or a
   * click outside closes. Escape in a note being edited, the search or a note's « when » only
   * leaves it: a layer over the dialog's keeps the Escape of such a field.
   */
  const OWN_ESCAPE = 'textarea.edit, input.search, .edit-when'
  let held: { remove: () => void } | null = null

  /** Once the focus is in, over the dialog's layer: it closes the same way, unless in such a field. */
  function onfocusin() {
    held ??= pushLayer({
      kind: 'menu',
      key: 'quick-notes-field',
      el: () => (document.activeElement?.closest(OWN_ESCAPE) as HTMLElement | null) ?? undefined,
      close: closeQuickNotes,
      escape: false,
    })
  }
  $effect(() => {
    if (quickNotes.open) return
    held?.remove()
    held = null
  })
</script>

<Modal open={quickNotes.open} key="quick-notes" label={t('Quick notes')} width={620} class="qn" onclose={closeQuickNotes}>
  <div class="sheet" role="presentation" {onfocusin}>
    <Notes snap={live.snapshot} floating opened={quickNotes.opened} />
  </div>
</Modal>

<style>
  /* The Notes block draws its own card: the dialog lends it its place only. */
  :global(.modal.qn) { max-height: 78vh; border: 0; background: none; }
  .sheet { display: flex; flex-direction: column; flex: 1; min-height: 0; }
  .sheet :global(.panel) { flex: 1; min-height: 0; max-height: 78vh; }
</style>
