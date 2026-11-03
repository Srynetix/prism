import type { InvokeArgs } from '@tauri-apps/api/core'
import type { EventCallback, UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { openUrl } from '@tauri-apps/plugin-opener'
import { localInvoke as tauriInvoke, type Transport } from './bridge'
import { applyChanges, type Change } from './snapshot-changes'

/**
 * A machine's window (`[[machines]]`, `machines.rs`): the window's page, its core another
 * computer's Prism. Its calls go there (`machine_call`, through this app, which holds the token),
 * but those of this window itself (its menu, clipboard, fonts, audio, profiles), which this
 * computer answers. The snapshot comes whole when the socket opens, then as changes; it is given
 * to the page as the main window's is (`snapshot_sync`, `snapshot-changes`).
 */

/** What the window shows of its machine's socket (the bar over the page). */
export const machineState = $state<{ name: string; ssh: string; connected: boolean; error: string | null; since: number | null }>({
  name: '',
  ssh: '',
  connected: false,
  error: null,
  since: null,
})

/** Answered by this computer: the window's own commands, and what reads or writes here. */
const LOCAL = new Set([
  'list_profiles', 'create_profile', 'setup_profile', 'rename_profile', 'remove_profile', 'switch_profile', 'open_profile',
  'set_help_guides', 'app_icon', 'font_face', 'font_faces', 'youtube_relay', 'copy_text', 'paste_text', 'quit_app', 'toggle_fullscreen',
  'audio_sources', 'audio_feed', 'now_playing', 'now_playing_command', 'feedback_capture', 'reduce_transparency',
  // Written where this computer's save dialog said.
  'save_csv',
  'machine_list', 'machine_open', 'machine_token', 'machine_main',
])
/** The main window's alone: its Dock icon and menu bar count. */
const IGNORED = new Set(['set_dock_icon', 'set_tray'])
/** Pushed by this computer's app (its menu bar, its player), not by the machine. */
const LOCAL_EVENTS = new Set(['menu', 'now-playing'])

/** The editor opening a folder of the machine here: VS Code over SSH, when `[[machines]] ssh` says how. */
const remoteEditor = (path: string) => (machineState.ssh ? `vscode://vscode-remote/ssh-remote+${encodeURIComponent(machineState.ssh)}${encodeURI(path)}` : null)

let first: () => void = () => {}
const firstOne = new Promise<void>((resolve) => (first = resolve))
/** Resolved once the machine's snapshot came, the first time. */
export const firstSnapshot = () => firstOne

export function openMachine(): Transport {
  const win = getCurrentWebviewWindow()
  let snap: unknown = null
  let rev = 0
  const waiters: (() => void)[] = []
  const listeners = new Map<string, Set<EventCallback<unknown>>>()
  const emit = (event: string, payload: unknown) => {
    for (const h of listeners.get(event) ?? []) h({ event, id: 0, payload } as never)
  }

  void win.listen<Record<string, unknown>>('machine', ({ payload: m }) => {
    if (m.t === 'snapshot') {
      // Whole: a revision the page does not have, which makes it ask for this one (`snapshot_sync`).
      snap = m.value
      rev += 2
      emit('snapshot-changes', { base: rev - 1, rev, changes: [] })
      waiters.splice(0).forEach((w) => w())
      first()
    } else if (m.t === 'changes') {
      snap = applyChanges(snap, m.changes as Change[])
      emit('snapshot-changes', { base: rev, rev: rev + 1, changes: m.changes })
      rev += 1
    } else if (m.t === 'state') {
      machineState.connected = !!m.connected
      machineState.error = (m.error as string | null) ?? null
      if (m.connected) machineState.since = null
      else machineState.since ??= Date.now()
    }
  })
  void tauriInvoke<{ name: string; ssh: string }>('machine_connect')
    .then((m) => {
      machineState.name = m.name
      machineState.ssh = m.ssh
    })
    .catch((e) => {
      machineState.error = String(e)
      machineState.since = Date.now()
    })

  const snapshot = () => (snap ? Promise.resolve(snap) : new Promise((resolve) => waiters.push(() => resolve(snap))))

  /** A folder of the machine opened here: in the editor over SSH, nothing else reaches it. */
  const open = (path: string | undefined, target: unknown) => {
    const url = target === 'editor' && path ? remoteEditor(path) : null
    if (!url) return Promise.reject(`on ${machineState.name || 'the machine'}: its folders open there, not on this computer${target === 'editor' ? ' ([[machines]] ssh opens them in VS Code)' : ''}`)
    return openUrl(url)
  }
  const agentDir = (id: unknown) => (snap as { agents?: { id: string; cwd: string }[] } | null)?.agents?.find((a) => a.id === id)?.cwd

  const call = async <T>(command: string, args?: InvokeArgs): Promise<T> => {
    const a = (args ?? {}) as Record<string, unknown>
    if (LOCAL.has(command)) return tauriInvoke<T>(command, args)
    if (IGNORED.has(command)) return undefined as T
    switch (command) {
      case 'get_snapshot':
        return (await snapshot()) as T
      case 'snapshot_sync':
        return { rev, snapshot: await snapshot() } as T
      case 'agent_attach':
        return tauriInvoke<T>('machine_attach', { id: a.id, cols: a.cols, rows: a.rows, onData: a.onData })
      case 'agent_write':
        return tauriInvoke<T>('machine_write', { id: a.id, data: a.data })
      case 'agent_resize':
        return tauriInvoke<T>('machine_resize', { id: a.id, cols: a.cols, rows: a.rows })
      case 'agent_detach':
        return tauriInvoke<T>('machine_detach', { id: a.id })
      case 'open_repo':
        return (await open(a.path as string, a.with)) as T
      case 'agent_open':
        return (await open(agentDir(a.id), a.with)) as T
      case 'agent_open_file': {
        const dir = agentDir(a.id)
        return (await open(dir && `${dir}/${a.path as string}`, 'editor')) as T
      }
    }
    return tauriInvoke<T>('machine_call', { command, args: args ?? null })
  }

  return {
    invoke: call,
    listen: async <T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> => {
      if (LOCAL_EVENTS.has(event)) return win.listen<T>(event, handler)
      const set = listeners.get(event) ?? new Set()
      set.add(handler as EventCallback<unknown>)
      listeners.set(event, set)
      return () => set.delete(handler as EventCallback<unknown>)
    },
  }
}
