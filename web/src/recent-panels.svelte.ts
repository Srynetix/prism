import { t } from './i18n.svelte'
import { toast } from './toast.svelte'

/**
 * Panels closed lately (MR, ticket, CI and pod logs, shells, event, ArgoCD, thread), the last
 * first: ⌘⇧T opens the last one again, the palette lists them all. Each knows how to come back as
 * it was (the same pod, container and log, the same job…), never its content: read again. Kept
 * for the session only.
 */
export interface RecentPanel {
  /** One entry per thing: closed again, it moves to the top. */
  key: string
  kind: 'mr' | 'ticket' | 'logs' | 'pod-logs' | 'shell' | 'event' | 'argo' | 'thread' | 'changes'
  title: string
  hint: string
  /** When it was closed. */
  at: number
  reopen: () => void
}

const MAX = 10

export const recentPanels = $state({ list: [] as RecentPanel[] })

export function rememberPanel(p: Omit<RecentPanel, 'at'>) {
  recentPanels.list = [{ ...p, at: Date.now() }, ...recentPanels.list.filter((r) => r.key !== p.key)].slice(0, MAX)
}

export function reopenPanel(p: RecentPanel) {
  recentPanels.list = recentPanels.list.filter((r) => r.key !== p.key)
  p.reopen()
}

/** ⌘⇧T. */
export function reopenLastPanel() {
  const last = recentPanels.list[0]
  if (last) reopenPanel(last)
  else toast(t('No panel closed lately'))
}

