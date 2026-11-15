<script lang="ts">
  import { t } from '../i18n.svelte'
  import { forgeName, onGithub } from '../forge'
  import { hasCore } from '../bridge'
  import Icon from './Icon.svelte'
  import { armer } from '../armed.svelte'
  import { playJob, replayJob } from '../pipeline-actions'
  import { featureOn } from '../fun.svelte'
  import { ago } from '../time'
  import type { ManualJob, PlayedJob } from '../types'

  /**
   * Two-click manual job launch: the first arms the button for a few seconds, the
   * second sends the order to the forge (on GitHub, approving the deployment the job waits for). `risky` (production tag): on Fridays from 4 pm,
   * Prism asks for one more click, "I like to live dangerously". A job already run (`PlayedJob`):
   * the same clicks run it again (GitLab's retry, GitHub's re-run), its outcome and age beside it.
   */
  let { job, label, risky = false }: { job: ManualJob | PlayedJob; label?: string; risky?: boolean } = $props()
  const text = $derived(label ?? job.name)
  const played = $derived('status' in job ? job : null)
  /** How a job already run ended, in words, and whether it went well. */
  const ok = $derived(played?.status === 'success')
  const outcome = $derived(!played ? '' : played.status === 'success' ? t('passed') : played.status === 'canceled' ? t('canceled') : t('failed'))
  const when = $derived(played?.finished_at ? ago(played.finished_at) : '')

  /** The first click arms `go`; on a Friday after 4 pm the second arms `friday`: one more before leaving. */
  const arm = armer()
  const armed = $derived(arm.is('go'))
  const friday = $derived(arm.is('friday'))
  const lateFriday = () => {
    const d = new Date()
    return risky && featureOn('friday_guard') && d.getDay() === 5 && d.getHours() >= 16
  }
  let busy = $state(false)
  let error = $state<string | null>(null)

  async function onclick() {
    if (busy) return
    if (arm.is('go') && lateFriday()) {
      arm.confirm('friday')
      return
    }
    if (!arm.is('go') && !arm.is('friday')) {
      error = null
      arm.confirm('go')
      return
    }
    arm.disarm()
    busy = true
    try {
      await (played ? replayJob(job.id) : playJob(job.id))
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }
  /** On GitHub, a manual job is a deployment awaiting approval; one already run is run again. */
  const approve = $derived(onGithub() && !played)
</script>

{#if hasCore() && played}
  <button
    class="btn sm play again"
    class:armed
    class:friday
    class:error={!!error}
    class:busy
    disabled={busy}
    {onclick}
    title={error ? t('Could not run {job} again: {error}', { job: job.name, error }) : friday ? t('It’s Friday after 16:00 and the weekend on-call is you') : armed ? t('Click again to run {job} again', { job: job.name }) : when ? t('Already run ({status}, {ago} ago): run {job} again', { job: job.name, status: outcome, ago: when }) : t('Already run ({status}): run {job} again', { job: job.name, status: outcome })}
    aria-label={armed ? t('Click again to run {job} again', { job: job.name }) : t('Run {job} again', { job: job.name })}
  >{#if busy}<span class="spinner"></span>{t('Starting…')}{:else}<Icon name="refresh" />{friday ? t('It’s Friday. I like to live dangerously') : armed ? t('Click again to confirm') : text}{/if}{#if !armed && !busy && !friday}<span class="when" class:ko={!ok}><Icon name={ok ? 'check' : 'fail'} />{when ? ` ${when}` : ''}</span>{/if}</button>
{:else if hasCore()}
  <button
    class="btn sm play"
    class:armed
    class:friday
    class:error={!!error}
    class:busy
    disabled={busy}
    {onclick}
    title={error ? (approve ? t('Could not approve {job}: {error}', { job: job.name, error }) : t('Could not start {job}: {error}', { job: job.name, error })) : friday ? t('It’s Friday after 16:00 and the weekend on-call is you') : armed ? (approve ? t('Click again to approve {job}', { job: job.name }) : t('Click again to start {job}', { job: job.name })) : approve ? t('Approve {job}', { job: job.name }) : t('Start {job}', { job: job.name })}
    aria-label={armed ? (approve ? t('Click again to approve {job}', { job: job.name }) : t('Click again to start {job}', { job: job.name })) : approve ? t('Approve {job}', { job: job.name }) : t('Start {job}', { job: job.name })}
  >{#if busy}<span class="spinner"></span>{t('Starting…')}{:else}<Icon name="play" />{friday ? t('It’s Friday. I like to live dangerously') : armed ? t('Click again to confirm') : text}{/if}</button>
{:else}
  <a class="btn sm play" href={job.url} target="_blank" rel="noreferrer" title={t('Open {job} in {forge}', { job: job.name, forge: forgeName() })}><Icon name="play" />{text}</a>
{/if}

<style>
  .play { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .play:hover { color: var(--text); }
  .play.friday { color: var(--ko); border-color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .play.error { color: var(--ko); border-color: var(--ko); }
  .when { margin-left: 4px; color: var(--ok); opacity: 0.8; }
  .when.ko { color: var(--ko); }
</style>
