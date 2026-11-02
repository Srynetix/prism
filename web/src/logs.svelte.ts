import { hasCore, invoke } from './bridge'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'
import type { JobLog } from './bindings/JobLog'
import type { JobInfo } from './bindings/JobInfo'
export type { JobInfo }

/**
 * Logs pane: a pipeline's jobs and the chosen job's log, read on demand and re-read
 * while the job runs. Nothing is kept once the pane is closed.
 */

export const logs = $state({
  open: false,
  /** What was opened: `main`, `!1957`, `api · staging`… */
  label: '',
  /** GitLab project of the pipeline: absent, the primary repository's; otherwise one a block allows (deployments). */
  project: null as string | null,
  pipelineId: null as number | null,
  jobs: [] as JobInfo[],
  jobId: null as number | null,
  job: null as JobInfo | null,
  log: '',
  truncated: false,
  loading: false,
  error: null as string | null,
})

/** Statuses of a job that may still write to its log. */
export const LIVE = new Set(['created', 'waiting_for_resource', 'preparing', 'pending', 'running'])
const POLL_MS = 4000

/** The browser preview has no access to GitLab. */
export const canShowLogs = hasCore

let timer: ReturnType<typeof setTimeout> | undefined
/** Incremented on each opening: a response from a previous opening is ignored. */
let generation = 0

/** Pipeline: its jobs, and the most telling job (failed, otherwise running, otherwise the last). */
export async function openPipeline(pipelineId: number, label: string, project: string | null = null) {
  reset(label, project)
  logs.pipelineId = pipelineId
  const gen = generation
  logs.loading = true
  try {
    const jobs = await invoke<JobInfo[]>('pipeline_jobs', { project: logs.project, pipeline: pipelineId })
    if (gen !== generation) return
    logs.jobs = order(jobs)
    const pick = logs.jobs.find((j) => j.status === 'failed' && !j.allow_failure) ?? logs.jobs.find((j) => LIVE.has(j.status) && j.started_at) ?? logs.jobs.at(-1)
    if (pick) await selectJob(pick.id)
    else logs.loading = false
  } catch (e) {
    if (gen === generation) {
      logs.error = String(e)
      logs.loading = false
    }
  }
}

/** Job: its log right away, then the other jobs of its pipeline. */
export async function openJob(jobId: number, label: string, project: string | null = null) {
  reset(label, project)
  const gen = generation
  await selectJob(jobId)
  const pipeline = logs.job?.pipeline_id
  if (pipeline && gen === generation) {
    logs.pipelineId = pipeline
    invoke<JobInfo[]>('pipeline_jobs', { project: logs.project, pipeline })
      .then((jobs) => gen === generation && (logs.jobs = order(jobs)))
      .catch(() => {})
  }
}

export async function selectJob(jobId: number) {
  clearTimeout(timer)
  const gen = generation
  if (logs.jobId !== jobId) {
    logs.log = ''
    logs.job = logs.jobs.find((j) => j.id === jobId) ?? null
  }
  logs.jobId = jobId
  logs.loading = true
  logs.error = null
  try {
    const r = await invoke<JobLog>('job_log', { project: logs.project, job: jobId })
    if (gen !== generation || logs.jobId !== jobId) return
    logs.job = r.job
    logs.log = r.log
    logs.truncated = r.truncated
    logs.jobs = logs.jobs.map((j) => (j.id === r.job.id ? r.job : j))
    if (LIVE.has(r.job.status)) timer = setTimeout(() => logs.open && logs.jobId === jobId && selectJob(jobId), POLL_MS)
  } catch (e) {
    if (gen === generation && logs.jobId === jobId) logs.error = String(e)
  } finally {
    if (gen === generation && logs.jobId === jobId) logs.loading = false
  }
}

/** Statuses of a job that can be retried. */
export const RETRYABLE = new Set(['failed', 'canceled'])
/** A successful job can be retried too (redeploy staging…), after a second click. */
export const RERUNNABLE = new Set(['success'])

/**
 * Retries a job; GitLab creates a new one. If the pane shows this pipeline, the list is
 * re-read and the new job is followed. Rejects with GitLab's error.
 */
export async function retryJob(jobId: number, project: string | null = null): Promise<JobInfo> {
  const next = await invoke<JobInfo>('retry_job', { project, job: jobId })
  if (logs.open && logs.pipelineId === next.pipeline_id && logs.project === project) {
    const gen = generation
    const jobs = await invoke<JobInfo[]>('pipeline_jobs', { project, pipeline: next.pipeline_id }).catch(() => null)
    if (jobs && gen === generation) logs.jobs = order(jobs)
    if (logs.jobId === jobId) await selectJob(next.id)
  }
  return next
}

export function closeLogs() {
  const { open, label, project, pipelineId, jobId, job } = logs
  if (open && (jobId || pipelineId)) {
    rememberPanel({
      key: `logs:${project ?? ''}:${jobId ?? `p${pipelineId}`}`,
      kind: 'logs',
      title: job ? t('Job {name} · {label}', { name: job.name, label }) : t('Pipeline · {label}', { label }),
      hint: job ? t('Pipeline #{id} · {status}', { id: job.pipeline_id, status: job.status }) : t('Pipeline #{id}', { id: pipelineId ?? '' }),
      reopen: () => void (jobId ? openJob(jobId, label, project) : openPipeline(pipelineId!, label, project)),
    })
  }
  clearTimeout(timer)
  generation++
  Object.assign(logs, { open: false, jobs: [], job: null, jobId: null, pipelineId: null, log: '', error: null, loading: false })
}

function reset(label: string, project: string | null = null) {
  clearTimeout(timer)
  generation++
  Object.assign(logs, { open: true, label, project, jobs: [], job: null, jobId: null, pipelineId: null, log: '', truncated: false, error: null, loading: false })
}

/** By stage, in order of appearance of the stages; jobs in their creation order. */
function order(jobs: JobInfo[]): JobInfo[] {
  const sorted = [...jobs].sort((a, b) => a.id - b.id)
  const stages = [...new Set(sorted.map((j) => j.stage))]
  return stages.flatMap((s) => sorted.filter((j) => j.stage === s))
}

/**
 * `…/-/jobs/123` or `…/-/pipelines/456` (GitLab), `…/actions/runs/7/job/123` or `…/commit/<sha>`
 * (GitHub: a pipeline is a commit, its id the SHA's first 12 hex digits) → what the pane can open.
 */
export function logTarget(url: string): { job: number } | { pipeline: number } | null {
  const job = url.match(/\/-\/jobs\/(\d+)/) ?? url.match(/\/actions\/runs\/\d+\/job\/(\d+)/)
  if (job) return { job: Number(job[1]) }
  const pipeline = url.match(/\/-\/pipelines\/(\d+)/)
  if (pipeline) return { pipeline: Number(pipeline[1]) }
  const commit = url.match(/\/commit\/([0-9a-f]{12})[0-9a-f]*(?:[/?#]|$)/)
  return commit ? { pipeline: parseInt(commit[1], 16) } : null
}

/**
 * Click on a job or pipeline link: the logs pane; ⌘-click keeps the forge. Outside the
 * app, or for a link the pane cannot open, the link opens normally.
 */
export function logsClick(url: string, label: string, project: string | null = null) {
  return (e: MouseEvent) => {
    const target = logTarget(url)
    if (!canShowLogs() || !target || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return
    e.preventDefault()
    e.stopPropagation()
    if ('job' in target) openJob(target.job, label, project)
    else openPipeline(target.pipeline, label, project)
  }
}
