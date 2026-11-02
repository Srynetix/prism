<script lang="ts">
  import { t } from '../i18n.svelte'
  import { forgeName } from '../forge'
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { tick } from 'svelte'
  import { closeLogs, LIVE, logs, RERUNNABLE, RETRYABLE, selectJob, type JobInfo } from '../logs.svelte'
  import Icon from './Icon.svelte'
  import RetryJob from './RetryJob.svelte'
  import type { IconName } from '../icons'
  import { LogLines } from '../ansi'
  import { span } from '../time'
  import { pref, setPref } from '../layout.svelte'
  import { claudeClick, diagnoseJobAction } from '../claude-actions'
  import { agentIcon } from '../agent-cli'

  /**
   * GitLab logs drawer: on the left the pipeline's jobs by stage, on the right the chosen
   * job's log, colors included. Reloaded every few seconds while the job runs,
   * staying pinned to the bottom if it was there. ⤢ or "f" switches to full screen (remembered under
   * `logs.full`, like the pod logs drawer). Escape closes.
   */
  /**
   * Parsed as it grows: the lines already read keep their objects and their keys, so a poll renders
   * only the new ones (the whole log again was 18–20 ms each time, measured on 4.8k lines).
   * Another job, another reading.
   */
  const reading = $derived.by(() => (void logs.jobId, new LogLines()))
  const lines = $derived(reading.parse(logs.log))
  const stages = $derived([...new Set(logs.jobs.map((j) => j.stage))])
  const ERROR = /\b(error|failed|fatal|exception)\b/i
  const firstError = $derived(lines.findIndex((l) => l.segs.some((s) => (s.fg === 'c31' || s.fg === 'c91') && ERROR.test(s.text))))

  const icon = (status: string): IconName =>
    status === 'success' ? 'check' : status === 'failed' ? 'fail' : status === 'canceled' ? 'canceled' : status === 'skipped' ? 'skipped' : status === 'manual' ? 'play' : LIVE.has(status) ? 'running' : 'pending'
  const tone = (j: JobInfo) => (j.status === 'success' ? 'ok' : j.status === 'failed' ? (j.allow_failure ? 'warn' : 'ko') : LIVE.has(j.status) ? 'run' : '')
  /** A failed job (allowed failures aside), its log read: an agent diagnoses it, read-only. */
  const diagnose = $derived(logs.job && logs.job.status === 'failed' && !logs.job.allow_failure && logs.log ? diagnoseJobAction(logs.job, logs.project, logs.log) : null)
  const duration = (j: JobInfo | null) => (j?.duration ? span(j.duration * 1000) : '')

  let view: HTMLElement | undefined = $state()
  /** Pinned to the bottom: a growing log stays followed. Reset to true on every job change. */
  let follow = true
  let shownJob: number | null = null

  $effect(() => {
    void lines
    const job = logs.jobId
    if (job !== shownJob) {
      shownJob = job
      follow = true
    }
    if (follow) tick().then(() => view?.scrollTo({ top: view.scrollHeight }))
  })

  function onScroll() {
    if (view) follow = view.scrollHeight - view.scrollTop - view.clientHeight < 40
  }

  function toLine(i: number) {
    follow = false
    view?.querySelector(`[data-line="${i}"]`)?.scrollIntoView({ block: 'center' })
  }

  let full = $state(pref<boolean>('logs.full', false))
  function toggleFull() {
    full = !full
    setPref('logs.full', full)
  }
  const typing = (e: KeyboardEvent) => !!(e.target as HTMLElement | null)?.closest?.('input, textarea, select')

  function onKey(e: KeyboardEvent) {
    if (!logs.open) return
    if (e.key === 'f' && !typing(e) && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      e.stopImmediatePropagation()
      return toggleFull()
    }
    if (e.key !== 'Escape') return
    e.stopImmediatePropagation()
    closeLogs()
  }

  let drawerEl: HTMLElement | undefined = $state()
  /** A click outside the drawer closes it (see drawers.ts). */
  function onOutside(e: PointerEvent) {
    if (logs.open && clickedOutside(e, drawerEl)) {
      closeLogs()
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if logs.open}
  <aside class="drawer" class:full bind:this={drawerEl} data-drawer="51" data-pinned={isPinned('logs')} aria-label={t('Pipeline logs')}>
    <header>
      <span class="what">{logs.label}</span>
      {#if logs.job}
        <span class="sep">›</span>
        <b class="mono">{logs.job.name}</b>
        <span class="st {tone(logs.job)}"><Icon name={icon(logs.job.status)} /> {logs.job.status}{#if LIVE.has(logs.job.status)}{' '}· {t('live')}{/if}</span>
        {#if duration(logs.job)}<span class="muted mono">{duration(logs.job)}</span>{/if}
      {/if}
      <span class="grow"></span>
      {#if firstError !== -1}<button class="txt" onclick={() => toLine(firstError)} title={t('Scroll to the first error line')}>{t('First error')}</button>{/if}
      <button class="txt" onclick={() => toLine(lines.length - 1)} title={t('Scroll to the end and follow')}>{t('End')} <Icon name="down" /></button>
      <button class="icon" onclick={() => logs.jobId && selectJob(logs.jobId)} disabled={logs.loading || !logs.jobId} title={t('Reload the log')} aria-label={t('Reload')}><Icon name="refresh" /></button>
      {#if diagnose && (diagnose.launch || diagnose.href)}<a class="txt" href={diagnose.href ?? undefined} onclick={claudeClick(diagnose)} title={t('An agent finds the cause of this failure and proposes a fix, changing nothing ({note}) — ⇧-click to launch at once', { note: diagnose.note ?? '' })}><Icon name={agentIcon()} /> {t('Diagnose')}</a>{/if}
      {#if logs.job && (RETRYABLE.has(logs.job.status) || RERUNNABLE.has(logs.job.status))}{#key logs.job.id}<RetryJob job={logs.job.id} name={logs.job.name} label rerun={RERUNNABLE.has(logs.job.status)} project={logs.project} />{/key}{/if}
      {#if logs.job}<a class="icon" href={logs.job.url} target="_blank" rel="noreferrer" title={t('Open the job in {forge}', { forge: forgeName() })} aria-label={t('Open in {forge}', { forge: forgeName() })}><Icon name="external" /></a>{/if}
      <button class="icon" onclick={toggleFull} title={full ? t('Exit full screen (f)') : t('Full screen (f)')} aria-label={t('Full screen')} aria-pressed={full}><Icon name={full ? 'shrink' : 'expand'} /></button>
      <PinButton kind="logs" />
      <button class="icon" onclick={closeLogs} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      <nav class="jobs" aria-label={t('Jobs')}>
        {#each stages as stage (stage)}
          <p class="stage">{stage}</p>
          {#each logs.jobs.filter((j) => j.stage === stage) as j (j.id)}
            <div class="jobline" class:on={j.id === logs.jobId}>
              <button class="job {tone(j)}" onclick={() => selectJob(j.id)} title="{j.name} · {j.status}{j.allow_failure ? ` ${t('(allowed to fail)')}` : ''}">
                <span class="i"><Icon name={icon(j.status)} /></span><span class="n">{j.name}</span><span class="d">{duration(j)}</span>
              </button>
              {#if RETRYABLE.has(j.status)}<RetryJob job={j.id} name={j.name} project={logs.project} />
              {:else if RERUNNABLE.has(j.status)}<span class="rerun"><RetryJob job={j.id} name={j.name} rerun project={logs.project} /></span>{/if}
            </div>
          {/each}
        {:else}
          <p class="muted pad">{logs.pipelineId === null && logs.loading ? t('Loading…') : t('No job list.')}</p>
        {/each}
      </nav>
      <div class="log" bind:this={view} onscroll={onScroll}>
        {#if logs.error}
          <p class="error">{logs.error}</p>
        {:else if !logs.log && logs.loading}
          <p class="muted pad">{t('Loading the log…')}</p>
        {:else if !logs.log}
          <p class="muted pad">{logs.job && ['created', 'pending', 'manual', 'skipped'].includes(logs.job.status) ? t('This job is {status}: no log yet.', { status: logs.job.status }) : t('Empty log.')}</p>
        {:else}
          {#if logs.truncated}<p class="muted pad">{t('Only the end of this log is shown — open the job in {forge} for the whole of it.', { forge: forgeName() })}</p>{/if}
          <pre>{#each lines as l, i (reading.key(i))}<div class="line" class:section={l.section} data-line={i}><span class="no">{i + 1}</span><span class="txt-line">{#each l.segs as s, k (k)}<span class="{s.fg ?? ''}" class:b={s.bold} style:color={s.color}>{s.text}</span>{/each}</span></div>{/each}</pre>
        {/if}
      </div>
    </div>
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 51; width: min(1100px, 94vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-gitlab) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  /* Full screen: the whole window, with no shadow or border. */
  .drawer.full { left: 0; width: auto; border-left: none; box-shadow: none; }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; min-width: 0; white-space: nowrap;
    border-bottom: 1px solid var(--border);
    background: var(--surface); box-shadow: inset 0 2px var(--src-gitlab);
  }
  .what { font-weight: 600; overflow: hidden; text-overflow: ellipsis; }
  .sep, .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .grow { flex: 1; }
  .st { font-size: var(--fs-meta); color: var(--text-muted); }
  .ok { color: var(--ok); } .ko { color: var(--ko); } .warn { color: var(--warn); } .run { color: var(--info); }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 15px; line-height: 1; cursor: pointer; }
  .icon:hover, .txt:hover { background: var(--surface-muted); color: var(--text); }
  .icon:disabled { opacity: 0.4; cursor: progress; }
  .txt { border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); padding: 1px 8px; cursor: pointer; }
  a.txt { display: inline-flex; align-items: center; gap: 4px; text-decoration: none; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 220px 1fr; }
  .jobs { overflow: auto; border-right: 1px solid var(--border); padding: 4px 0 12px; }
  .stage { margin: 8px 10px 2px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .job { display: flex; align-items: center; gap: 6px; width: 100%; padding: 3px 10px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .jobline { display: flex; align-items: center; padding-right: 6px; }
  .jobline .job { flex: 1; min-width: 0; }
  .jobline:hover { background: var(--surface-muted); }
  .jobline.on { background: color-mix(in srgb, var(--src-gitlab) 14%, transparent); }
  /* Retry a successful job: offered on hover only (two clicks). */
  .rerun { visibility: hidden; }
  .jobline:hover .rerun, .jobline.on .rerun, .rerun:focus-within { visibility: visible; }
  .job .i { display: inline-flex; justify-content: center; width: 12px; flex-shrink: 0; }
  .job:not(.ok, .ko, .warn, .run) .i { color: var(--text-muted); }
  .job .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .job .d { flex-shrink: 0; font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .log { overflow: auto; background: var(--bg); color: var(--text); }
  .pad { padding: 10px 14px; margin: 0; font-size: var(--fs-body); }
  .log .muted { color: var(--text-muted); }
  .error { margin: 0; padding: 10px 14px; color: var(--ko); font-size: var(--fs-body); white-space: pre-wrap; }
  pre { margin: 0; padding: 6px 0 16px; font: 11.5px/1.5 var(--mono); }
  /* Off screen, a line is neither laid out nor painted: a long log scrolls and grows lightly. */
  .line { display: flex; padding-right: 14px; content-visibility: auto; contain-intrinsic-size: auto 1.5em; }
  .line:hover { background: color-mix(in srgb, var(--text) 4%, transparent); }
  .line.section { margin-top: 4px; border-top: 1px solid var(--border); }
  .no { flex-shrink: 0; width: 52px; padding-right: 12px; text-align: right; color: color-mix(in srgb, var(--text-muted) 70%, transparent); user-select: none; }
  .txt-line { white-space: pre-wrap; overflow-wrap: anywhere; min-width: 0; }
  .b { font-weight: 700; }
</style>
