import { invoke } from './bridge'

/**
 * The files that say what a task runs (package.json, a Makefile, scripts/…) the agent's branch
 * changed, then those of its folder's dependencies (`node_modules`…) written since its launch, or
 * none: a task run then is the agent's to choose — or the MR's it checked out. Prism decides
 * (`prism_agents::defines_task`), for its chains as for here.
 */
export const taskFilesChanged = (agent: string) => invoke<string[]>('agent_task_files_changed', { id: agent })
