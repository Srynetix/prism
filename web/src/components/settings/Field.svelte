<script lang="ts">
  import type { Snippet } from 'svelte'
  import { discardValue, hasInherited, originOf, packLabel, resetValue, settingsView } from '../../settings.svelte'
  import { t, tc } from '../../i18n.svelte'

  /**
   * A settings row: label, explanation and TOML key on the left, the control on the right.
   * `path` (config.toml key): where its value comes from — the default, a pack (named), config.toml,
   * or a change not saved yet (a key taken out of the file too) — with ↺ to go back (the change
   * dropped, or the key taken out of config.toml: the pack's value, otherwise the default). The key
   * a search result opened the page on is marked. Without `path`: an app preference (theme, Fun), applied immediately.
   */
  let {
    label, hint, path, section, children,
  }: { label: string; hint?: string; path?: string[]; /** Its section's id: a key shown in two is marked in the one opened. */ section?: string; children: Snippet } = $props()
  const origin = $derived(path ? originOf(path) : null)
  const dotted = $derived(path?.join('.') ?? '')
  const focused = $derived(!!dotted && settingsView.focus === dotted && (!section || !settingsView.focusIn || settingsView.focusIn === section))
</script>

<div class="field" class:changed={origin === 'changed'} class:focused data-key={dotted || undefined}>
  <div class="lbl">
    <b>{label}</b>
    {#if hint}<small>{hint}</small>{/if}
    {#if path}<span class="key">{dotted}</span>{/if}
  </div>
  <div class="ctl">
    {@render children()}
    {#if path}
      <span class="state">
        {#if origin === 'changed'}
          <span class="badge warn">{t('not saved')}</span>
          <button class="reset" onclick={() => discardValue(path)} title={t('Drop this change: the saved value comes back')} aria-label={t('Drop this change')}>↺</button>
        {:else if origin === 'file'}
          <span class="badge file" title={t('Written in config.toml')}>config.toml</span>
          <button class="reset" onclick={() => resetValue(path)} title={hasInherited(path) ? t('Remove the key from config.toml: the pack’s value applies') : t('Remove the key from config.toml: the default applies')} aria-label={t('Reset to default')}>↺</button>
        {:else if origin === 'pack'}
          <span class="badge pack" title={t('Given by a pack: changing it writes your own value in config.toml')}>{packLabel(path)}</span>
        {:else}
          <span class="badge def">{tc('settings', 'default')}</span>
        {/if}
      </span>
    {/if}
  </div>
</div>

<style>
  .field { display: grid; grid-template-columns: minmax(160px, 240px) minmax(0, 1fr); gap: 4px 18px; padding: 11px 14px; border-bottom: 1px solid var(--border); align-items: start; scroll-margin: 60px; }
  .field:last-child { border-bottom: none; }
  .field.changed { background: color-mix(in srgb, var(--warn) 6%, transparent); box-shadow: inset 3px 0 0 var(--warn); }
  .field.focused { animation: focus 2.4s ease-out; box-shadow: inset 3px 0 0 var(--accent); }
  @keyframes focus { from { background: color-mix(in srgb, var(--accent) 22%, transparent); } to { background: transparent; } }
  .lbl { display: flex; flex-direction: column; gap: 2px; }
  .lbl b { display: flex; align-items: center; gap: 6px; font-weight: 550; font-size: var(--fs-body); }
  .lbl small { color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.35; }
  .key { font: var(--fs-tag) var(--mono); color: var(--text-muted); opacity: 0.75; user-select: text; }
  .ctl { display: flex; flex-direction: column; align-items: flex-start; gap: 6px; min-width: 0; }
  .state { display: inline-flex; align-items: center; gap: 6px; min-height: 16px; }
  .badge { padding: 0 6px; border-radius: var(--r9); font-size: var(--fs-tag); font-weight: 500; line-height: 16px; }
  .badge.def { background: var(--surface-muted); color: var(--text-muted); }
  .badge.pack { background: var(--info-bg); color: var(--info); }
  .badge.file { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); font-family: var(--mono); }
  .badge.warn { background: color-mix(in srgb, var(--warn) 16%, transparent); color: var(--warn); }
  .reset { padding: 0 4px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); line-height: 16px; cursor: pointer; }
  .reset:hover { color: var(--accent); background: var(--surface-muted); }
  @container (max-width: 620px) { .field { grid-template-columns: minmax(0, 1fr); } }
</style>
