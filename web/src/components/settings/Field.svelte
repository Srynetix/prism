<script lang="ts">
  import type { Snippet } from 'svelte'
  import { hasInherited, isChanged, isSet, packLabel, resetValue } from '../../settings.svelte'
  import { t, tc } from '../../i18n.svelte'

  /**
   * A settings row: label, explanation and TOML key on the left, the control on the right.
   * `path` (config.toml key): highlighted if modified, "default" (or "from pack …" when a pack
   * gives it) if absent from the file, with a way to revert. Without `path`: an app preference (theme, Fun), applied immediately.
   */
  let {
    label, hint, path, children,
  }: { label: string; hint?: string; path?: string[]; children: Snippet } = $props()
  const set = $derived(path ? isSet(path) : true)
  const inherited = $derived(path ? hasInherited(path) : false)
</script>

<div class="field" class:changed={path && isChanged(path)}>
  <div class="lbl">
    <b>{label}</b>
    {#if hint}<small>{hint}</small>{/if}
    {#if path}<span class="key">{path.at(-1)}</span>{/if}
  </div>
  <div class="ctl">
    {@render children()}
    {#if path}
      <span class="state">
        {#if !set}<span class="badge def" class:pack={inherited}>{inherited ? t('from {pack}', { pack: packLabel() }) : tc('settings', 'default')}</span>
        {:else if inherited}<button class="reset" onclick={() => resetValue(path)} title={t('Remove the key from config.toml: the pack’s value applies')}>{t('Back to pack value')}</button>
        {:else}<button class="reset" onclick={() => resetValue(path)} title={t('Remove the key from config.toml: the default applies')}>{t('Reset to default')}</button>{/if}
      </span>
    {/if}
  </div>
</div>

<style>
  .field { display: grid; grid-template-columns: minmax(160px, 240px) minmax(0, 1fr); gap: 4px 18px; padding: 11px 14px; border-bottom: 1px solid var(--border); align-items: start; }
  .field:last-child { border-bottom: none; }
  .field.changed { background: color-mix(in srgb, var(--warn) 6%, transparent); box-shadow: inset 3px 0 0 var(--warn); }
  .lbl { display: flex; flex-direction: column; gap: 2px; }
  .lbl b { display: flex; align-items: center; gap: 6px; font-weight: 550; font-size: var(--fs-body); }
  .lbl small { color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.35; }
  .key { font: var(--fs-tag) var(--mono); color: var(--text-muted); opacity: 0.75; }
  .ctl { display: flex; flex-direction: column; align-items: flex-start; gap: 6px; min-width: 0; }
  .state { min-height: 16px; }
  .badge { padding: 0 6px; border-radius: var(--r9); font-size: 10px; font-weight: 500; line-height: 16px; }
  .badge.def { background: var(--surface-muted); color: var(--text-muted); }
  .badge.def.pack { background: var(--info-bg); color: var(--info); }
  .reset { padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  @container (max-width: 620px) { .field { grid-template-columns: minmax(0, 1fr); } }
</style>
