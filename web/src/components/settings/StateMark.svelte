<script module lang="ts">
  import { t } from '../../i18n.svelte'

  /**
   * A Settings page's state, the one mark for it everywhere (the pages' list, the overview, the
   * search, a page's anchors): unsaved first, then a failed check, then its switch.
   */
  export type PageState = 'changed' | 'attention' | 'on' | 'off'

  export function pageState(dirty: boolean, attention: boolean, on: boolean | undefined): PageState | undefined {
    if (dirty) return 'changed'
    if (attention) return 'attention'
    return on === undefined ? undefined : on ? 'on' : 'off'
  }

  export const STATE_TITLE: Record<PageState, string> = {
    changed: t('Unsaved changes'),
    attention: t('A check failed, or this source could not be read'),
    on: t('On'),
    off: t('Off'),
  }
</script>

<script lang="ts">
  /** A dot: lit green when on, hollow when off, amber when not saved, red when to check. */
  let { state }: { state: PageState | undefined } = $props()
</script>

{#if state}<span class="dot mark {state}" class:ok={state === 'on'} class:warn={state === 'changed'} class:ko={state === 'attention'} title={STATE_TITLE[state]} role="img" aria-label={STATE_TITLE[state]}></span>{/if}

<style>
  .mark.off { background: transparent; box-shadow: inset 0 0 0 1.5px var(--text-muted); opacity: 0.7; }
</style>
