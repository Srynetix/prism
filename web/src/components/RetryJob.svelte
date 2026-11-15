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
    class="btn sm ghost retry"
    class:error={!!error}
    class:busy
    class:done
    class:armed={armed.is()}
    class:rerun
    disabled={busy || done}
    {onclick}
    title={error ? t('Could not retry: {error}', { error }) : done ? (rerun ? t('{job} started again', { job: name || t('Job') }) : t('{job} retried', { job: name || t('Job') })) : armed.is() ? t('Click again to run it again') : rerun ? t('Run {job} again (it succeeded)', { job: name || t('this job') }) : t('Retry {job} only', { job: name || t('this job') })}
    aria-label={t('Retry {job}', { job: name || t('job') })}
  >{#if busy}<span class="spinner"></span>{:else}<Icon name={done ? 'check' : 'retry'} />{/if}{#if label || busy || error || armed.is()}<span>{busy ? (rerun ? t('starting…') : t('retrying…')) : error ? tc('mrs', 'failed') : armed.is() ? t('Click again to confirm') : rerun ? t('Run again') : t('Retry job')}</span>{/if}</button>
{/if}

<style>
  .retry { padding: 0 var(--sp-3); vertical-align: middle; }
  .retry:hover:not(:disabled) { color: var(--info); }
  .retry.done { color: var(--ok); opacity: 1; }
  .retry.error { color: var(--ko); border-color: var(--ko); }
</style>
