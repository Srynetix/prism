<script lang="ts">
  import { keys } from '../keys.svelte'
  import { shortcutKeys, shortcutSections } from '../shortcuts'
  import { openHelp } from '../help.svelte'
  import { t } from '../i18n.svelte'
  import Modal from './ui/Modal.svelte'

  /**
   * Keyboard shortcut help ("?"), drawn from the registry (shortcuts.ts) as it is now: the pages'
   * digits, the keys set in config.toml. And the pending prefix (g…, z…) at the bottom right.
   */
  const sections = $derived(keys.help ? shortcutSections() : [])
  const close = () => (keys.help = false)
</script>

<Modal open={keys.help} key="key-help" label={t('Keyboard shortcuts')} title={t('Keyboard')} width={720} onclose={close}>
  {#snippet head()}<small class="muted">{t('vim-style · Esc to close')}</small>{/snippet}
  <div class="body">
    <div class="grid">
      {#each sections as s (s.title)}
        <section>
          <h3>{s.title}</h3>
          {#each s.keys as [k, what] (k + what)}<div class="k"><kbd>{k}</kbd><span>{what}</span></div>{/each}
        </section>
      {/each}
    </div>
  </div>
  {#snippet footer()}
    <span class="more muted">{t('Guides on every feature, and this list again:')}</span>
    <button class="btn sm ghost" onclick={() => { close(); openHelp('ref-keys') }}>{t('Help')} ({shortcutKeys('help')})</button>
  {/snippet}
</Modal>
{#if keys.pending}<div class="pending" aria-live="polite"><kbd>{keys.pending === 'w' ? 'Ctrl-w' : keys.pending}</kbd>…</div>{/if}

<style>
  .body { min-height: 0; overflow: auto; padding: var(--sp-5) var(--sp-6) var(--sp-6); }
  small { font-size: var(--fs-meta); white-space: nowrap; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 14px 24px; }
  h3 { margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-muted); }
  .k { display: grid; grid-template-columns: 140px 1fr; gap: 10px; align-items: baseline; padding: 2px 0; font-size: var(--fs-body); }
  kbd { justify-self: start; padding: 0 6px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: var(--r5); background: var(--surface-muted); font-family: var(--mono); font-size: var(--fs-mono); white-space: nowrap; }
  .more { margin-right: auto; font-size: var(--fs-meta); }
  .pending { position: fixed; right: 16px; bottom: 16px; z-index: var(--z-modal); padding: 4px 10px; border-radius: var(--r8); background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-pop); color: var(--text-muted); font-size: var(--fs-body); }
</style>
