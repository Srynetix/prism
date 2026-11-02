import { invoke, isDesktop } from './bridge'
import { live } from './live.svelte'
import { shellTabs } from './agents.svelte'
import { openPodShell, openWorktreeShell, shellDrawer } from './shell-drawer.svelte'
import { tildePath } from './platform'
import type { OpenShell } from './bindings/OpenShell'

/**
 * The shells with no agent still open, a worktree's or a pod's: tmux keeps them once their drawer
 * is closed, and the status bar and the palette list them to go back. One line per worktree or pod
 * (its shells are the drawer's tabs). Read at start, when the shells drawer opens or closes, when
 * the list opens and when the window comes back: no polling.
 */
export const openShells = $state({ list: [] as OpenShell[] })

export async function refreshShells() {
  if (!isDesktop()) return
  try {
    openShells.list = await invoke<OpenShell[]>('open_shells')
  } catch {
    // tmux missing or gone: what was listed stays until the next reading.
  }
}

/** A worktree's or a pod's shells, oldest first. */
export interface ShellGroup {
  /** `shell:<owner>:`, the shells' keys before their number. */
  id: string
  shells: OpenShell[]
  title: string
  where: string
  pod: boolean
  /** Its pod's `kubectl exec` has exited, in every shell: the pod is likely gone. */
  ended: boolean
  /** What runs in a worktree's shells (not at their prompt). */
  running: string[]
  started: number
}

const owner = (s: OpenShell) => s.key.slice(0, s.key.lastIndexOf(':') + 1)

function title(s: OpenShell) {
  if (s.pod) return s.pod.container ? `${s.pod.pod} (${s.pod.container})` : s.pod.pod
  const w = live.snapshot?.worktrees.find((w) => w.path === s.folder)
  return w?.branch ?? s.folder!.split('/').pop() ?? s.folder!
}

export function shellGroups(): ShellGroup[] {
  const groups = new Map<string, ShellGroup>()
  for (const s of openShells.list) {
    const id = owner(s)
    let g = groups.get(id)
    if (!g) {
      g = { id, shells: [], title: title(s), where: s.pod ? `${s.pod.context} / ${s.pod.namespace}` : tildePath(s.folder!), pod: !!s.pod, ended: true, running: [], started: s.started }
      groups.set(id, g)
    }
    g.shells.push(s)
    g.ended &&= s.ended
    if (s.busy) g.running.push(s.command)
  }
  return [...groups.values()]
}

/** Its drawer, on its first shell. */
export function goToShells(g: ShellGroup) {
  const s = g.shells[0]
  if (s.pod) openPodShell(s.pod, title(s))
  else openWorktreeShell(s.folder!, title(s))
  if (shellDrawer.owner) shellTabs[shellDrawer.owner.id] = s.n
}

/** Closes every shell of the group: kubectl exec ends, what a worktree's shell ran is killed. */
export async function closeShells(g: ShellGroup) {
  await Promise.allSettled(g.shells.map((s) => invoke('close_open_shell', { key: s.key })))
  await refreshShells()
}
