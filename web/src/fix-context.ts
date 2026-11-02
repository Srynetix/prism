import { invoke } from './bridge'
import { parse } from './ansi'
import { oneLine, untrusted } from './untrusted'
import type { JobInfo } from './logs.svelte'

/** Failed jobs whose log ends go into a fix's prompt, at most. */
const MAX_JOBS = 3

/**
 * What Prism hands an agent sent to fix a failed pipeline: each failed job (allowed failures
 * aside) and the last `lines` lines of its log, colors and GitLab's section markers removed. The
 * agent starts from the error instead of fetching the logs itself. Empty when nothing reads.
 */
export async function failedJobsContext(project: string | null, pipeline: number, lines: number, header: string): Promise<string> {
  if (lines <= 0) return ''
  const jobs = (await invoke<JobInfo[]>('pipeline_jobs', { project, pipeline })).filter((j) => j.status === 'failed' && !j.allow_failure).slice(0, MAX_JOBS)
  const parts = await Promise.all(
    jobs.map(async (j) => {
      const r = await invoke<{ log: string }>('job_log', { project, job: j.id }).catch(() => null)
      const text = r ? tail(r.log, lines) : ''
      // Whatever the CI ran (a dependency's tests too) writes these logs: quoted, never instructions.
      return text ? `### ${oneLine(j.name)} (${oneLine(j.stage)}) ${j.url}\n\n${untrusted(`Log of ${oneLine(j.name)}`, text)}` : ''
    }),
  )
  const found = parts.filter(Boolean)
  return found.length ? `\n\n${header}\n\n${found.join('\n\n')}` : ''
}

/** The last `n` non-empty lines of a job log, as plain text. */
export function tail(log: string, n: number): string {
  const lines = parse(log)
    .map((l) => l.segs.map((s) => s.text).join('').trimEnd())
    .filter((l) => l.trim())
  return lines.slice(-n).join('\n')
}
