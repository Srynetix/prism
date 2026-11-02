import { hasCore, invoke, isDesktop } from './bridge'
import { feedbackOn, loadRequests, openFeedback, requests, setDraft as setFeedbackDraft, shipped } from './feedback.svelte'
import { openUrl } from '@tauri-apps/plugin-opener'
import { agentsTarget, promptAction } from './claude-actions'
import { agentsEnabled, openAgents, openLauncher, spawnAgent } from './agents.svelte'
import { defaultSaver, replayBoot, SAVER_MODES, startSaver, type SaverMode } from './fun.svelte'
import { pref, setPref } from './layout.svelte'
import { trackerOn, live } from './live.svelte'
import { trackerName } from './tracker'
import { openMr } from './mr-view.svelte'
import { setTheme, THEMES } from './theme.svelte'
import { openTicket } from './ticket-view.svelte'
import { ticketExample } from './conventions'
import { cfg, restartApp, updatePack, type FileView } from './settings.svelte'
import { parseShortcut } from './shortcut'
import { addNote, parseStatus } from './notes.svelte'
import { parseWhen, whenLabel } from './when'
import { canRetry, canRunPipeline, mergeMr, rebaseMr, retryPipeline, runMrPipeline, setDraft } from './pipeline-actions'
import { canShowLogs, openPipeline } from './logs.svelte'
import { approve } from './mr-review.svelte'
import { copy, toast } from './toast.svelte'
import { mrKey } from './mr-ref'
import { canMerge, mergeState, openMrs, rebaseHint } from './rules'
import type { MergeRequest } from './types'
import { forgeName, meOn, mrNumber } from './forge'
import { t } from './i18n.svelte'

/**
 * Prism console, dropped down by its shortcut (`[console] key`): a command palette
 * in a terminal (open an MR, switch view, rerun a scan, a Claude skill…), with its
 * aliases. Remembered history (`[console] history` lines), Tab completion.
 */
export type Tone = 'ok' | 'muted' | 'err' | 'warn' | 'cmd' | ''
export interface Line {
  text: string
  tone?: Tone
}

export const term = $state({ open: false, lines: [] as Line[] })
/** Console shortcut, from the config (`[console] key`). */
export const consoleKey = () => parseShortcut(live.snapshot?.console?.key || '`')
const settings = () => live.snapshot?.console
const history: string[] = pref<string[]>('console.history', [])
const started = Date.now()

export function toggleConsole(open = !term.open) {
  term.open = open
  if (open && !term.lines.length) print(t('Prism console — type help'), 'muted')
}

/** A command run from the palette: its lines, collected here rather than printed (`runCommand`). */
let captured: Line[] | null = null

function print(text: string, tone: Tone = '') {
  if (captured) return void captured.push(...text.split('\n').map((l) => ({ text: l, tone })))
  term.lines.push(...text.split('\n').map((l) => ({ text: l, tone })))
  if (term.lines.length > 400) term.lines.splice(0, term.lines.length - 400)
}

const ok = (text: string) => print(`→ ${text}`, 'ok')
const err = (text: string) => print(text, 'err')

async function open(url: string) {
  if (isDesktop()) await openUrl(url)
  else window.open(url, '_blank', 'noopener')
}

interface Command {
  /** A line for the help. */
  help: string
  /** Completion of the next argument, given those before it. */
  args?: (before: string[]) => string[]
  run: (args: string[]) => unknown
}

/** Actions of `mr <iid> <action>`. */
const MR_ACTIONS = ['open', 'link', 'merge', 'rebase', 'approve', 'unapprove', 'retry', 'pipeline', 'logs', 'draft', 'ready']

/** `1957`, `!1957` or `helm!214`: the MR in the snapshot, the primary repository's first. */
function findMr(ref: string | undefined): MergeRequest | string {
  const m = /^(?:([\w.-]+)?!)?(\d+)$/.exec(ref ?? '')
  if (!m) return `${t('usage:')} mr <iid> ${MR_ACTIONS.join('|')}`
  const [repo, iid] = [m[1] ?? null, Number(m[2])]
  const all = live.snapshot?.merge_requests.filter((x) => x.iid === iid) ?? []
  const found = repo ? all.find((x) => x.repo === repo) : (all.find((x) => !x.repo) ?? (all.length === 1 ? all[0] : undefined))
  return found ?? t('no merge request {mr} in Prism', { mr: `${repo ?? ''}${mrNumber(iid)}` })
}

