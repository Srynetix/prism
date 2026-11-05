import { used } from './feature-use.svelte'
import { openProblems } from './problems.svelte'
import { mrTickets } from './conventions'
import { live, trackerOn } from './live.svelte'
import { pref, setPref, showBlock, setView, toggleEditing, toggleZen } from './layout.svelte'
import { agentsEnabled, agentsSettings, agentsView, allowAgent, clearAgent, interruptAgent, openAgents, restartAgent } from './agents.svelte'
import { sendPrompt } from './agent-inbox.svelte'
import { CLIS, cliOf } from './agent-cli'
import { continueWith, targetsOf } from './agent-switch.svelte'
import { answersFor, onPhone, sendFromPhone } from './quick-answers'
import { quickActionsOn, quickPrompt, runQuickAction } from './quick-actions'
import { commands, consoleKey, runCommand, toggleConsole, type CommandInfo } from './console.svelte'
import { KIND_LABEL, KIND_ONE, search as searchEntries, type Entry, type Group, type Kind, type PaletteAction, type Row } from './palette-search'
import { flatten, mrMenu, ticketMenu, type Menu } from './menus'
import { openMr } from './mr-view.svelte'
import { openTicket } from './ticket-view.svelte'
import { canShowLogs, openPipeline } from './logs.svelte'
import { openSettings, quitApp, toggleFullscreen } from './settings.svelte'
import { repoAdd, type AddMode } from './clones.svelte'
import { openHelp } from './help.svelte'
import type { Guide } from './help-guides'
import { TREE, chainOf, descendants } from './settings-tree'
import { SCHEMA } from './settings-schema'
import { views, BLOCK_LABELS, type BlockId } from './views'
import { BLOCK_DEFS } from './blocks'
import { awaitsMyReview, mergeState, ticketIndex, waitsForYou } from './rules'
import { paletteFiles } from './palette-files.svelte'
import { mrKey } from './mr-ref'
import { forgeName, meOn } from './forge'
import { parseShortcut, shortcutLabel } from './shortcut'
import { isMac, keys } from './platform'
import { invoke, isDesktop } from './bridge'
import { toast } from './toast.svelte'
import { goToShells, refreshShells, shellGroups } from './open-shells.svelte'
import { recentPanels, reopenLastPanel, reopenPanel } from './recent-panels.svelte'
import type { Agent, MergeRequest, Snapshot, StatusOption, Ticket } from './types'
import { t, tc } from './i18n.svelte'

export { KIND_LABEL, KIND_ONE, type Entry, type Group, type Kind, type PaletteAction, type Row }

/**
 * The command palette (`[palette] key`, ⌘K): one field to go to anything Prism shows (a page, a
 * block, an agent, an MR, a ticket, a pipeline, an environment, a pod, a Settings page or key, a
 * guide) and to run its commands, the console's included (one registry: `commands()`), with their
 * words (`note buy milk`). ⌘↵ opens the actions of the chosen line: an MR's or a ticket's ⋯ menu,
 * an agent's quick answers. Everything comes from the snapshot, but the tickets: from 3 characters,
 * the tracker is searched too (`[palette] tracker_search`), for those Prism does not show.
 * On the phone (the remote), what it can show there.
 */
export const palette = $state({ open: false, query: '', selected: 0, actions: null as { entry: Entry; list: PaletteAction[]; selected: number; armed: string | null } | null })

/** The remote's pages, given by the phone app: there, the palette goes to them. */
let phonePages: { id: string; label: string; go: () => void }[] | null = null
export const usePhonePages = (pages: { id: string; label: string; go: () => void }[]) => (phonePages = pages)
const phone = () => phonePages !== null

export const paletteOn = () => live.snapshot?.palette?.enabled !== false
export const paletteKey = () => parseShortcut(live.snapshot?.palette?.key || 'Cmd+K')

export function openPalette(query = '') {
  if (!paletteOn()) return
  void refreshShells()
  palette.query = query
  palette.selected = 0
  palette.actions = null
  palette.open = true
}
export const closePalette = () => {
  palette.open = false
  palette.actions = null
}
export const togglePalette = () => (palette.open ? closePalette() : openPalette())

const RECENT = 'palette.recent'
const recent = () => pref<string[]>(RECENT, [])
function remember(e: Entry) {
  // Settings › Cleanup: the palette's use, a ticket the tracker found apart.
  used(e.found ? 'palette:tracker-search' : `palette:${e.kind}`)
  setPref(RECENT, [e.id, ...recent().filter((id) => id !== e.id)].slice(0, 8))
}

