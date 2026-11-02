<script lang="ts">
  import { keys } from '../keys.svelte'
  import { keyLabel, SHORTCUTS } from '../shortcuts'
  import { openHelp } from '../help.svelte'
  import { keys as forPlatform } from '../platform'
  import { t } from '../i18n.svelte'

  /** Keyboard shortcut help ("?"), and the pending prefix (g…, z…) at the bottom right. */
</script>

{#if keys.help}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="veil" onclick={() => (keys.help = false)}>
    <div class="help" role="dialog" aria-label={t('Keyboard shortcuts')}>
      <h2>{t('Keyboard')} <small>{t('vim-style · Esc to close')}</small></h2>
      <div class="grid">
        {#each SHORTCUTS as s (s.title)}
          <section>
            <h3>{s.title}</h3>
            {#each s.keys as [k, what] (k)}<div class="k"><kbd>{keyLabel(k)}</kbd><span>{what}</span></div>{/each}
          </section>
        {/each}
      </div>
      <p class="more">{t('Guides on every feature, and this list again:')} <button onclick={() => { keys.help = false; openHelp('ref-keys') }}>{t('Help')} ({forPlatform('⌘?')})</button></p>
    </div>
  </div>
{/if}
{#if keys.pending}<div class="pending" aria-live="polite"><kbd>{keys.pending === 'w' ? 'Ctrl-w' : keys.pending}</kbd>…</div>{/if}

<style>
  .veil { position: fixed; inset: 0; z-index: 60; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.35); }
  .help { width: min(760px, calc(100vw - 32px)); max-height: calc(100vh - 64px); overflow: auto; padding: 16px 20px 18px; background: var(--surface); border: 1px solid var(--border); border-top: 2px solid var(--accent); border-radius: var(--r12); box-shadow: var(--shadow-modal); }
  h2 { margin: 0 0 12px; font-size: 15px; font-weight: 600; }
  h2 small { margin-left: 8px; font-size: var(--fs-meta); font-weight: 400; color: var(--text-muted); }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 14px 24px; }
  h3 { margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-muted); }
  .k { display: grid; grid-template-columns: 150px 1fr; gap: 10px; align-items: baseline; padding: 2px 0; font-size: var(--fs-body); }
  kbd { justify-self: start; padding: 0 6px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: var(--r5); background: var(--surface-muted); font-family: var(--mono); font-size: var(--fs-mono); white-space: nowrap; }
  .more { margin: 14px 0 0; padding-top: 10px; border-top: 1px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); }
  .more button { border: none; background: none; padding: 0; color: var(--accent); font: inherit; font-weight: 600; cursor: pointer; }
  .pending { position: fixed; right: 16px; bottom: 16px; z-index: 60; padding: 4px 10px; border-radius: var(--r8); background: var(--surface); border: 1px solid var(--border); box-shadow: var(--shadow-pop); color: var(--text-muted); font-size: var(--fs-body); }
</style>
