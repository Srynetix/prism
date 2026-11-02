<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { armer } from '../armed.svelte'
  import { hasCore } from '../bridge'
  import Icon from './Icon.svelte'
  import { retryJob } from '../logs.svelte'

  /**
   * ⟳ on a finished job: GitLab retries it alone (a new job), without touching the rest of the
   * pipeline. A failed job is retried in one click; a successful job (`rerun`) redoes what it
   * did (a staging deployment…): a first click arms the button, the second retries it.
   */
  let { job, name = '', label = false, rerun = false, project = null }: { job: number; name?: string; label?: boolean; rerun?: boolean; project?: string | null } = $props()

  const armed = armer()
  let busy = $state(false)
  let done = $state(false)
  let error = $state<string | null>(null)

  async function onclick(e: MouseEvent) {
    e.preventDefault()
    e.stopPropagation()
    if (busy) return
    if (rerun && !armed.confirm()) return
    armed.disarm()
    busy = true
    error = null
    try {
      await retryJob(job, project)
      done = true
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }
</script>

{#if hasCore()}
  <button
    class="retry"
    class:error={!!error}
    class:done
    class:armed={armed.is()}
    class:rerun
    disabled={busy || done}
    {onclick}
    title={error ? t('Could not retry: {error}', { error }) : done ? (rerun ? t('{job} started again', { job: name || t('Job') }) : t('{job} retried', { job: name || t('Job') })) : armed.is() ? t('Click again to run it again') : rerun ? t('Run {job} again (it succeeded)', { job: name || t('this job') }) : t('Retry {job} only', { job: name || t('this job') })}
    aria-label={t('Retry {job}', { job: name || t('job') })}
  ><Icon name={done ? 'check' : 'retry'} />{#if label || busy || error || armed.is()}<span>{busy ? (rerun ? t('starting…') : t('retrying…')) : error ? tc('mrs', 'failed') : armed.is() ? t('Run again?') : rerun ? t('Run again') : t('Retry job')}</span>{/if}</button>
{/if}

<style>
  .retry { display: inline-flex; align-items: center; gap: 4px; flex-shrink: 0; height: 18px; padding: 0 4px; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); line-height: 1; cursor: pointer; vertical-align: middle; }
  .retry span { font-size: var(--fs-meta); }
  .retry:hover:not(:disabled) { color: var(--info); border-color: color-mix(in srgb, var(--info) 40%, transparent); }
  .retry.done { color: var(--ok); }
  .retry.armed { color: var(--warn); border-color: var(--warn); background: var(--warn-bg); }
  .retry.error { color: var(--ko); border-color: var(--ko); }
  .retry:disabled { cursor: default; }
</style>
