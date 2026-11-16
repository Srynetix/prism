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
    class="btn sm ghost cancel"
    class:armed={armed.is()}
    class:error={!!error}
    class:busy
    {onclick}
    disabled={busy}
    title={error ? t('Could not stop the pipeline: {error}', { error }) : armed.is() ? t('Click again to stop the pipeline') : t('Stop this pipeline')}
    aria-label={armed.is() ? t('Click again to stop the pipeline') : t('Stop the pipeline')}
  >{#if busy}<span class="spinner"></span>{:else}<Icon name="stop" />{/if}{#if label || armed.is() || busy}<span>{busy ? t('stopping…') : error ? tc('mrs', 'failed') : armed.is() ? t('Click again to confirm') : t('Stop pipeline')}</span>{/if}</button>
{/if}

<style>
  .cancel { padding: 0 var(--sp-3); }
  .cancel:hover { color: var(--ko); }
  .cancel.error { color: var(--ko); border-color: var(--ko); }
</style>
