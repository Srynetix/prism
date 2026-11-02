<script lang="ts">
  import { pref, setPref } from '../layout.svelte'
  import { t } from '../i18n.svelte'

  /**
   * A panel section's title that folds it: closed by default, the choice kept for every ticket
   * (`pref`), a few words beside the title (`hint`) saying what it holds while closed.
   */
  let { label, key, open = $bindable(false), hint = '' }: { label: string; key: string; open?: boolean; hint?: string } = $props()

  // The kept choice, and again when the prefs change (another panel folding the same section).
  $effect.pre(() => {
    open = pref<boolean>(key, false)
  })
  function flip() {
    open = !open
    setPref(key, open)
  }
</script>

<h3><button class="fold" onclick={flip} aria-expanded={open} title={open ? t('Fold') : t('Unfold')}><span class="chev" class:open>›</span>{label}{#if hint && !open}<small>{hint}</small>{/if}</button></h3>

<style>
  h3 { margin: 0; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .fold { display: flex; align-items: center; gap: 6px; width: 100%; padding: 0; border: none; background: none; color: inherit; font: inherit; letter-spacing: inherit; text-transform: inherit; text-align: left; cursor: pointer; }
  .fold:hover { color: var(--text); }
  .chev { display: inline-block; width: 8px; transition: transform 0.15s; }
  .chev.open { transform: rotate(90deg); }
  small { margin-left: 4px; font-weight: 400; text-transform: none; letter-spacing: 0; }
  /* Open: the room under the title the other sections have. */
  h3:has(.open) { margin-bottom: 8px; }
</style>
