import { BLOCK_DEFS } from './blocks'
import { t } from './i18n.svelte'

/**
 * The Settings tree: settings sorted by what they configure, not by config.toml table. Seven
 * domains (General, Interface, Work, Code, Agents, Infra, Remote), each a list of pages sorted in
 * groups (`groups`, a page's `group`); a page is made of one or more sections (Settings.svelte's
 * own, or settings-schema.ts's), each keeping its TOML table.
 *
 * `toggle`: the section whose `enabled` key is the feature's switch, shown in the page header
 * and as the page's state in the tree (and left out of the section's rows).
 * `choice`: the key choosing which of the page's providers applies (Tracker › Provider), chosen at
 * the top of the page. Its `children` are those providers (`provider`: the value choosing it): not
 * pages of the tree, but sections of their page, the chosen one shown, the others folded below
 * (or, for an agent CLI, one a repository runs), all editable.
 * `tabs`: the sections after the first are tabs of the page (a delivery model's stages), not stacked.
 */
export interface TreeNode {
  id: string
  label: string
  color?: string
  sections?: string[]
  toggle?: string
  choice?: { path: string[]; fallback: string; noun: string }
  provider?: string
  tabs?: boolean
  /** A domain's page: the id of its group in the domain's `groups`. */
  group?: string
  /** A domain: its groups of pages, in order (a group without label: the pages alone). */
  groups?: { id: string; label: string }[]
  children?: TreeNode[]
}

