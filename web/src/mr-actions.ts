import { agentsTarget, mrActions, runClaude, type ClaudeActionId } from './claude-actions'
import { actorName } from './agent-cli'
import { ticketRegex } from './conventions'
import { openPipeline } from './logs.svelte'
import { mergeMr, rebaseMr, setAutoMerge, setMergeQueue } from './pipeline-actions'
import { rebaseFailed, rebaseFailure } from './rules'
import type { PrimaryAction } from './rules'
import type { MergeRequest } from './types'
import { mrKey } from './mr-ref'
import { forgeName, onGithub } from './forge'
import { emojiText } from './emoji.svelte'
import { t, tc } from './i18n.svelte'

/**
 * Main action of an MR line (the one that unblocks it): a Claude link, or a GitLab
 * action executed in place.
 */
export type Primary = { label: string; title: string; confirm?: string } & ({ href: string } | { run: () => Promise<unknown> | void })

const CLAUDE: Record<string, { label: string; action: ClaudeActionId; title: () => string }> = {
  'claude-fix': { label: t('Fix'), action: 'fix-pipeline', title: () => t('{actor} analyses and fixes the pipeline errors', { actor: actorName() }) },
  'claude-review': { label: tc('action', 'Review'), action: 'review', title: () => t('{actor} reviews this MR', { actor: actorName() }) },
  'claude-rebase': { label: t('Resolve'), action: 'rebase', title: () => t('{actor} rebases the branch and resolves the conflicts', { actor: actorName() }) },
}

/** The forge's rebase failed: the agent rebases locally, where the commit that conflicts shows. */
const LOCAL_REBASE = (m: MergeRequest) => t('{actor} rebases the branch locally: {forge} could not ({reason})', { actor: actorName(), forge: forgeName(m.forge), reason: rebaseFailure(m) })

export function primaryAction(kind: PrimaryAction | undefined, m: MergeRequest, queueLabel: string | null | undefined): Primary | null {
  if (!kind) return null
  const claude = CLAUDE[kind]
  if (claude) {
    const a = mrActions(m).find((x) => x.id === claude.action)
    const local = kind === 'claude-rebase' && rebaseFailed(m)
    const [label, title] = local ? [t('Rebase locally'), LOCAL_REBASE(m)] : [claude.label, claude.title()]
    if (a?.launch && agentsTarget()) return { label, title: t('{title} — opens the agent launcher', { title }), run: () => runClaude(a) }
    return a?.href ? { label, title: t('{title} — opens a Claude Code session', { title }), href: a.href } : null
  }
  switch (kind) {
    case 'rebase':
      return { label: t('Rebase'), title: t('{forge} rebases {mr} onto {branch}', { forge: forgeName(m.forge), mr: mrKey(m), branch: m.target_branch }), run: () => rebaseMr(m) }
    case 'merge':
      return { label: t('Merge'), title: t('Merge {mr} into {branch} now', { mr: mrKey(m), branch: m.target_branch }), confirm: t('Merge?'), run: () => mergeMr(m) }
    case 'automerge':
      return { label: t('Auto-merge'), title: t('{forge} merges {mr} into {branch} as soon as the pipeline is green', { forge: forgeName(m.forge), mr: mrKey(m), branch: m.target_branch }), run: () => setAutoMerge(m, true) }
    case 'queue':
      return queueLabel ? { label: t('Merge queue'), title: t('Add the “{label}” label: the CI job rebases, then auto-merges', { label: queueLabel }), run: () => setMergeQueue(m, true) } : null
    case 'logs':
      return m.head_pipeline ? { label: t('Logs'), title: t('Open the pipeline logs'), run: () => openPipeline(m.head_pipeline!.id, mrKey(m), m.project ?? null) } : null
  }
  return null
}

/** Time entry description: the MR, without the ` | #ABC-…` suffix that duplicates the ticket. */
export const timeDescription = (m: MergeRequest) =>
  `${mrKey(m)} ${m.title.replace(/^Draft:\s*/i, '').replace(ticketRegex('mr-suffix', (t) => String.raw`\s*\|\s*#?${t}\s*$`), '')}`

/** An MR's title as text (an attribute, a sheet's title): without its `Draft:`, its shortcodes (`:bug:`) as emoji; see MrTitle.svelte. */
export const cleanTitle = (m: MergeRequest) => emojiText(m.title.replace(/^Draft:\s*/i, ''), onGithub(m))
