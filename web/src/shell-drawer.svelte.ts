import { invoke } from './bridge'
import type { AgentShellInfo, ShellOwner } from './agents.svelte'
import { rememberPanel } from './recent-panels.svelte'
import { t } from './i18n.svelte'
import { live } from './live.svelte'
import { baseName, tildePath } from './platform'
import { toast } from './toast.svelte'

/**
 * Shells with no agent, in a drawer: a worktree's (Worktrees view) or a pod's (its logs pane, by
 * `kubectl exec`). tmux keeps them like an agent's shells: closing the drawer only detaches the
 * terminal. `where`: what the header shows beside its title (a folder, a pod's place).
 */
export const shellDrawer = $state({ owner: null as ShellOwner | null, where: '', kind: 'folder' as 'folder' | 'pod' })

export function openWorktreeShell(path: string, name: string) {
  Object.assign(shellDrawer, { owner: worktreeShellOwner(path, name), where: path, kind: 'folder' })
}

/** A folder's shells under one name wherever they are opened from: its worktree's branch, else its name (`~`: the home folder). */
export function folderShellTitle(path: string) {
  const w = live.snapshot?.worktrees.find((w) => w.path === path)
  return w?.branch ?? (tildePath(path) === '~' ? '~' : baseName(path) || path)
}

let home: string | null = null

/** A shell in the home folder, from anywhere (⌃`, the palette, File › Terminal): the same shells each time. */
export async function openHomeShell() {
  try {
    home ??= await invoke<string>('home_folder')
    openFolderShell(home)
  } catch (e) {
    toast(String(e), 6000)
  }
}

/** A folder's shells (a worktree, a clone, an agent's folder), titled by `folderShellTitle`. */
export const openFolderShell = (path: string) => openWorktreeShell(path, folderShellTitle(path))

/** A pod, as `pod_shell` takes it; no container: kubectl's default one. */
export interface PodRef {
  context: string
  namespace: string
  pod: string
  container: string | null
}

export function openPodShell(pod: PodRef, label: string) {
  Object.assign(shellDrawer, { owner: podShellOwner(pod, label), where: `${pod.context} / ${pod.namespace}`, kind: 'pod' })
}

export function closeShellDrawer() {
  const { owner, where, kind } = shellDrawer
  if (owner) rememberPanel({ key: `shell:${owner.id}`, kind: 'shell', title: t('Shell · {name}', { name: owner.title }), hint: where, reopen: () => Object.assign(shellDrawer, { owner, where, kind }) })
  Object.assign(shellDrawer, { owner: null, where: '' })
}

export const worktreeShellOwner = (path: string, name: string): ShellOwner => ({
  id: `wt:${path}`,
  title: name,
  folder: path,
  list: () => invoke<AgentShellInfo[]>('worktree_shells', { path }),
  open: (n) => invoke<string>('worktree_shell', { path, n }),
  close: (n) => invoke('worktree_close_shell', { path, n }),
})

const podShellOwner = (pod: PodRef, label: string): ShellOwner => ({
  id: `kp:${pod.context}/${pod.namespace}/${pod.pod}/${pod.container ?? ''}`,
  title: label,
  folder: pod.container ? `${pod.pod} (${pod.container})` : pod.pod,
  list: () => invoke<AgentShellInfo[]>('pod_shells', { pod }),
  open: (n) => invoke<string>('pod_shell', { pod, n }),
  close: (n) => invoke('pod_close_shell', { pod, n }),
})
