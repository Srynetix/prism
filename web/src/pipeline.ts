import { t } from './i18n.svelte'
import type { Pipeline, Stage } from './types'
import { elapsed, span } from './time'

export type StageTone = 'ok' | 'ko' | 'run' | 'man' | 'cx' | ''

export function stageTone(s: Stage): StageTone {
  if (s.failed > 0 || s.status === 'failed') return 'ko'
  if (['running', 'pending', 'preparing', 'waiting_for_resource'].includes(s.status)) return 'run'
  if (s.status === 'manual') return 'man'
  if (s.status === 'canceled') return 'cx'
  if (s.status === 'success') return 'ok'
  return ''
}

/** Total time: from the first started stage to the last finished one (or up to now). */
export function pipelineSpan(p: Pipeline, now: number): number | null {
  const starts = p.stages.map((s) => s.started_at).filter((v): v is string => !!v).sort()
  if (!starts.length) return null
  const running = p.stages.some((s) => stageTone(s) === 'run')
  const ends = p.stages.map((s) => s.finished_at).filter((v): v is string => !!v).sort()
  return elapsed(starts[0], running ? null : ends.at(-1) ?? null, now)
}

/** What to know at a glance: the failure and its job, the progress, the cancel, or the success. */
export function pipelineLabel(p: Pipeline, now: number): { text: string; tone: StageTone } {
  const total = pipelineSpan(p, now)
  const took = total !== null ? ` · ${span(total)}` : ''
  const failed = p.stages.find((s) => stageTone(s) === 'ko')
  if (failed) {
    const jobs = failed.failed_jobs
    const what = jobs.length === 1 ? `${jobs[0].name} ✗` : t('{n} failed', { n: jobs.length || failed.failed })
    return { text: `${failed.name} · ${what}`, tone: 'ko' }
  }
  const running = p.stages.find((s) => stageTone(s) === 'run')
  if (running) return { text: `${running.name} ${running.finished}/${running.jobs}${took}`, tone: 'run' }
  // Canceled: the stage where it stopped (the first canceled one), never "passed".
  const canceled = p.stages.find((s) => stageTone(s) === 'cx')
  if (p.status === 'canceled' || canceled) return { text: `${t('canceled')}${canceled ? ` · ${canceled.name}` : ''}${took}`, tone: 'cx' }
  const manual = p.stages.find((s) => stageTone(s) === 'man')
  if (p.status === 'success' || p.stages.every((s) => ['ok', 'man', ''].includes(stageTone(s)))) {
    return { text: manual && p.status !== 'success' ? `${manual.name} ▷ ${t('manual')}` : `${t('passed')}${took}`, tone: manual && p.status !== 'success' ? '' : 'ok' }
  }
  return { text: p.status, tone: '' }
}

/** A protected branch's pipeline, named with its repository unless it is the primary one's: `helm · main`. */
export const branchName = (p: { repo?: string | null; target: string }) => (p.repo ? `${p.repo} · ${p.target}` : p.target)
