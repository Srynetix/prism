<script lang="ts">
  import { t } from '../i18n.svelte'
  import { forgeName, onGithub } from '../forge'
  import { hasCore } from '../bridge'
  import Icon from './Icon.svelte'
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

  const ARM_MS = 3000
  let armed = $state(false)
  /** Second click on a Friday after 4 pm: one more before leaving. */
  let friday = $state(false)
  const lateFriday = () => {
    const d = new Date()
    return risky && featureOn('friday_guard') && d.getDay() === 5 && d.getHours() >= 16
  }
  let busy = $state(false)
  let error = $state<string | null>(null)
  let timer: ReturnType<typeof setTimeout> | undefined

  async function onclick() {
    if (busy) return
    clearTimeout(timer)
    if (!armed) {
      armed = true
      error = null
      timer = setTimeout(() => (armed = false), ARM_MS)
      return
    }
    if (!friday && lateFriday()) {
      friday = true
      timer = setTimeout(() => (armed = friday = false), ARM_MS * 2)
      return
    }
    armed = friday = false
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
    class="play again"
    class:armed
    class:friday
    class:error={!!error}
    disabled={busy}
    {onclick}
    title={error ? t('Could not run {job} again: {error}', { job: job.name, error }) : friday ? t('It’s Friday after 16:00 and the weekend on-call is you') : armed ? t('Click again to run {job} again', { job: job.name }) : t('Already run ({status}, {ago} ago): run {job} again', { job: job.name, status: played.status, ago: ago(played.finished_at) })}
    aria-label={armed ? t('Confirm: run {job} again', { job: job.name }) : t('Run {job} again', { job: job.name })}
  ><Icon name="refresh" />{busy ? t('starting…') : friday ? t('It’s Friday. I like to live dangerously') : armed ? t('run {job} again?', { job: text }) : text}{#if !armed && !busy && !friday}<span class="when" class:ko={played.status === 'failed'}>{played.status === 'success' ? '✓' : '✗'} {ago(played.finished_at)}</span>{/if}</button>
{:else if hasCore()}
  <button
    class="play"
    class:armed
    class:friday
    class:error={!!error}
    disabled={busy}
    {onclick}
    title={error ? (approve ? t('Could not approve {job}: {error}', { job: job.name, error }) : t('Could not start {job}: {error}', { job: job.name, error })) : friday ? t('It’s Friday after 16:00 and the weekend on-call is you') : armed ? (approve ? t('Click again to approve {job}', { job: job.name }) : t('Click again to start {job}', { job: job.name })) : approve ? t('Approve {job}', { job: job.name }) : t('Start {job}', { job: job.name })}
    aria-label={armed ? (approve ? t('Confirm: approve {job}', { job: job.name }) : t('Confirm: start {job}', { job: job.name })) : approve ? t('Approve {job}', { job: job.name }) : t('Start {job}', { job: job.name })}
  ><Icon name="play" />{busy ? t('starting…') : friday ? t('It’s Friday. I like to live dangerously') : armed ? (approve ? t('approve {job}?', { job: text }) : t('start {job}?', { job: text })) : text}</button>
{:else}
  <a class="play" href={job.url} target="_blank" rel="noreferrer" title={t('Open {job} in {forge}', { job: job.name, forge: forgeName() })}><Icon name="play" />{text}</a>
{/if}

<style>
  .play { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); line-height: 1.5; cursor: pointer; text-decoration: none; white-space: nowrap; }
  .play:hover { color: var(--text); border-color: var(--accent); }
  .play.armed { color: var(--accent); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .play.friday { color: var(--ko); border-color: var(--ko); background: color-mix(in srgb, var(--ko) 12%, transparent); }
  .play.error { color: var(--ko); border-color: var(--ko); }
  .play:disabled { cursor: progress; opacity: 0.7; }
  .when { margin-left: 4px; color: var(--ok); opacity: 0.8; }
  .when.ko { color: var(--ko); }
</style>