const fromMenu = (menu: Menu): PaletteAction[] => flatten(menu)

function agentActions(a: Agent): PaletteAction[] {
  const s = agentsSettings()
  const out: PaletteAction[] = answersFor(s.quick_answers, s.prompt_language, a).map((q, i) => ({
    key: `qa-${i}`,
    label: q.label,
    hint: q.sent,
    group: tc('shell', 'Answer'),
    run: () => onPhone() ? sendFromPhone(a, q).then(() => toast(t('“{label}” sent to {agent}', { label: q.label, agent: a.title })), (e) => toast(t('Not sent to {agent}: {error}', { agent: a.title, error: String(e) }), 6000)) : sendPrompt(a.id, q.sent).then((r) => toast(r === 'sent' ? t('“{label}” sent to {agent}', { label: q.label, agent: a.title }) : r === 'queued' ? t('“{label}” goes once its menu is answered', { label: q.label }) : t('Already sent to {agent}', { agent: a.title }))),
  }))
  if (a.state === 'running' && a.status === 'waiting_tool') out.push({ key: 'allow', label: t('Allow once'), hint: a.needs_action ?? undefined, run: () => allowAgent(a.id) })
  if (a.state === 'running' && a.status === 'running') out.push({ key: 'interrupt', label: t('Interrupt'), hint: keys('⌘.'), run: () => interruptAgent(a.id) })
  if (a.state === 'running' && cliOf(a.provider).transcript && !['running', 'waiting_tool'].includes(a.status)) out.push({ key: 'clear', label: t('Clear conversation'), hint: t('/clear: the transcript stays'), confirm: t('Its context empties: ↵ again to clear'), run: () => clearAgent(a.id).catch((e) => toast(tc('shell', 'Could not clear: {error}', { error: String(e) }), 6000)) })
  // Continue with each of the other CLIs: "codex" finds the one to hand it over to.
  if (a.state !== 'prepared') for (const p of targetsOf(a)) out.push({ key: `switch-${p}`, label: t('Continue with {cli}', { cli: CLIS[p].cli }), hint: t('from a relay'), run: () => continueWith(a, p) })
  if (a.state === 'running') out.push({ key: 'restart', label: t('Restart'), confirm: a.status === 'running' ? t('It is working: ↵ again to restart') : undefined, run: () => restartAgent(a.id) })
  if (a.ticket && trackerOn()) out.push({ key: 'ticket', label: t('Ticket {id}', { id: a.ticket }), run: () => openTicket(a.ticket!) })
  if (a.mr) out.push({ key: 'mr', label: mrKey(a.mr), run: () => openMr(a.mr!) })
  return out
}

const STATUS_PILL: Record<string, [string, string]> = { needs_you: [t('asks'), 'warn'], waiting_tool: [t('permission'), 'ko'], review_ready: [t('plan'), 'warn'], running: [t('working'), 'info'], background: [t('in the background'), 'info'] }

function agentEntries(s: Snapshot): Entry[] {
  if (!agentsEnabled()) return []
  return (s.agents ?? []).map((a) => {
    const live = a.state === 'running'
    const [label, tone] = live ? (STATUS_PILL[a.status] ?? [t('idle'), '']) : [tc('agent', a.state), '']
    return {
      id: `agent:${a.id}`,
      kind: 'agent',
      title: a.title,
      hint: [a.ticket, a.branch].filter(Boolean).join(' · ') || a.cwd,
      words: [a.ticket, a.branch, ...a.tags, a.category].filter(Boolean).join(' '),
      pill: { label, tone },
      waiting: live && waitsForYou(a.status),
      run: () => openAgents(a.id),
      actions: () => agentActions(a),
    } satisfies Entry
  })
}

/** At most this many files offered: a whole repository is not searched letter by letter. */
const MAX_FILES = 5000

/** The files of the agent whose Files tab was shown last: one opens there, in a tab kept. */
function fileEntries(): Entry[] {
  const { agent, title, paths } = paletteFiles
  if (!agent || !agentsEnabled()) return []
  return paths.slice(0, MAX_FILES).map((path) => {
    const cut = path.lastIndexOf('/') + 1
    return {
      id: `file:${agent}:${path}`,
      kind: 'file',
      title: path.slice(cut),
      hint: `${path.slice(0, cut) || './'} · ${title}`,
      words: path,
      run: () => {
        openAgents(agent)
        agentsView.side = 'files'
        // The editor (CodeMirror) loaded only now: the palette is a phone's too.
        void import('./files-editor.svelte').then((m) => m.openFile(agent, path, { pin: true }))
      },
    } satisfies Entry
  })
}

