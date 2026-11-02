<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { parseShortcut, shortcutLabel } from '../../shortcut'
  import { isMac } from '../../platform'

  /**
   * Shortcut recorder: a click, then the key. A dead key (` on AZERTY) is
   * kept under its physical name (`Backquote`); Escape cancels; × reverts to the default.
   */
  let { value, onchange, fallback }: { value: string; onchange: (v: string) => void; fallback: string } = $props()
  let recording = $state(false)

  function onkeydown(e: KeyboardEvent) {
    if (!recording) return
    e.preventDefault()
    e.stopPropagation()
    if (['Meta', 'Control', 'Alt', 'Shift'].includes(e.key)) return
    if (e.key === 'Escape') return (e.currentTarget as HTMLElement).blur()
    // `Cmd` is ⌘ on macOS but Ctrl elsewhere: there the meta key is written `Super`.
    const mods = [e.metaKey && (isMac ? 'Cmd' : 'Super'), e.ctrlKey && 'Ctrl', e.altKey && 'Alt', (e.metaKey || e.ctrlKey || e.altKey) && e.shiftKey && 'Shift'].filter(Boolean)
    // With ⌥, the key gives another character (⌥T → †): the physical key is safer.
    const key = e.key === 'Dead' || e.key === 'Unidentified' || e.altKey ? e.code.replace(/^Key|^Digit/, '') : e.key === ' ' ? 'Space' : e.key
    onchange([...mods, key].join('+'))
    ;(e.currentTarget as HTMLElement).blur()
  }
</script>

<div class="rec" role="button" tabindex="0" title={t('Click, then press the shortcut')} onfocus={() => (recording = true)} onblur={() => (recording = false)} {onkeydown}>
  {#if recording}<span class="ph">{t('Press the shortcut…')}</span>{:else}<kbd>{shortcutLabel(parseShortcut(value))}</kbd><span class="raw">{value}</span>{/if}
  {#if value !== fallback && !recording}<button class="clear" title={t('Back to {key}', { key: shortcutLabel(parseShortcut(fallback)) })} onmousedown={(e) => e.preventDefault()} onclick={() => onchange(fallback)}><Icon name="close" /></button>{/if}
</div>

<style>
  .rec { display: inline-flex; align-items: center; gap: 8px; min-width: 200px; height: 30px; padding: 0 6px 0 10px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--bg); cursor: pointer; outline: none; }
  .rec:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  kbd { padding: 2px 7px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: var(--r5); background: var(--surface); color: var(--text); font: 600 12px var(--mono); }
  .raw { color: var(--text-muted); font: var(--fs-tag) var(--mono); }
  .ph { color: var(--accent); font-size: var(--fs-meta); }
  .clear { display: inline-flex; margin-left: auto; padding: 2px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .clear:hover { color: var(--text); }
</style>
