import { hasCore, invoke } from './bridge'
import { onGithub } from './forge'
import { mrArgs, type MrRef } from './mr-ref'
import type { MergeRequest } from './types'

/** Statuses of a pipeline that can still be stopped (same list on the Rust side). */
const CANCELABLE = new Set(['created', 'waiting_for_resource', 'preparing', 'pending', 'running', 'scheduled'])

/** The browser preview has no access to GitLab: no action possible. */
export const canCancel = (status: string | null | undefined) => hasCore() && !!status && CANCELABLE.has(status)

export const cancelPipeline = (id: number) => invoke<void>('cancel_pipeline', { id })

/** Pipeline whose jobs can be retried (same list on the Rust side). */
export const canRetry = (status: string | null | undefined) => hasCore() && (status === 'failed' || status === 'canceled')

export const retryPipeline = (id: number) => invoke<void>('retry_pipeline', { id })

/** A new pipeline for an open MR, on GitLab only (its rules read the MR's labels now). */
export const canRunPipeline = (mr: MergeRequest) => hasCore() && mr.state === 'opened' && !onGithub(mr)

export const runMrPipeline = (mr: MrRef) => invoke<void>('run_mr_pipeline', mrArgs(mr))

export const playJob = (id: number) => invoke<void>('play_job', { id })

/** Runs again a manual job already run (GitLab's retry, GitHub's re-run): a tag set again. */
export const replayJob = (id: number) => invoke<void>('replay_job', { id })

export const rebaseMr = (mr: MrRef) => invoke<void>('rebase_mr', mrArgs(mr))

/** Merges an MR now: GitLab checks again that it can (pipeline, approvals, threads). */
export const mergeMr = (mr: MrRef) => invoke<void>('merge_mr', mrArgs(mr))

/** In a repository with a merge queue (`[[repos]] merge_queue`): a job of its CI. */
export const setMergeQueue = (mr: MrRef, on: boolean) => invoke<void>('set_merge_queue', { ...mrArgs(mr), on })

/** The merge queue's order (`[gitlab] merge_priority_prefix`): `priority` its number, `null` none. */
export const setMergePriority = (mr: MrRef, priority: number | null) => invoke<void>('set_merge_priority', { ...mrArgs(mr), priority })

export const setAutoMerge = (mr: MrRef, enable: boolean) => invoke<void>('set_auto_merge', { ...mrArgs(mr), enable })

/** Set an MR to draft, or mark it ready (`Draft:` title prefix, set by GitLab). */
export const setDraft = (mr: MrRef, draft: boolean) => invoke<void>('set_draft', { ...mrArgs(mr), draft })

export const playRenovate = () => invoke<void>('play_renovate')

export const checkRenovate = (marker: string) => invoke<void>('check_renovate', { marker })
