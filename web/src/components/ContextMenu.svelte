<script lang="ts">
  import { t } from '../i18n.svelte'
  import { invoke, isDesktop } from '../bridge'
  import { openLink } from '../links'
  import { toastError, writeClipboard } from '../toast.svelte'
  import { anchored, dismissable } from '../popup'
  import { untrack } from 'svelte'
  import { pushLayer } from '../layers.svelte'

  /**
   * The right-click menu, the app's in place of the webview's (Reload, Back, Inspect… on Linux,
   * Aqua's on the Mac): in a field, Cut, Copy, Paste and Select all; on text selected, Copy; on a
   * link, Open and Copy the link; elsewhere, nothing. A component with its own menu (status bar,
   * terminal) handles the right-click first. ⌥ (Alt) + right-click keeps the webview's (Inspect
   * in a dev build).
   */
  type Field = HTMLInputElement | HTMLTextAreaElement
  interface Menu {
    x: number
    y: number
    field: Field | HTMLElement | null
    selected: string
    link: string | null
  }
  let menu = $state<Menu | null>(null)
  let menuEl: HTMLDivElement | undefined = $state()
  const close = () => (menu = null)

  const TEXT = /^(text|search|url|email|tel|password|number|)$/
  const isField = (el: Element | null): el is Field => el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && TEXT.test(el.type))
  const editable = (el: HTMLElement) => (isField(el) ? !el.readOnly && !el.disabled : el.isContentEditable)

  function selectedIn(field: Field | HTMLElement | null): string {
    if (field && isField(field)) {
      // A password is not copied out.
      if (field.type === 'password') return ''
      return field.value.slice(field.selectionStart ?? 0, field.selectionEnd ?? 0)
    }
    return window.getSelection()?.toString() ?? ''
  }

  function onContext(e: MouseEvent) {
    if (e.defaultPrevented || e.altKey) return
    e.preventDefault()
    const target = e.target as HTMLElement | null
    const field = target?.closest<HTMLElement>('input, textarea, [contenteditable]:not([contenteditable="false"])') ?? null
    const usable = field && (isField(field) || field.isContentEditable) ? field : null
    const link = target?.closest<HTMLAnchorElement>('a[href]')?.getAttribute('href') ?? null
    const next = { x: e.clientX, y: e.clientY, field: usable, selected: selectedIn(usable), link: link && /^(https?|claude|obsidian):/i.test(link) ? link : null }
    menu = next.field || next.selected || next.link ? next : null
  }

  $effect(() => {
    document.addEventListener('contextmenu', onContext)
    return () => document.removeEventListener('contextmenu', onContext)
  })
  /**
   * Open, a layer on top: Esc closes the menu only, not a drawer the field is in. The field keeps
   * the focus: the menu is shown to the stack once it has placed the focus (it would take it).
   */
  $effect(() => {
    if (!menu) return
    let ready = false
    const layer = untrack(() => pushLayer({ kind: 'menu', key: 'context-menu', el: () => (ready ? menuEl : undefined), close, outside: false }))
    queueMicrotask(() => (ready = true))
    return layer.remove
  })

  /**
   * The clipboard may answer late (Linux, up to its wait): its field still in the page, and no
   * other field taken up since, or what follows is left undone.
   */
  function stillThere(m: Menu): boolean {
    if (!m.field?.isConnected) return false
    const now = document.activeElement as HTMLElement | null
    return !now || now === m.field || now === document.body || !(isField(now) || now.isContentEditable)
  }
  async function cut(m: Menu) {
    await writeClipboard(m.selected)
    // Deleted only if what was copied is still what is selected: else the text stays, copied.
    if (!stillThere(m) || selectedIn(m.field) !== m.selected) return
    m.field?.focus()
    document.execCommand('delete')
  }
  async function paste(m: Menu) {
    const text = isDesktop() ? await invoke<string>('paste_text') : await navigator.clipboard.readText()
    if (!stillThere(m)) return
    m.field?.focus()
    // As typed: the field's undo and its `input` event.
    document.execCommand('insertText', false, text)
  }
  function selectAll(m: Menu) {
    if (m.field && isField(m.field)) {
      m.field.focus()
      m.field.select()
    } else {
      m.field?.focus()
      document.execCommand('selectAll')
    }
  }
  /** A menu's action, once it is closed; its failure told when it says how (`failed`). */
  function run(action: () => unknown, failed?: (error: string) => string) {
    close()
    // From a `then`: an action that throws at once is caught too.
    void Promise.resolve()
      .then(action)
      .catch((e) => failed && toastError(failed(String(e))))
  }
  // The clipboard read or written through the core may fail (Linux: one that does not answer).
  const notCopied = (error: string) => t('Could not copy: {error}', { error })
  const notPasted = (error: string) => t('Could not paste: {error}', { error })

  /** The point clicked, as the element the menu is placed against. */
  const at = (m: Menu) => ({ getBoundingClientRect: () => new DOMRect(m.x, m.y, 0, 0) }) as HTMLElement
</script>

<!-- As an item of its own, not `{@const m = menu}`: closed, the menu would be read as null once more. -->
{#each menu ? [menu] : [] as m (m)}
  {@const canEdit = !!m.field && editable(m.field)}
  <!-- The field keeps its focus and its selection while a choice is clicked. -->
  <div class="ctxmenu popover" role="menu" tabindex="-1" bind:this={menuEl} onmousedown={(e) => e.preventDefault()} {@attach anchored(() => at(m), { align: 'start', gap: 0 })} {@attach dismissable(close, { resize: true, escape: false })}>
    {#if m.link}
      <button class="mi" role="menuitem" onclick={() => run(() => openLink(m.link!))}>{t('Open link')}</button>
      <button class="mi" role="menuitem" onclick={() => run(() => writeClipboard(m.link!), notCopied)}>{t('Copy link')}</button>
      {#if m.field || m.selected}<hr />{/if}
    {/if}
    {#if m.field}
      <button class="mi" role="menuitem" disabled={!canEdit || !m.selected} onclick={() => run(() => cut(m), notCopied)}>{t('Cut')}</button>
    {/if}
    {#if m.field || m.selected}
      <button class="mi" role="menuitem" disabled={!m.selected} onclick={() => run(() => writeClipboard(m.selected), notCopied)}>{t('Copy')}</button>
    {/if}
    {#if m.field}
      <button class="mi" role="menuitem" disabled={!canEdit} onclick={() => run(() => paste(m), notPasted)}>{t('Paste')}</button>
      <hr />
      <button class="mi" role="menuitem" onclick={() => run(() => selectAll(m))}>{t('Select all')}</button>
    {/if}
  </div>
{/each}

<style>
  .ctxmenu { position: fixed; min-width: 150px; outline: none; }
</style>
