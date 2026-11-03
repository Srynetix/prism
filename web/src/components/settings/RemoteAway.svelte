<script lang="ts">
  import { onMount } from 'svelte'
  import { invoke } from '../../bridge'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import type { AwayCheck } from '../../bindings/AwayCheck'

  /**
   * Settings › Remote › Away: before leaving the computer for days, what keeps it within reach of the
   * phone — its sleep, its restarts (macOS updates, FileVault, a power cut), Prism itself. Read on
   * opening and on demand (after a save above); nothing is changed here.
   */
  let checks = $state<AwayCheck[] | null>(null)
  let error = $state<string | null>(null)
  let reading = $state(false)
  async function read() {
    reading = true
    error = null
    try {
      checks = await invoke<AwayCheck[]>('remote_away_checks')
    } catch (e) {
      error = String(e)
    } finally {
      reading = false
    }
  }
  onMount(() => void read())
</script>

<div class="card">
  <div class="head">
    <b>{t('Before leaving')}</b>
    <button class="btn" onclick={read} disabled={reading}><Icon name="retry" />{reading ? t('Checking…') : t('Check again')}</button>
  </div>
  {#if error}<p class="ko pad">{error}</p>{/if}
  {#each checks ?? [] as c, i (i)}
    <div class="row" class:warn={!c.ok}><Icon name={c.ok ? 'check' : 'warning'} /><span>{c.text}</span></div>
  {/each}
</div>

<style>
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 4px 12px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .head { display: flex; align-items: center; justify-content: space-between; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .row { display: flex; align-items: flex-start; gap: 8px; padding: 8px 14px; font-size: 13px; color: var(--text); }
  .row + .row { border-top: 1px solid var(--border); }
  .row :global(svg) { flex: none; margin-top: 2px; color: var(--ok); }
  .row.warn :global(svg) { color: var(--warn); }
  .pad { padding: 8px 14px; margin: 0; }
  .ko { color: var(--ko); }
</style>