export const TREE: TreeNode[] = [
  {
    id: 'general', label: t('General'), color: 'var(--accent)',
    groups: [{ id: 'config', label: t('Configuration') }, { id: 'prism', label: 'Prism' }, { id: 'diagnostic', label: t('Diagnostics') }],
    children: [
      { id: 'profiles', label: t('Profiles'), sections: ['profiles'], group: 'config' },
      { id: 'packs', label: t('Packs'), sections: ['packs'], group: 'config' },
      { id: 'file', label: t('Config file'), sections: ['file'], group: 'config' },
      { id: 'cleanup', label: t('Cleanup'), sections: ['cleanup', 'cleanup-usage'], group: 'config' },
      { id: 'updates', label: t('Updates'), sections: ['updates-now', 'updates'], toggle: 'updates', group: 'prism' },
      { id: 'feedback', label: t('Feedback'), sections: ['feedback-requests', 'feedback', 'feedback-labels'], toggle: 'feedback', group: 'prism' },
      { id: 'cli', label: t('Command line'), sections: ['cli-command'], group: 'prism' },
      { id: 'logs', label: t('Logs'), sections: ['logs'], group: 'diagnostic' },
      { id: 'dependencies', label: t('Dependencies'), sections: ['dependencies'], group: 'diagnostic' },
    ],
  },
  {
    id: 'interface', label: t('Interface'), color: 'var(--violet)',
    groups: [{ id: 'look', label: t('Appearance') }, { id: 'alerts', label: t('Alerts') }, { id: 'tools', label: t('Shortcuts and tools') }],
    children: [
      { id: 'appearance', label: t('Appearance'), sections: ['appearance'], group: 'look' },
      { id: 'views', label: t('Pages'), sections: ['views'], group: 'look' },
      { id: 'custom-blocks', label: t('Custom blocks'), sections: ['custom-blocks'], group: 'look' },
      { id: 'alerts', label: t('Thresholds'), sections: ['alerts'], group: 'alerts' },
      { id: 'notifications', label: t('Notifications'), sections: ['digest'], group: 'alerts' },
      { id: 'audio', label: t('Audio'), sections: ['audio'], group: 'alerts' },
      { id: 'palette', label: t('Palette'), sections: ['palette'], toggle: 'palette', group: 'tools' },
      { id: 'openwith', label: t('Open with'), sections: ['openwith'], group: 'tools' },
    ],
  },
  {
    id: 'work', label: t('Work'), color: 'var(--src-clickup)',
    children: [
      {
        id: 'tracker', label: t('Tracker'), sections: ['tracker'], choice: { path: ['tracker', 'provider'], fallback: 'clickup', noun: t('the tracker') }, children: [
          { id: 'clickup', label: t('ClickUp'), sections: ['clickup'], toggle: 'clickup', provider: 'clickup' },
          { id: 'jira', label: t('Jira'), sections: ['jira'], provider: 'jira' },
          { id: 'obsidian', label: t('Obsidian'), sections: ['obsidian'], provider: 'obsidian' },
        ],
      },
      {
        id: 'calendar', label: t('Calendar'), sections: ['calendar'], toggle: 'calendar', choice: { path: ['calendar', 'provider'], fallback: 'macos', noun: t('the calendar') }, children: [
          { id: 'calendar-macos', label: t('macOS Calendar'), sections: ['calendar-macos'], provider: 'macos' },
          { id: 'calendar-google', label: t('Google Calendar'), sections: ['google'], provider: 'google' },
        ],
      },
      {
        id: 'mail', label: t('Mail'), sections: ['mail'], toggle: 'mail', choice: { path: ['mail', 'provider'], fallback: 'microsoft', noun: t('the mail provider') }, children: [
          { id: 'mail-microsoft', label: t('Microsoft 365'), sections: ['mail-microsoft'], provider: 'microsoft' },
          { id: 'mail-gmail', label: t('Gmail'), sections: ['google'], provider: 'gmail' },
        ],
      },
      { id: 'notes', label: t('Notes'), sections: ['notes'] },
    ],
  },
  {
    id: 'code', label: t('Code'), color: 'var(--src-gitlab)',
    groups: [{ id: 'forge', label: t('Forge and repositories') }, { id: 'quality', label: t('Quality') }],
    children: [
      {
        id: 'forge', label: t('Forge'), sections: ['forge'], toggle: 'forge', group: 'forge', choice: { path: ['forge', 'provider'], fallback: 'gitlab', noun: t('the forge') }, children: [
          { id: 'forge-gitlab', label: t('GitLab'), sections: ['gitlab'], provider: 'gitlab' },
          { id: 'forge-github', label: t('GitHub'), sections: ['github'], provider: 'github' },
        ],
      },
      { id: 'repos', label: t('Repositories'), sections: ['repos', 'repo-blocks'], group: 'forge' },
      { id: 'worktrees', label: t('Worktrees'), sections: ['worktrees'], group: 'forge' },
      { id: 'renovate', label: t('Renovate'), sections: ['renovate'], toggle: 'renovate', group: 'quality' },
      { id: 'vulns', label: t('Vulnerabilities'), sections: ['vulns'], toggle: 'vulns', group: 'quality' },
    ],
  },
  {
    id: 'agents-root', label: t('Agents'), color: 'var(--src-claude)',
    groups: [{ id: 'cli', label: t('CLI') }, { id: 'session', label: t('In session') }, { id: 'prism', label: t('Prism and the agents') }, { id: 'tools', label: t('Tools') }],
    children: [
      {
        id: 'agents', label: t('Agent'), sections: ['agents'], toggle: 'agents', group: 'cli', choice: { path: ['agents', 'provider'], fallback: 'claude', noun: t('the default CLI') }, children: [
          { id: 'cli-claude', label: t('Claude Code'), sections: ['agents-claude'], provider: 'claude' },
          { id: 'cli-command-code', label: t('Command Code'), sections: ['agents-command-code'], provider: 'command-code' },
          { id: 'cli-opencode', label: t('OpenCode'), sections: ['agents-opencode'], provider: 'opencode' },
          { id: 'cli-codex', label: t('Codex'), sections: ['agents-codex'], provider: 'codex' },
        ],
      },
      { id: 'quick-answers', label: t('Quick answers'), sections: ['agents-quick'], toggle: 'agents-quick', group: 'session' },
      { id: 'quick-actions', label: t('Quick actions'), sections: ['agents-actions'], group: 'session' },
      { id: 'aside', label: t('In passing'), sections: ['agents-aside'], toggle: 'agents-aside', group: 'session' },
      { id: 'overlaps', label: t('Overlaps'), sections: ['agents-overlaps'], toggle: 'agents-overlaps', group: 'session' },
      { id: 'assistant', label: t('Assistant'), sections: ['agents-assistant'], toggle: 'agents-assistant', group: 'prism' },
      { id: 'routines', label: t('Routines'), sections: ['routines'], group: 'prism' },
      { id: 'prism-mcp', label: t('Prism MCP'), sections: ['mcp', 'agents-mcp', 'mcp-agents'], toggle: 'mcp', group: 'prism' },
      { id: 'dictation', label: t('Dictation'), sections: ['agents-dictation', 'dictation-panel'], toggle: 'agents-dictation', group: 'prism' },
      { id: 'files', label: t('Files'), sections: ['files'], group: 'tools' },
      { id: 'console', label: t('Console'), sections: ['console'], group: 'tools' },
    ],
  },
  {
    id: 'infra', label: t('Infra'), color: 'var(--src-k8s)',
    groups: [{ id: 'kubernetes', label: t('Kubernetes') }, { id: 'machine', label: t('Machine') }, { id: 'watch', label: t('Monitoring') }],
    children: [
      { id: 'kubernetes', label: t('Clusters'), sections: ['clusters', 'envs'], toggle: 'clusters', group: 'kubernetes' },
      { id: 'pods', label: t('Pods'), sections: ['pods'], group: 'kubernetes' },
      { id: 'argocd', label: t('ArgoCD'), sections: ['argocd', 'argocd-layout'], group: 'kubernetes' },
    ],
  },
  {
    id: 'remote-root', label: t('Remote'), color: 'var(--info)',
    children: [
      { id: 'remote-access', label: t('Access'), sections: ['remote'], toggle: 'remote' },
      { id: 'remote-devices', label: t('Devices'), sections: ['remote-devices'] },
      { id: 'remote-exposure', label: t('Exposure'), sections: ['remote-exposure'] },
      { id: 'remote-sessions', label: t('Sessions'), sections: ['remote-sessions'] },
      { id: 'remote-away', label: t('Away'), sections: ['remote-away', 'remote-away-checks'] },
      { id: 'machines', label: t('Machines'), sections: ['machines', 'remote-peers'] },
    ],
  },
]