function mrEntries(s: Snapshot): Entry[] {
  return s.merge_requests.map((m: MergeRequest) => {
    const state = mergeState(m)
    const review = awaitsMyReview(m, meOn(s, m))
    return {
      id: `mr:${mrKey(m)}`,
      kind: 'mr',
      title: `${mrKey(m)} ${m.title}`,
      hint: [mrTickets(m).join(' '), m.source_branch, m.author].filter(Boolean).join(' · '),
      words: [m.source_branch, m.author, ...mrTickets(m)].filter(Boolean).join(' '),
      pill: review ? { label: t('to review'), tone: 'warn' } : { label: state.label, tone: state.tone ?? '' },
      waiting: review,
      run: () => openMr(m),
      actions: () => fromMenu(mrMenu(m, s.merge_queue_label, meOn(s, m))),
    } satisfies Entry
  })
}

function ticketEntries(s: Snapshot): Entry[] {
  if (!trackerOn()) return []
  const index = ticketIndex(s)
  const extra: { t: Ticket; statuses: StatusOption[] }[] = [...(s.next_sprint?.tickets ?? []), ...(s.refinement?.boards.flatMap((b) => b.tickets) ?? [])].map((t) => ({ t, statuses: [] }))
  const seen = new Set<string>()
  return [...index.values(), ...extra].flatMap(({ t, statuses }) => {
    if (seen.has(t.id)) return []
    seen.add(t.id)
    return [{
      id: `ticket:${t.id}`,
      kind: 'ticket',
      title: `${t.id} ${t.name}`,
      hint: t.status,
      words: t.assignees.map((a) => a.name).join(' '),
      run: () => openTicket(t.id),
      actions: () => fromMenu(ticketMenu(t, s.tracker_me, statuses)),
    } satisfies Entry]
  })
}

function pipelineEntries(s: Snapshot): Entry[] {
  return s.pipelines.map((p) => ({
    id: `pipeline:${p.project ?? ''}:${p.id}`,
    kind: 'pipeline',
    title: `${p.target} #${p.id}`,
    hint: [p.repo, p.status].filter(Boolean).join(' · '),
    pill: { label: tc('pipeline', p.status), tone: p.status === 'failed' ? 'ko' : p.status === 'success' ? 'ok' : 'info' },
    run: () => (canShowLogs() ? openPipeline(p.id, p.target, p.project ?? null) : showBlock('ci')),
    actions: () => [{ key: 'forge', label: t('In {forge}', { forge: forgeName() }), href: p.url }],
  } satisfies Entry))
}

function infraEntries(s: Snapshot): Entry[] {
  const envs = (s.environments?.envs ?? []).map((e) => ({ id: `env:${e.env}`, kind: 'env' as const, title: e.env, hint: [e.cluster, e.namespace].filter(Boolean).join(' · '), run: () => showBlock('envs') }))
  const pods = s.pods.map((p) => ({
    id: `pod:${p.context}:${p.namespace}:${p.name}`,
    kind: 'pod' as const,
    title: p.name,
    hint: [p.namespace, p.status].join(' · '),
    words: p.workload ?? '',
    pill: p.healthy ? undefined : { label: p.status, tone: 'ko' },
    run: () => showBlock('pods'),
  }))
  return [...envs, ...pods]
}

/** The shells still open (open-shells.svelte.ts) and the panels closed lately (recent-panels.svelte.ts), to go back to them. */
function backEntries(): Entry[] {
  const shells = shellGroups().map((g) => ({
    id: `shell:${g.id}`,
    kind: 'shell' as const,
    title: g.title,
    hint: g.where,
    words: g.pod ? 'shell pod kubectl' : 'shell worktree',
    pill: g.pod && g.ended ? { label: t('kubectl exited'), tone: 'ko' } : undefined,
    run: () => goToShells(g),
  }))
  // A shell's drawer closed: its shells are listed above.
  const closed = recentPanels.list.filter((p) => p.kind !== 'shell').map((p) => ({ id: `closed:${p.key}`, kind: 'closed' as const, title: p.title, hint: p.hint || undefined, keys: p === recentPanels.list[0] ? keys('⌘⇧T') : undefined, run: () => reopenPanel(p) }))
  return [...shells, ...closed]
}

