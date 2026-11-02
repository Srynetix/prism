<script lang="ts">
  import { isPinned, pinsOn, togglePin, type DrawerKind } from '../drawer-pins'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'

  /** A side pane's pin (`[appearance] pin_drawers`): pinned, a click outside no longer closes it. */
  let { kind }: { kind: DrawerKind } = $props()
  const on = $derived(isPinned(kind))
</script>

{#if pinsOn()}
  <button class="pin" class:on onclick={() => togglePin(kind)} aria-pressed={on} title={on ? t('Pinned: only × closes it, not a click outside · unpin') : t('Pin: a click outside no longer closes it')} aria-label={t('Pin')}><Icon name="pin" /></button>
{/if}

<style>
  .pin { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .pin:hover { background: var(--surface-muted); color: var(--text); }
  .pin.on { color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
</style>
