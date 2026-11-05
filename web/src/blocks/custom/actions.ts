import { call } from '../block'
import { openLauncher } from '../../agents.svelte'
import { toast } from '../../toast.svelte'
import { t } from '../../i18n.svelte'
import { SOURCE, type CustomItem } from './types'

/**
 * An action on a line: its command or script runs on the computer (the block read again after),
 * or its agent opens in the launcher with the line as data — never launched without me, and in the
 * usual permission mode.
 */
export async function runAction(block: string, action: string, label: string, item: CustomItem) {
  try {
    const out = await call<{ agent?: { title: string; prompt: string; cwd?: string | null }; stdout?: string }>(SOURCE, 'action', { id: block, action, key: item.key })
    if (out.agent) return openLauncher({ title: out.agent.title, prompt: out.agent.prompt, ...(out.agent.cwd ? { cwd: out.agent.cwd } : { worktree: {} }) }, null, null, block)
    toast(out.stdout?.trim() ? `${label}: ${out.stdout.trim().split('\n').pop()}` : t('{action}: done', { action: label }), 4000)
  } catch (e) {
    toast(String(e), 8000)
  }
}

/** ↻: read the block now. */
export const readNow = (block: string) => call(SOURCE, 'run', { id: block })