// The blocks with a source of their own: their page, in the domain and group they name.
for (const b of BLOCK_DEFS) {
  const parent = b.settings && TREE.find((n) => n.id === b.settings!.parent)
  if (parent) (parent.children ??= []).push({ ...b.settings!.node, group: b.settings!.group })
}
// Within a domain, the pages by group, in the groups' order (a block's after the domain's own).
for (const root of TREE) {
  // A page naming no group of its domain (a module's block): after the groups.
  const rank = (n: TreeNode) => {
    const i = root.groups?.findIndex((g) => g.id === n.group) ?? 0
    return i < 0 ? root.groups!.length : i
  }
  root.children = root.children?.map((n, i) => [n, i] as const).sort(([a, i], [b, j]) => rank(a) - rank(b) || i - j).map(([n]) => n)
}

/** Every node, and its parent (`null` for a domain). */
export const NODES = new Map<string, TreeNode>()
export const PARENT = new Map<string, TreeNode | null>()
;(function index(list: TreeNode[], parent: TreeNode | null) {
  for (const n of list) {
    NODES.set(n.id, n)
    PARENT.set(n.id, parent)
    if (n.children) index(n.children, n)
  }
})(TREE, null)

/** Former ids, of pages since moved (`settings/ci` still opens Renovate). */
const MOVED: Record<string, string> = { ci: 'renovate', security: 'vulns' }

/**
 * The node a link names, by its id or one of its sections: a page, a provider (shown in its
 * page), a domain. A link to `settings/gitlab` names the provider Code › Forge › GitLab.
 */
export function nodeOf(id: string): TreeNode | undefined {
  id = MOVED[id] ?? id
  return NODES.get(id) ?? [...NODES.values()].find((n) => n.sections?.includes(id))
}

/** A provider (Forge › GitLab): a section of its parent's page, not a page of its own. */
export const isProvider = (n: TreeNode) => !!n.provider

/** The page showing a node: a provider's parent, the node itself otherwise (a domain shows its pages). */
export function pageOf(id: string): TreeNode | undefined {
  const n = nodeOf(id)
  return n && isProvider(n) ? (PARENT.get(n.id) ?? n) : n
}

/** A domain's pages, by group (`label` empty: a domain without groups). */
export function groupsOf(root: TreeNode): { id: string; label: string; pages: TreeNode[] }[] {
  const pages = root.children ?? []
  if (!root.groups) return [{ id: '', label: '', pages }]
  const known = new Set(root.groups.map((g) => g.id))
  // The pages naming no group of the domain (a module's block), last, without a label.
  const rest = { id: '', label: '', pages: pages.filter((p) => !known.has(p.group ?? '')) }
  return [...root.groups.map((g) => ({ ...g, pages: pages.filter((p) => p.group === g.id) })), rest].filter((g) => g.pages.length)
}

/** From the domain down to the node. */
export function chainOf(n: TreeNode): TreeNode[] {
  const out: TreeNode[] = []
  for (let p: TreeNode | null | undefined = n; p; p = PARENT.get(p.id)) out.unshift(p)
  return out
}

/** The node and everything under it. */
export const descendants = (n: TreeNode): TreeNode[] => [n, ...(n.children ?? []).flatMap(descendants)]
