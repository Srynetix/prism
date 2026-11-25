import { agentsEnabled, openAgents } from './agents.svelte'
import { showBlock } from './layout.svelte'
import { canShowLogs, openPipeline } from './logs.svelte'
import { openMr } from './mr-view.svelte'
import { mrKey } from './mr-ref'
import { pipelineProject } from './pipeline'
import type { MergeRequest, Pipeline } from './types'

/**
 * One door per object: what the status bar, the header, the rail and the palette open for the same
 * thing, opened the same way from each (the new agent, Settings and the quick notes have theirs in
 * agents.svelte.ts, settings.svelte.ts and quick-notes.svelte.ts).
 */

/**
 * What waits for me among the agents (« À vous »): the Agents view by status, on `id` or the first
 * one waiting; without agents (the browser preview, `[agents] enabled` off), the Sessions block.
 */
export function openWaiting(id?: string) {
  if (agentsEnabled()) openAgents(id, 'waiting')
  else showBlock('sessions')
}

/**
 * A pipeline, from the status bar or the palette: failed, the logs of its failing job; otherwise
 * its MR's panel (`mr`: the MR it runs for), which shows it; otherwise its jobs in the logs pane;
 * where nothing reads logs (the browser preview), the CI block.
 */
export function showPipeline(p: Pipeline, mr?: MergeRequest | null) {
  const label = mr ? mrKey(mr) : p.target
  const project = pipelineProject(p) ?? mr?.project ?? null
  if (p.status === 'failed' && canShowLogs()) void openPipeline(p.id, label, project)
  else if (mr) openMr(mr)
  else if (canShowLogs()) void openPipeline(p.id, label, project)
  else showBlock('ci')
}