function pageEntries(): Entry[] {
  if (phonePages) return phonePages.map((p) => ({ id: `page:${p.id}`, kind: 'page', title: p.label, run: p.go }))
  const pages: Entry[] = views().map((v, i) => ({ id: `page:${v.id}`, kind: 'page', title: v.label, keys: keys(`⌘${i + 1}`), run: () => setView(v.id) }))
  if (agentsEnabled()) pages.push({ id: 'page:agents', kind: 'page', title: t('Agents'), keys: keys(`⌘${views().length + 1}`), run: () => openAgents() })
  const blocks: Entry[] = [...new Set<string>([...Object.keys(BLOCK_LABELS), ...BLOCK_DEFS.map((b) => b.id)])].map((id) => ({
    id: `block:${id}`,
    kind: 'block',
    title: t(BLOCK_LABELS[id as BlockId] ?? BLOCK_DEFS.find((b) => b.id === id)?.label ?? id),
    hint: t('block'),
    run: () => showBlock(id),
  }))
  return [...pages, ...blocks]
}

/** As ⌘N: the Agents view, its launcher open. */
function newAgent() {
  openAgents()
  agentsView.creating = true
  agentsView.desktopHref = null
}

/** Settings › Repositories, its Add panel open on `mode`. */
function addRepo(mode: AddMode) {
  repoAdd.publish = ''
  repoAdd.mode = mode
  openSettings('repos')
}

/** The palette's own commands, then the console's: one registry. */
function commandEntries(): Entry[] {
  const own: Entry[] = phone() ? [] : [
    ...(agentsEnabled() ? [{ id: 'cmd:new-agent', kind: 'command' as const, title: t('New agent'), words: 'new agent', keys: keys('⌘N'), run: newAgent }] : []),
    { id: 'cmd:reopen', kind: 'command', title: t('Reopen the last closed panel'), words: 'reopen closed panel undo restore', keys: keys('⌘⇧T'), run: reopenLastPanel },
    { id: 'cmd:zen', kind: 'command', title: t('Zen mode'), words: 'zen', keys: keys('⌘⇧F'), run: toggleZen },
    { id: 'cmd:edit-layout', kind: 'command', title: t('Edit layout'), words: 'edit layout', run: toggleEditing },
    { id: 'cmd:problems', kind: 'command', title: t('Problems'), hint: t('What fails now, and what failed lately'), words: 'problems errors warnings failures log', run: () => openProblems() },
    { id: 'cmd:settings', kind: 'command', title: t('Settings'), words: 'settings', keys: keys('⌘,'), run: () => openSettings() },
    { id: 'cmd:repo-add', kind: 'command', title: t('Add a repository'), hint: t('An existing folder, or a clone'), words: 'add repository', run: () => addRepo('folder') },
    { id: 'cmd:repo-new', kind: 'command', title: t('New project'), hint: t('A folder with a git repository'), words: 'new project create repository git init', run: () => addRepo('new') },
    { id: 'cmd:repo-publish', kind: 'command', title: t('Publish a local project'), hint: t('Its project on the forge, its branch pushed'), words: 'publish push forge gitlab github remote origin', run: () => addRepo('publish') },
    { id: 'cmd:help', kind: 'command', title: t('Help'), words: 'help', keys: keys('⌘?'), run: () => openHelp() },
    { id: 'cmd:console', kind: 'command', title: t('Console'), words: 'console', keys: shortcutLabel(consoleKey()), run: () => toggleConsole(true) },
    // The macOS menu bar's own, which Linux has not.
    ...(isDesktop() ? [
      { id: 'cmd:fullscreen', kind: 'command' as const, title: t('Full screen'), words: 'fullscreen maximize', keys: isMac ? keys('⌃⌘F') : 'F11', run: () => void toggleFullscreen() },
      { id: 'cmd:quit', kind: 'command' as const, title: t('Quit Prism'), words: 'quit exit close', keys: keys('⌘Q'), run: () => void quitApp() },
    ] : []),
  ]
  // On the phone: what a phone can do (notes are shared, agents start in a new worktree).
  const PHONE = new Set(['note', 'remind', 'refresh', 'agent', 'open'])
  const console = commands().filter((c) => !phone() || PHONE.has(c.name)).map((c: CommandInfo): Entry => ({
    id: `cmd:${c.name}`,
    kind: 'command',
    title: c.name,
    hint: c.description,
    usage: c.usage || undefined,
    required: c.required,
    run: (words) => runCommand(`${c.name}${words ? ` ${words}` : ''}`),
  }))
  return [...own, ...console]
}

