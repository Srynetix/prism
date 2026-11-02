<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { armer } from '../armed.svelte'
  import Icon from './Icon.svelte'
  import { cancelPipeline, canCancel } from '../pipeline-actions'

  /**
   * Stopping a pipeline in two clicks: the first arms the button for a few seconds, the second
   * sends the order to GitLab. No dialog box, but no stop on a stray click.
   */
  let { id, status, label = false }: { id: number; status: string | null; label?: boolean } = $props()

  const armed = armer()
  let busy = $state(false)
  let error = $state<string | null>(null)

  async function onclick(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    if (busy) return
    if (!armed.confirm()) {
      error = null
      return
    }
    busy = true
    try {
      await cancelPipeline(id)
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }
</script>

{#if canCancel(status) || error}
  <button
    class="cancel"
    class:armed={armed.is()}
    class:error={!!error}
    {onclick}
    disabled={busy}
    title={error ? t('Could not stop the pipeline: {error}', { error }) : armed.is() ? t('Click again to stop the pipeline') : t('Stop this pipeline')}
    aria-label={armed.is() ? t('Confirm: stop the pipeline') : t('Stop the pipeline')}
  ><Icon name="stop" />{#if label || armed.is()}<span>{busy ? t('stopping…') : error ? tc('mrs', 'failed') : armed.is() ? t('stop?') : t('Stop pipeline')}</span>{/if}</button>
{/if}

<style>
  .cancel { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; height: 18px; padding: 0 5px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 1; cursor: pointer; }
  .cancel span { font-size: var(--fs-meta); }
  .cancel:hover { color: var(--ko); border-color: color-mix(in srgb, var(--ko) 40%, transparent); }
  .cancel.armed, .cancel.error { color: var(--ko); border-color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .cancel:disabled { opacity: 0.6; cursor: progress; }
</style>
