import { BLOCK_DEFS } from './blocks'
import { t } from './i18n.svelte'

/**
 * The Settings tree: settings sorted by what they configure, not by config.toml table. A root
 * (General, Work, Code…) is a page listing its children; any other node is a page made of one or
 * more sections (Settings.svelte's own, or settings-schema.ts's), each keeping its TOML table.
 *
 * `toggle`: the section whose `enabled` key is the feature's switch, shown in the page header
 * and as the node's state in the tree (and left out of the section's rows).
 * `choice`: the key choosing which of the node's children applies (Tracker › Provider).
 * `provider`: a child that is one of its parent's choices: marked unused unless it is the chosen one
 * (or, for an agent CLI, one a repository runs), its page editable all the same.
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
  children?: TreeNode[]
}

export const TREE: TreeNode[] = [
  {
    id: 'general', label: t('General'), color: 'var(--accent)', children: [
      { id: 'profiles', label: t('Profiles'), sections: ['profiles'] },
      { id: 'packs', label: t('Packs'), sections: ['packs'] },
      { id: 'file', label: t('Config file'), sections: ['file'] },
      { id: 'logs', label: t('Logs'), sections: ['logs'] },
      { id: 'dependencies', label: t('Dependencies'), sections: ['dependencies'] },
      { id: 'updates', label: t('Updates'), sections: ['updates-now', 'updates'], toggle: 'updates' },
      { id: 'feedback', label: t('Feedback'), sections: ['feedback-requests', 'feedback', 'feedback-labels'], toggle: 'feedback' },
    ],
  },
  {
    id: 'interface', label: t('Interface'), color: 'var(--accent)', children: [
      { id: 'appearance', label: t('Appearance'), sections: ['appearance'] },
      { id: 'palette', label: t('Palette'), sections: ['palette'], toggle: 'palette' },
      { id: 'views', label: t('Pages'), sections: ['views'] },
      { id: 'custom-blocks', label: t('Custom blocks'), sections: ['custom-blocks'] },
      { id: 'alerts', label: t('Alerts & sound'), sections: ['alerts', 'digest', 'audio'] },
      { id: 'openwith', label: t('Open with'), sections: ['openwith'] },
    ],
  },
  {
    id: 'work', label: t('Work'), color: 'var(--src-clickup)', children: [
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
    id: 'code', label: t('Code'), color: 'var(--src-gitlab)', children: [
      {
        id: 'forge', label: t('Forge'), sections: ['forge'], toggle: 'forge', choice: { path: ['forge', 'provider'], fallback: 'gitlab', noun: t('the forge') }, children: [
          { id: 'forge-gitlab', label: t('GitLab'), sections: ['gitlab'], provider: 'gitlab' },
          { id: 'forge-github', label: t('GitHub'), sections: ['github'], provider: 'github' },
        ],
      },
      { id: 'repos', label: t('Repositories'), sections: ['repos', 'repo-blocks', 'worktrees'] },
    ],
  },
  {
    id: 'ci', label: t('CI'), color: 'var(--info)', children: [
      { id: 'renovate', label: t('Renovate'), sections: ['renovate'], toggle: 'renovate' },
    ],
  },
  {
    id: 'security', label: t('Security'), color: 'var(--src-alert)', children: [
      { id: 'vulns', label: t('Vulnerabilities'), sections: ['vulns'], toggle: 'vulns' },
    ],
  },
  {
    id: 'agents-root', label: t('Agents'), color: 'var(--src-claude)', children: [
      {
        id: 'agents', label: t('Agent'), sections: ['agents'], toggle: 'agents', choice: { path: ['agents', 'provider'], fallback: 'claude', noun: t('the default CLI') }, children: [
          { id: 'cli-claude', label: t('Claude Code'), sections: ['agents-claude'], provider: 'claude' },
          { id: 'cli-command-code', label: t('Command Code'), sections: ['agents-command-code'], provider: 'command-code' },
          { id: 'cli-opencode', label: t('OpenCode'), sections: ['agents-opencode'], provider: 'opencode' },
          { id: 'cli-codex', label: t('Codex'), sections: ['agents-codex'], provider: 'codex' },
        ],
      },
      { id: 'quick-answers', label: t('Quick answers'), sections: ['agents-quick'], toggle: 'agents-quick' },
      { id: 'quick-actions', label: t('Quick actions'), sections: ['agents-actions'] },
      { id: 'routines', label: t('Routines'), sections: ['routines'] },
      { id: 'overlaps', label: t('Overlaps'), sections: ['agents-overlaps'], toggle: 'agents-overlaps' },
      { id: 'aside', label: t('In passing'), sections: ['agents-aside'], toggle: 'agents-aside' },
      { id: 'dictation', label: t('Dictation'), sections: ['agents-dictation', 'dictation-panel'], toggle: 'agents-dictation' },
      { id: 'prism-mcp', label: t('Prism MCP'), sections: ['mcp', 'agents-mcp', 'mcp-agents'], toggle: 'mcp' },
      { id: 'files', label: t('Files'), sections: ['files'] },
      { id: 'console', label: t('Console'), sections: ['console'] },
    ],
  },
  {
    id: 'infra', label: t('Infra'), color: 'var(--src-k8s)', children: [
      {
        id: 'kubernetes', label: t('Kubernetes'), sections: ['clusters', 'envs'], toggle: 'clusters', children: [
          { id: 'pods', label: t('Pods'), sections: ['pods'] },
          { id: 'argocd', label: t('ArgoCD'), sections: ['argocd', 'argocd-layout'] },
        ],
      },
    ],
  },
  {
    id: 'remote-root', label: t('Remote'), color: 'var(--violet)', children: [
      { id: 'remote-access', label: t('Access'), sections: ['remote'], toggle: 'remote' },
      { id: 'remote-devices', label: t('Devices'), sections: ['remote-devices'] },
      { id: 'remote-exposure', label: t('Exposure'), sections: ['remote-exposure'] },
      { id: 'remote-sessions', label: t('Sessions'), sections: ['remote-sessions'] },
      { id: 'remote-away', label: t('Away'), sections: ['remote-away', 'remote-away-checks'] },
      { id: 'machines', label: t('Machines'), sections: ['machines', 'remote-peers'] },
    ],
  },
  // What is used, what is not, what differs from its default: last, apart from the domains.
  { id: 'cleanup', label: t('Cleanup'), color: 'var(--text-muted)', sections: ['cleanup', 'cleanup-usage'] },
]

// The blocks with a source of their own: their page, under the root they name.
for (const b of BLOCK_DEFS) {
  const parent = b.settings && TREE.find((n) => n.id === b.settings!.parent)
  if (parent) (parent.children ??= []).push(b.settings!.node)
}

/** Every node, and its parent (`null` for a root). */
export const NODES = new Map<string, TreeNode>()
export const PARENT = new Map<string, TreeNode | null>()
;(function index(list: TreeNode[], parent: TreeNode | null) {
  for (const n of list) {
    NODES.set(n.id, n)
    PARENT.set(n.id, parent)
    if (n.children) index(n.children, n)
  }
})(TREE, null)

/** The page showing a section, or a node's id (a link to `settings/gitlab` opens Code › Forge). */
export function pageOf(id: string): TreeNode | undefined {
  return NODES.get(id) ?? [...NODES.values()].find((n) => n.sections?.includes(id))
}

/** From the root down to the node. */
export function chainOf(n: TreeNode): TreeNode[] {
  const out: TreeNode[] = []
  for (let p: TreeNode | null | undefined = n; p; p = PARENT.get(p.id)) out.unshift(p)
  return out
}

/** The node and everything under it. */
export const descendants = (n: TreeNode): TreeNode[] => [n, ...(n.children ?? []).flatMap(descendants)]