/** The keys of the sections with a layout of their own (not in `SCHEMA`), to be found by them. */
const CUSTOM_KEYS: Record<string, string> = {
  appearance: 'theme theme_preview density ignore_reduce boot animations theme_motion celebrations easter_eggs friday_guard font font_mono font_terminal locale screensaver screensaver_seconds screensaver_mode tooltips tooltip_delay ticket_threads page_swipe pin_drawers statusbar statusbar_hidden',
  console: 'console key history aliases',
  notes: 'notes key quick notes',
  profiles: 'profiles profile',
  packs: 'packs pack import update',
  file: 'config.toml file',
  'updates-now': 'updates install version release',
  'remote-devices': 'remote phone devices pair passkey',
  'custom-blocks': 'custom blocks script command json own block',
  'remote-away-checks': 'remote away leave sleep filevault restart power cut crash keep running awake',
  'feedback-requests': 'feedback requests issues',
  'cleanup-usage': 'cleanup usage unused never used most used default reset',
}

function settingEntries(): Entry[] {
  if (phone()) return []
  const fields = new Map(SCHEMA.map((sp) => [sp.id, sp]))
  return TREE.flatMap(descendants).flatMap((n) => {
    const path = chainOf(n).map((x) => t(x.label)).join(' › ')
    const specs = (n.sections ?? []).map((id) => fields.get(id)).filter((x) => !!x)
    const page: Entry = {
      id: `setting:${n.id}`,
      kind: 'setting',
      title: `${t('Settings')} › ${path}`,
      hint: specs.map((sp) => sp.table).join(' '),
      words: [...specs.flatMap((sp) => sp.fields.map((f) => `${f.key} ${f.label}`)), ...(n.sections ?? []).map((id) => CUSTOM_KEYS[id] ?? '')].join(' '),
      run: () => openSettings(n.id),
    }
    const keyed: Entry[] = specs.flatMap((sp) => sp.fields.map((f) => ({ id: `setting:${n.id}:${f.key}`, kind: 'setting' as const, title: t(f.label), hint: `${t('Settings')} › ${path} · ${f.key}`, words: `${f.key} ${f.label}`, run: () => openSettings(n.id) })))
    return [page, ...keyed]
  })
}

/**
 * The guides, read the first time the palette lists them (at the desk, where they are already in
 * the window's code: at once); never on the phone, which leaves them out and so never downloads them.
 */
