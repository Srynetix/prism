<script module lang="ts">
  import { cfg } from '../../settings.svelte'

  /**
   * What the panel shows for a view (`file`, `pack:<name>`, `merged`): a pack's view whose pack is
   * gone, or the merged one without a pack, falls back to config.toml.
   */
  export function tomlModeOf(view: string): 'file' | 'pack' | 'merged' {
    const packs = cfg.file?.pack_files ?? []
    if (view === 'merged' && packs.length) return 'merged'
    return packs.some((p) => `pack:${p.name}` === view) ? 'pack' : 'file'
  }
</script>

<script lang="ts">
  import { armer } from '../../armed.svelte'
  import { isDesktop } from '../../bridge'
  import { t, tc } from '../../i18n.svelte'
  import { setPref } from '../../layout.svelte'
  import { cancelRaw, changes, editable, editRaw, mergedToml, raw, saveRaw } from '../../settings.svelte'
  import ArmedLabel from '../ArmedLabel.svelte'
  import Icon from '../Icon.svelte'
  import TomlPreview from './TomlPreview.svelte'

  /**
   * The TOML panel beside the settings: config.toml (what saving would write, or edited by hand),
   * a pack's file, read-only, or the packs and config.toml merged as the app reads them, the
   * unsaved changes included. `view` is the one shown, remembered.
   */
  let { view = $bindable() }: { view: string } = $props()

  const tomlPack = $derived(cfg.file?.pack_files.find((p) => `pack:${p.name}` === view))
  const mode = $derived(tomlModeOf(view))
  function pick(v: string) {
    view = v
    setPref('settings.tomlView', v)
  }
  let merged = $state({ text: '', error: '' })
  $effect(() => {
    if (mode !== 'merged' || !isDesktop()) return
    let gone = false
    mergedToml(cfg.preview).then(
      (text) => !gone && (merged = { text, error: '' }),
      (e) => !gone && (merged = { text: '', error: String(e) }),
    )
    return () => (gone = true)
  })

  const rawChanged = $derived(raw.on && raw.text !== raw.base)
  /** Cancel throws the typed text away: a second click within 3 s once it differs from the file. */
  const armed = armer()
  function askCancel() {
    if (rawChanged && !armed.confirm('cancel')) return
    armed.disarm()
    cancelRaw()
  }
</script>

<aside class="file">
  <div class="th">
    {#if cfg.file?.pack_files.length}
      <select class="tsel" value={mode === 'pack' ? view : mode} onchange={(e) => pick(e.currentTarget.value)} aria-label={t('File shown')}>
        <option value="file">config.toml</option>
        {#each cfg.file.pack_files as p (p.name)}<option value="pack:{p.name}">{t('Pack {name}', { name: p.name })}</option>{/each}
        <option value="merged">{tc('settings', 'Merged')}</option>
      </select>
    {:else}<b>config.toml</b>{/if}
    {#if mode === 'pack'}
      <span class="mono grow">{tomlPack?.path.replace(/^\/Users\/[^/]+/, '~')}</span><span title={t('Update it from Settings › General › Packs')}>{t('read-only')}</span>
    {:else if mode === 'merged'}
      <span class="grow" title={t('Tables merge key by key, [[repos]] by name, anything else is replaced; [pack] and the sections a pack may not set are left out. The defaults are not shown.')}>{t('packs, then config.toml on top')}</span><span>{changes().length ? t('{n} unsaved', { n: changes().length }) : t('saved')}</span>
    {:else if raw.on}
      <span class="grow">{rawChanged ? t('edited, not saved') : t('as on disk')}</span>
      <button class="btn sm ghost" class:armed={armed.is('cancel')} onclick={askCancel} disabled={cfg.saving} title={rawChanged ? t('Throw away what you typed (click twice)') : undefined}><ArmedLabel label={t('Cancel')} armedLabel={t('Click again to confirm')} armed={armed.is('cancel')} /></button>
      <button class="btn sm primary" onclick={saveRaw} disabled={cfg.saving || !rawChanged} title={t('Write config.toml as typed (⌘S): the app rereads it, the previous version goes to config.toml.bak')}>{cfg.saving ? t('Saving…') : t('Save')}</button>
    {:else}
      <span class="mono grow">{cfg.file?.path.replace(/^\/Users\/[^/]+/, '~') ?? ''}</span><span>{changes().length ? t('{n} unsaved', { n: changes().length }) : t('saved')}</span>
      {#if editable() && cfg.file}<button class="btn sm ghost" onclick={editRaw} disabled={!!changes().length} title={changes().length ? t('Save or discard the changes of the form first') : t('Edit config.toml by hand, here')}><Icon name="edit" size={11} />{t('Edit')}</button>{/if}
    {/if}
  </div>
  {#if mode === 'pack' && tomlPack}
    <TomlPreview text={tomlPack.text} original={tomlPack.text} />
  {:else if mode === 'merged'}
    {#if merged.error}<p class="note err pad">{merged.error}</p>
    {:else}<TomlPreview text={merged.text || (cfg.file?.merged ?? '')} original={cfg.file?.merged ?? ''} />{/if}
  {:else if raw.on}
    {#if raw.error}<p class="note err pad">{raw.error}</p>{/if}
    <!-- svelte-ignore a11y_autofocus -->
    <textarea class="raw" bind:value={raw.text} spellcheck="false" autocomplete="off" autofocus aria-label={t('config.toml, editable')}></textarea>
  {:else}
    <TomlPreview text={cfg.preview} original={cfg.file?.text ?? ''} />
  {/if}
</aside>

<style>
  .file { display: flex; flex-direction: column; min-width: 0; overflow: hidden; background: #121211; border-left: 1px solid var(--border); }
  .th { display: flex; align-items: center; gap: 8px; padding: 9px 12px; border-bottom: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-meta); }
  .th b { color: var(--text); font-size: var(--fs-body); }
  .th .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .raw { flex: 1; min-height: 0; margin: 0; padding: 10px 12px; border: none; outline: none; resize: none; background: transparent; color: #c9c9cf; font: 11px/1.6 var(--mono); white-space: pre; overflow: auto; tab-size: 2; }
  .tsel { padding: 1px 4px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); font-weight: 600; cursor: pointer; }
  .note { margin: 0 0 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .note.err { color: var(--ko); }
  .note.pad { padding: 10px 12px; }
  .grow { flex: 1; }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
</style>
