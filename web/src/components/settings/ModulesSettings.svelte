<script lang="ts">
  import Icon from '../Icon.svelte'
  import { blockLabel } from '../../views'
  import { moduleStates } from '../../modules/modules.svelte'
  import { t } from '../../i18n.svelte'

  /**
   * Settings › Packs › Modules: a team's blocks loaded at run time (`modules/<id>/` next to
   * config.toml), as this window loaded them: their blocks, or why not. A module added or replaced
   * shows once the window reloads.
   */
  const list = $derived(moduleStates())
</script>

<div class="card">
  <div class="ch"><b>{t('Modules')}</b><small>{t('a team’s blocks loaded without rebuilding Prism, from modules/ next to config.toml')}</small></div>
  {#each list as m (m.id)}
    <div class="module">
      <div class="minfo">
        <b>{m.id}{#if m.version}<span class="ver mono">v{m.version}</span>{/if}</b>
        {#if m.description}<small>{m.description}</small>{/if}
        {#if m.state === 'on'}
          <small>{m.blocks.length ? t('Its blocks: {names}', { names: m.blocks.filter((b) => !m.clashes.includes(b)).map(blockLabel).join(', ') || '—' }) : t('No block.')}</small>
        {:else}
          <span class="hint warn"><Icon name="warning" /> {m.state === 'refused' ? t('Not loaded: {reason}', { reason: m.reason ?? '' }) : t('Failed to load: {reason}', { reason: m.reason ?? '' })}</span>
        {/if}
        {#each m.clashes as block (block)}<span class="hint warn"><Icon name="warning" /> {t('Its block {block} is not shown: that id is already taken.', { block })}</span>{/each}
      </div>
      <span class="state" class:on={m.state === 'on' && !m.clashes.length}>{m.state === 'on' ? t('Loaded') : m.state === 'refused' ? t('Refused') : t('Failed')}</span>
    </div>
  {:else}
    <p class="hint pad">{t('No module: put a module’s folder in modules/, next to config.toml, then reload the window.')}</p>
  {/each}
  <div class="reload">
    <span class="hint">{t('A module added, replaced or removed shows once the window reloads.')}</span>
    <button class="btn" onclick={() => location.reload()} title={t('Load the window again, its modules with it')}>{t('Reload the window')}</button>
  </div>
</div>

<style>
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); flex-shrink: 0; }
  .ch { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .ch b { font-size: var(--fs-title); }
  .ch small { color: var(--text-muted); font-size: var(--fs-meta); }
  .module { display: flex; align-items: flex-start; gap: 12px; padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .minfo { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .minfo b { display: flex; align-items: baseline; gap: 8px; font-weight: 550; font-size: var(--fs-body); }
  .minfo small { color: var(--text-muted); font-size: var(--fs-meta); overflow-wrap: anywhere; }
  .ver { color: var(--text-muted); font-weight: 400; }
  .hint { font-size: var(--fs-meta); color: var(--text-muted); overflow-wrap: anywhere; }
  .hint.warn { color: var(--warn); }
  .hint.pad { margin: 0; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .state { font-size: var(--fs-meta); color: var(--ko); white-space: nowrap; }
  .state.on { color: var(--ok); }
  .reload { display: flex; gap: 12px; align-items: center; justify-content: space-between; padding: 10px 14px; }
</style>