let guides: Guide[] = []
let guidesAsked = false
function helpEntries(): Entry[] {
  if (phone()) return []
  if (!guidesAsked) {
    guidesAsked = true
    void import('./help-guides').then((m) => (guides = m.guides))
  }
  return guides.flatMap((g) => [
    { id: `help:${g.id}`, kind: 'help' as const, title: `${t('Help')} › ${g.title}`, hint: t(g.group), run: () => openHelp(g.id) },
    ...[...g.body.matchAll(/^##\s+(.+)$/gm)].map((m) => ({ id: `help:${g.id}:${m[1]}`, kind: 'help' as const, title: m[1].trim(), hint: `${t('Help')} › ${g.title}`, run: () => openHelp(g.id) })),
  ])
}

/** The tracker's answer to a query (`search_tickets`): the tickets it found, beyond the snapshot's. */
export const found = $state({ query: '', tickets: [] as Ticket[], busy: false })
const SEARCH_FROM = 3
const trackerSearchOn = () => trackerOn() && live.snapshot?.palette?.tracker_search !== false
let wanted = ''
let timer: ReturnType<typeof setTimeout> | undefined

/** What is typed, searched in the tracker once the typing pauses; a command and its words are not. */
export function lookUp(query: string) {
  const q = query.trim()
  clearTimeout(timer)
  wanted = q
  const head = q.split(/\s+/)[0].toLowerCase()
  if (!palette.open || !trackerSearchOn() || q.length < SEARCH_FROM || (/\s/.test(q) && commands().some((c) => c.name === head))) {
    Object.assign(found, { query: '', tickets: [], busy: false })
    return
  }
  if (found.query === q) return
  found.busy = true
  timer = setTimeout(async () => {
    let tickets: Ticket[] = []
    try {
      tickets = await invoke<Ticket[]>('search_tickets', { query: q })
    } catch (e) {
      console.warn('palette: tracker search', e)
    }
    if (wanted === q) Object.assign(found, { query: q, tickets, busy: false })
  }, 300)
}

/** The tickets the tracker found and Prism does not show, and a typed id (`ABC-123`) opened as is. */
function foundEntries(query: string, all: Entry[]): Entry[] {
  if (!trackerOn() || !live.snapshot) return []
  const s = live.snapshot
  const known = new Set(all.filter((e) => e.kind === 'ticket').map((e) => e.id))
  const q = query.trim()
  // Kept whatever their title for the query they answer; while the next one is read, as any line.
  const current = found.query === q
  const out: Entry[] = found.tickets.filter((t) => !known.has(`ticket:${t.id}`)).map((t) => ({
    id: `ticket:${t.id}`,
    kind: 'ticket',
    title: `${t.id} ${t.name}`,
    hint: [t.status, t.list?.name].filter(Boolean).join(' · '),
    words: t.assignees.map((a) => a.name).join(' '),
    found: current,
    run: () => openTicket(t.id),
    actions: () => fromMenu(ticketMenu(t, s.tracker_me, [])),
  } satisfies Entry))
  const id = q.toUpperCase()
  if (/^[A-Z][A-Z0-9]*-\d+$/.test(id) && !known.has(`ticket:${id}`) && !out.some((e) => e.id === `ticket:${id}`)) {
    out.unshift({ id: `ticket:${id}`, kind: 'ticket', title: id, hint: t('Open this ticket'), found: true, run: () => openTicket(id) })
  }
  return out
}

/** The quick actions, one line per repository they are offered on: ↵ launches, ⌘↵ changes it first. */
function quickEntries(s: Snapshot): Entry[] {
  if (!agentsEnabled()) return []
  return (s.repos ?? []).filter((r) => r.path && !r.error).flatMap((r) =>
    quickActionsOn(r.name).map((a, i) => ({
      // Its place too: two of the same name stay two lines.
      id: `quick:${r.name}:${i}:${a.name}`,
      kind: 'command' as const,
      title: a.name,
      hint: `${r.name} · ${quickPrompt(a, r)}`,
      words: `${r.name} quick action agent`,
      run: () => runQuickAction(a, r.name),
      actions: isDesktop() ? () => [{ key: 'edit', label: t('Change it before it goes'), hint: t('The launcher, prefilled'), run: () => runQuickAction(a, r.name, true) }] : undefined,
    })),
  )
}

/** Everything the palette can find, now. */
export function entries(): Entry[] {
  const s = live.snapshot
  const objects = s ? [...agentEntries(s), ...mrEntries(s), ...ticketEntries(s), ...fileEntries(), ...pipelineEntries(s), ...(phone() ? [] : infraEntries(s))] : []
  return [...objects, ...(phone() ? [] : backEntries()), ...pageEntries(), ...(s ? quickEntries(s) : []), ...commandEntries(), ...settingEntries(), ...helpEntries()]
}

/** What the query finds (`palette-search.ts`), among everything the palette can find now. */
export const search = (query: string, all: Entry[] = entries()): Group[] => searchEntries(query, [...all, ...foundEntries(query, all)], recent())

/** ↵ on a line: a command waiting for its words gets them typed; anything else runs, and closes. */
export async function choose(row: Row) {
  const e = row.entry
  if (e.kind === 'command' && e.required && !row.words) {
    palette.query = `${e.title} `
    return
  }
  remember(e)
  closePalette()
  try {
    await e.run(row.words)
  } catch (err) {
    toast(String(err), 6000)
  }
}

/** ⌘↵: the line's actions, in the palette. */
export function showActions(row: Row) {
  const list = row.entry.actions?.() ?? []
  if (!list.length) return toast(t('No other action here'))
  palette.actions = { entry: row.entry, list, selected: 0, armed: null }
}

/** ↵ on an action: one asking for a confirmation needs a second ↵. */
export async function runAction(a: PaletteAction) {
  const state = palette.actions
  if (!state) return
  if (a.confirm && state.armed !== a.key) {
    state.armed = a.key
    return
  }
  remember(state.entry)
  closePalette()
  try {
    if (a.run) await a.run()
    else if (a.href) (await import('./links')).openLink(a.href)
  } catch (err) {
    toast(String(err), 6000)
  }
}
