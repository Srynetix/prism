<script lang="ts">
  import { t } from '../i18n.svelte'
  import { invoke, isDesktop } from '../bridge'
  import { openLink } from '../links'
  import { writeClipboard } from '../toast.svelte'
  import { anchored, dismissable } from '../popup'

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

  /** Esc closes the menu only, not a popup the field is in. */
  function onKey(e: KeyboardEvent) {
    if (menu && e.key === 'Escape') {
      e.stopImmediatePropagation()
      close()
    }
  }

  $effect(() => {
    document.addEventListener('contextmenu', onContext)
    document.addEventListener('keydown', onKey, true)
    return () => {
      document.removeEventListener('contextmenu', onContext)
      document.removeEventListener('keydown', onKey, true)
    }
  })

  async function cut(m: Menu) {
    await writeClipboard(m.selected)
    m.field?.focus()
    document.execCommand('delete')
  }
  async function paste(m: Menu) {
    const text = isDesktop() ? await invoke<string>('paste_text') : await navigator.clipboard.readText()
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
  function run(action: () => unknown) {
    close()
    void Promise.resolve(action()).catch(() => {})
  }

  /** The point clicked, as the element the menu is placed against. */
  const at = (m: Menu) => ({ getBoundingClientRect: () => new DOMRect(m.x, m.y, 0, 0) }) as HTMLElement
</script>

<!-- As an item of its own, not `{@const m = menu}`: closed, the menu would be read as null once more. -->
{#each menu ? [menu] : [] as m (m)}
  {@const canEdit = !!m.field && editable(m.field)}
  <!-- The field keeps its focus and its selection while a choice is clicked. -->
  <div class="ctxmenu" role="menu" tabindex="-1" onmousedown={(e) => e.preventDefault()} {@attach anchored(() => at(m), { align: 'start', gap: 0 })} {@attach dismissable(close, { resize: true })}>
    {#if m.link}
      <button role="menuitem" onclick={() => run(() => openLink(m.link!))}>{t('Open link')}</button>
      <button role="menuitem" onclick={() => run(() => writeClipboard(m.link!))}>{t('Copy link')}</button>
      {#if m.field || m.selected}<hr />{/if}
    {/if}
    {#if m.field}
      <button role="menuitem" disabled={!canEdit || !m.selected} onclick={() => run(() => cut(m))}>{t('Cut')}</button>
    {/if}
    {#if m.field || m.selected}
      <button role="menuitem" disabled={!m.selected} onclick={() => run(() => writeClipboard(m.selected))}>{t('Copy')}</button>
    {/if}
    {#if m.field}
      <button role="menuitem" disabled={!canEdit} onclick={() => run(() => paste(m))}>{t('Paste')}</button>
      <hr />
      <button role="menuitem" onclick={() => run(() => selectAll(m))}>{t('Select all')}</button>
    {/if}
  </div>
{/each}

<style>
  .ctxmenu { position: fixed; z-index: 200; min-width: 150px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); color: var(--text); font-size: var(--fs-body); outline: none; }
  .ctxmenu button { display: flex; align-items: center; width: 100%; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .ctxmenu button:hover:not(:disabled) { background: var(--surface-muted); }
  .ctxmenu button:disabled { color: var(--text-muted); opacity: 0.6; cursor: default; }
  .ctxmenu hr { margin: 4px 6px; border: none; border-top: 1px solid var(--border); }
</style>