/** The MR actions need GitLab, through the app. */
const needsApp = () => (hasCore() ? null : t('only in the app'))

/** The longest start (up to 3 words) `parseWhen` understands, and the rest. */
function splitWhen(words: string[]): { at: Date; rest: string } | null {
  for (let n = Math.min(3, words.length - 1); n >= 1; n--) {
    const at = parseWhen(words.slice(0, n).join(' '))
    if (at) return { at, rest: words.slice(n).join(' ') }
  }
  return null
}

const COMMANDS: Record<string, Command> = {
  help: {
    help: `help — ${t('this list (the guides: ⌘?)')}`,
    run: () => {
      for (const c of Object.values(COMMANDS)) print(`  ${c.help}`)
      const aliases = Object.entries(settings()?.aliases ?? {})
      if (aliases.length) print(`  ${t('aliases:')} ${aliases.map(([a, cmd]) => `${a} → ${cmd}`).join(' · ')}`, 'muted')
    },
  },
  open: {
    get help() {
      return `open !1957 | ${ticketExample(4688)} — ${t('open a merge request or a ticket')}`
    },
    run: ([what]) => {
      if (/^[!#]\d+$/.test(what ?? '')) return (openMr({ iid: Number(what.slice(1)) }), ok(t('merge request {mr}', { mr: mrNumber(what.slice(1)) })))
      if (/^[A-Z]+-\d+$/i.test(what ?? '')) {
        if (!trackerOn()) return err(t('No tracker is read (Settings › Work › Tracker)'))
        return (openTicket(what.toUpperCase()), ok(t('ticket {id}', { id: what.toUpperCase() })))
      }
      err(`${t('usage:')} open !iid | ${ticketExample(123)}`)
    },
  },
  mr: {
    help: `mr <iid> <action> — ${MR_ACTIONS.join(' ')} (${t('merge needs --yes')})`,
    args: (before) =>
      before.length === 0 ? (live.snapshot ? openMrs(live.snapshot) : []).map((m) => (m.repo ? mrKey(m) : String(m.iid)))
      : before.length === 1 ? MR_ACTIONS
      : before[1] === 'merge' ? ['--yes'] : [],
    run: async ([ref, action, flag]) => {
      const m = findMr(ref)
      if (typeof m === 'string') return err(m)
      const key = mrKey(m)
      const p = m.head_pipeline
      const opened = m.state === 'opened' ? null : t('{mr} is merged', { mr: key })
      const ready = (why: string | null) => (why ? (err(why), false) : true)
      switch (action) {
        case 'open':
          openMr(m)
          return ok(t('merge request {mr}', { mr: key }))
        case 'link':
          await copy(m.url, t('Link to {mr} copied', { mr: key }))
          return ok(t('link to {mr} copied', { mr: key }))
        case 'merge':
          if (!ready(needsApp() ?? opened)) return
          if (!canMerge(m)) return err(t('{mr} cannot be merged now: {why}', { mr: key, why: m.auto_merge ? t('auto-merge is set') : mergeState(m).label }))
          if (flag !== '--yes') return print(t('add --yes to merge {mr} into {branch}', { mr: key, branch: m.target_branch }), 'warn')
          await mergeMr(m)
          return ok(t('{mr} merged into {branch}', { mr: key, branch: m.target_branch }))
        case 'rebase':
          if (!ready(needsApp() ?? opened)) return
          if (m.conflicts) return err(t('{mr} has conflicts with {branch}: rebase it by hand or with an agent', { mr: key, branch: m.target_branch }))
          if (!rebaseHint(m)) return err(t('{mr} does not need a rebase', { mr: key }))
          await rebaseMr(m)
          return ok(t('{forge} rebases {mr} onto {branch}', { forge: forgeName(m.forge), mr: key, branch: m.target_branch }))
        case 'approve':
        case 'unapprove': {
          if (!ready(needsApp() ?? opened)) return
          const me = meOn(live.snapshot, m)
          const mine = !!me && m.approved_by.includes(me)
          if (action === 'approve' && mine) return err(t('{mr} is already approved by you', { mr: key }))
          if (action === 'unapprove' && !mine) return err(t('{mr} is not approved by you', { mr: key }))
          await approve(m, action === 'approve', m.updated_at)
          return ok(action === 'approve' ? t('{mr} approved', { mr: key }) : t('{mr} unapproved', { mr: key }))
        }
        case 'retry':
          if (!ready(needsApp())) return
          if (!p) return err(t('{mr} has no pipeline', { mr: key }))
          if (!canRetry(m.pipeline)) return err(t('pipeline of {mr} is {status}: nothing to retry', { mr: key, status: m.pipeline ?? '' }))
          await retryPipeline(p.id)
          return ok(t('failed jobs of {mr} retried', { mr: key }))
        case 'pipeline':
          if (!ready(needsApp() ?? opened)) return
          if (!canRunPipeline(m)) return err(t('only on GitLab'))
          await runMrPipeline(m)
          return ok(t('new pipeline for {mr}', { mr: key }))
        case 'logs':
          if (!canShowLogs()) return err(t('only in the app'))
          if (!p) return err(t('{mr} has no pipeline', { mr: key }))
          openPipeline(p.id, key, m.project ?? null)
          return ok(t('logs of {mr}', { mr: key }))
        case 'draft':
        case 'ready':
          if (!ready(needsApp() ?? opened)) return
          if (m.draft === (action === 'draft')) return err(action === 'draft' ? t('{mr} is already a draft', { mr: key }) : t('{mr} is already ready', { mr: key }))
          await setDraft(m, action === 'draft')
          return ok(action === 'draft' ? t('{mr} marked as draft', { mr: key }) : t('{mr} marked ready', { mr: key }))
        default:
          err(`${t('usage:')} mr ${key} ${MR_ACTIONS.join('|')}`)
      }
    },
  },
  note: {
    help: `note <text> — ${t('a sticky note (TODO … / DOING … makes it a task)')}`,
    run: (words) => {
      if (!words.length) return err(`${t('usage:')} note <text>`)
      const parsed = parseStatus(words.join(' '))
      addNote(parsed.text, 'sticky', { status: parsed.status })
      ok(`${t('note')}${parsed.status ? ` (${parsed.status})` : ''}: ${parsed.text}`)
    },
  },
  remind: {
    help: `remind <when> <text> — ${t('a reminder: 14:30, 14h, demain 9h, lun 10h, +20m, dans 1h30, 12/10 15h')}`,
    run: (words) => {
      const w = splitWhen(words)
      if (!w || !w.rest) return err(t('usage: remind <when> <text>, e.g. remind demain 9h call Alice'))
      addNote(w.rest, 'reminder', { remind_at: w.at.toISOString() })
      ok(t('reminder set: {when} — {text}', { when: whenLabel(w.at), text: w.rest }))
    },
  },
  rescan: {
    help: `rescan — ${t('run the Trivy scan now')}`,
    run: async () => {
      if (!hasCore()) return err(t('only in the app'))
      await invoke('rescan_vulnerabilities')
      ok(t('Trivy scan queued'))
    },
  },
  refresh: {
    help: `refresh — ${t('read the tracker again')}`,
    run: async () => {
      if (!hasCore()) return err(t('only in the app'))
      if (!trackerOn()) return err(t('No tracker is read (Settings › Work › Tracker)'))
      await invoke('refresh_tickets')
      ok(t('{tracker} refreshed', { tracker: trackerName() }))
    },
  },
  theme: {
    help: `theme <name> — ${t('change the theme (theme list)')}`,
    args: () => ['list', 'system', ...THEMES.map((th) => th.id)],
    run: ([id]) => {
      if (!id || id === 'list') return print(['system', ...THEMES.map((th) => th.id)].join('  '), 'muted')
      if (id !== 'system' && !THEMES.some((th) => th.id === id)) return err(t('unknown theme {id} (theme list)', { id }))
      setTheme(id)
      ok(t('theme {id}', { id }))
    },
  },
  agent: {
    help: `agent [prompt] — ${t('an agent ([agents] provider) on that prompt, right away, in the primary repository (a /plugin:skill command passes as is); without a prompt, the agent launcher')}`,
    run: async (words) => {
      const prompt = words.join(' ').trim()
      if (!prompt) {
        if (!agentsEnabled()) return err(t('agents run in the Prism app only, with [agents] enabled'))
        toggleConsole(false)
        return openLauncher({ title: '', prompt: '', worktree: {} })
      }
      const a = promptAction(prompt)
      // In the app, the agent starts right away (the console does, it does not propose).
      if (agentsTarget() && a.launch) {
        const id = await spawnAgent(a.launch)
        toggleConsole(false)
        openAgents(id)
        return ok(`${t('agent:')} ${a.prompt}`)
      }
      if (!a.href) return err(a.why ?? t('no repository'))
      await open(a.href)
      ok(`Claude Desktop: ${a.prompt}`)
    },
  },
  screensaver: {
    help: `screensaver [${SAVER_MODES.join('|')}|random] — ${t('← / → to switch, a click or another key to wake')}`,
    args: () => [...SAVER_MODES, 'random'],
    run: ([mode]) => {
      const m = (SAVER_MODES as readonly string[]).includes(mode) || mode === 'random' ? (mode as SaverMode | 'random') : defaultSaver()
      toggleConsole(false)
      // After the Enter keystroke, so that it does not wake it right away.
      setTimeout(() => startSaver(m), 250)
    },
  },
  config: {
    help: `config — ${t('open config.toml in the editor')}`,
    run: async () => {
      if (!isDesktop()) return err(t('only in the app'))
      try {
        await invoke('open_config', { with: 'editor' })
        ok(t('config.toml in the editor'))
      } catch (e) {
        await invoke('open_config', { with: 'finder' })
        print(t('no editor ({error}): config.toml shown in the Finder', { error: String(e) }), 'warn')
      }
    },
  },
  pack: {
    help: `pack list | pack update <name> — ${t('the team packs (Settings › General › Packs)')}`,
    args: (before) => (before.length ? [] : ['list', 'update']),
    run: async ([action, name]) => {
      if (!isDesktop()) return err(t('only in the app'))
      if (action === 'list' || !action) {
        const file = await invoke<FileView>('read_config')
        if (!file.packs.length) return print(t('no packs (Settings › General › Packs)'), 'muted')
        for (const p of file.packs) {
          const desc = [p.version && `v${p.version}`, p.description, p.missing ? t('missing') : null].filter(Boolean).join(' — ')
          print(`  ${p.name}${desc ? `  ${desc}` : ''}${p.source ? `  (${p.source})` : ''}`, p.missing ? 'err' : '')
        }
        return
      }
      if (action !== 'update' || !name) return err(`${t('usage:')} pack list | pack update <name>`)
      if (!(await updatePack(name))) return err(cfg.error || t('a pack command is already running'))
      ok(t('pack {name} updated and applied', { name }))
    },
  },
  feedback: {
    help: `feedback [feature|bug|idea] [title…] | feedback list — ${t('an issue on Prism’s repository or a pack’s (Settings › General › Feedback)')}`,
    args: (before) => (before.length ? [] : ['feature', 'bug', 'idea', 'list']),
    run: async ([first, ...rest]) => {
      if (!hasCore()) return err(t('only in the app'))
      if (!feedbackOn()) return err(t('feedback is off (Settings › General › Feedback)'))
      if (first === 'list') {
        await loadRequests(true)
        if (!requests.list.length) return print(t('no requests yet'), 'muted')
        for (const r of requests.list) print(`  #${r.number}  ${r.state === 'open' ? t('open  ') : shipped(r) ? t('done  ') : t('closed')}  ${r.title}  (${r.name})`, r.state === 'open' ? '' : 'muted')
        return
      }
      const kind = (['feature', 'bug', 'idea'] as const).find((k) => k === first)
      const words = kind ? rest : [first, ...rest].filter(Boolean)
      toggleConsole(false)
      await openFeedback({ kind, where: 'console' })
      if (words.length) setFeedbackDraft({ title: words.join(' ') })
    },
  },
  reload: {
    help: `reload — ${t('start the collectors again from the config (one that went wrong); a save does it already')}`,
    run: async () => {
      if (!isDesktop()) return err(t('only in the app'))
      await invoke('reload_collectors')
      ok(t('collectors started again'))
    },
  },
  restart: {
    help: `restart — ${t('restart Prism')}`,
    run: async () => {
      if (!isDesktop()) return err(t('only in the app'))
      await restartApp()
    },
  },
  boot: {
    help: `boot — ${t('replay the boot screen')}`,
    run: () => {
      toggleConsole(false)
      setTimeout(replayBoot, 250)
    },
  },
  clear: { help: `clear — ${t('clear the console')}`, run: () => void (term.lines = []) },
}

/** Commands visible at the current Fun level. */
const available = () => Object.keys(COMMANDS)

export async function runLine(line: string) {
  const text = line.trim()
  print(`% ${text}`, 'cmd')
  if (!text) return
  if (history.at(-1) !== text) {
    history.push(text)
    history.splice(0, Math.max(0, history.length - (settings()?.history ?? 50)))
    setPref('console.history', history)
  }
  // An alias expands before executing: `r !1957` → `agent review !1957`.
  const [first, ...rest] = text.split(/\s+/)
  const alias = settings()?.aliases[first]
  const [name, ...args] = alias ? [...alias.split(/\s+/), ...rest] : [first, ...rest]
  const cmd = available().includes(name) ? COMMANDS[name] : null
  if (!cmd) return err(t('prism: command not found: {name} (try help)', { name }))
  try {
    await cmd.run(args)
  } catch (e) {
    err(String(e))
  }
}

/**
 * The commands, as the palette lists them: the console's and its aliases, one registry for both.
 * `usage`: the words after the name (`<text>`, `[prompt]`); `required`, it does nothing without them.
 */
export interface CommandInfo {
  name: string
  usage: string
  description: string
  required: boolean
  /** An alias: what it runs. */
  alias?: string
  args?: (before: string[]) => string[]
}

/** The palette offers these elsewhere (clear) or as its own entries. */
const NOT_IN_PALETTE = new Set(['clear'])

export function commands(): CommandInfo[] {
  const own = available()
    .filter((name) => !NOT_IN_PALETTE.has(name))
    .map((name) => {
      const c = COMMANDS[name]
      const [head, ...rest] = c.help.split(' — ')
      const usage = head.slice(head.indexOf(name) + name.length).trim()
      return { name, usage, description: rest.join(' — '), required: usage.startsWith('<'), args: c.args }
    })
  const aliases = Object.entries(settings()?.aliases ?? {}).map(([name, alias]) => ({ name, usage: '', description: `${t('alias:')} ${alias}`, required: false, alias }))
  return [...own, ...aliases]
}

/**
 * Runs a command from the palette: what it prints goes to a toast, or to the console when it is
 * a list (several lines: `pack list`, `help`). Kept in the console's history, as typed there.
 */
export async function runCommand(line: string) {
  captured = []
  try {
    await runLine(line)
  } finally {
    const lines = captured.filter((l) => l.tone !== 'cmd')
    captured = null
    if (lines.length > 1) {
      toggleConsole(true)
      print(`% ${line.trim()}`, 'cmd')
      term.lines.push(...lines)
    } else if (lines.length) {
      toast(lines[0].text.replace(/^→ /, ''), lines[0].tone === 'err' ? 6000 : 2600)
    }
  }
}

/** History line, going up (`-1`) or down (`+1`) from `index`. */
export const historyAt = (index: number) => history[index] ?? ''
export const historyLength = () => history.length

/** Completion: the command name, then its arguments. A single possibility: completed. */
export function complete(line: string): { line: string; options: string[] } {
  const parts = line.split(/\s+/)
  const pick = (options: string[], word: string) => options.filter((o) => o.startsWith(word))
  if (parts.length <= 1) {
    const options = pick([...available(), ...Object.keys(settings()?.aliases ?? {})], parts[0] ?? '')
    return { line: options.length === 1 ? `${options[0]} ` : line, options }
  }
  const before = parts.slice(1, -1)
  const word = parts.at(-1) ?? ''
  const options = pick(COMMANDS[parts[0]]?.args?.(before) ?? [], word)
  return { line: options.length === 1 ? `${[...parts.slice(0, -1), options[0]].join(' ')} ` : line, options }
}
