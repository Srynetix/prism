import { hasCore, isDesktop, localInvoke, localListen } from './bridge'
import { live } from './live.svelte'
import { call } from './blocks/block'
import { parseShortcut, matches } from './shortcut'
import { isMac } from './platform'
import { t } from './i18n.svelte'
import type { DictationConfig } from './bindings/DictationConfig'
import type { SoundState } from './bindings/SoundState'
import type { AgentConversation } from './agents.svelte'

/**
 * Dictation at the computer (`[dictation]`): a prompt spoken rather than typed. A microphone button
 * beside a long text field (an agent's Chat, a plan's comment, a long answer, the launcher), held
 * to speak or clicked to start and stop, and a shortcut held while that field has the focus (Fn on
 * macOS). The computer's app records in memory and transcribes on the computer (`dictation_*`); the
 * text goes in at the cursor, never sent on its own. Esc cancels. One dictation at a time.
 *
 * Without a core (the browser preview), a pretend recording and a sample sentence, to see it.
 */

const DEFAULTS: DictationConfig = { enabled: true, engine: 'auto', whisper_model: 'base', language: 'auto', shortcut: isMac ? 'Fn' : 'Ctrl+Shift+Space', toggle_on_click: true, max_seconds: 300, hints: true, input_device: '' }
export const dictationSettings = (): DictationConfig => live.snapshot?.dictation ?? DEFAULTS
/** Offered here: on, and in a desktop window (a phone has its keyboard's dictation). */
export const dictationOn = () => dictationSettings().enabled && (isDesktop() || !hasCore())

/** A field one can dictate into. */
export interface Target {
  field: () => HTMLTextAreaElement | HTMLInputElement | undefined
  text: () => string
  write: (v: string) => void
  /** Words to expect: the repository's, the branch's, the files'. */
  hints?: () => string[]
}

type Phase = 'idle' | 'starting' | 'recording' | 'transcribing'
/** Why the microphone would hear nothing, said before recording. */
export type MicWarning = { kind: 'muted' } | { kind: 'off' | 'low'; volume: number }

class State {
  phase = $state<Phase>('idle')
  /** The field dictated into (or warned about). */
  key = $state<string | null>(null)
  seconds = $state(0)
  /** The last levels, 0–1, for the meter. */
  levels = $state<number[]>([])
  device = $state('')
  /** Held (button or key): released, it stops; toggled: the next click stops. */
  mode = $state<'hold' | 'toggle'>('hold')
  warn = $state<{ key: string; why: MicWarning } | null>(null)
  error = $state<{ key: string; text: string } | null>(null)
  /** What made the last text: `whisper · base · 0.8 s`. */
  done = $state<{ key: string; text: string } | null>(null)
}
export const dictation = new State()

const targets = new Map<string, Target>()
let id = 0
/** Bumped by a cancel: an answer of an older dictation is dropped. */
let generation = 0
/** Released before the recording had started: it stops as soon as it has. */
let released = false
/** The selection when the dictation started, where the text goes. */
let selection: { start: number; end: number } | null = null
let doneTimer: ReturnType<typeof setTimeout> | undefined

/** `key`'s field takes dictation while it is mounted; the returned function lets it go. */
export function registerTarget(key: string, target: Target): () => void {
  targets.set(key, target)
  listenOnce()
  return () => {
    if (targets.get(key) === target) targets.delete(key)
    if (dictation.key === key && dictation.phase !== 'idle') cancelDictation()
  }
}

/** Each agent's conversation as its Chat last read it: the files it read or wrote are words its dictation expects. */
const conversations = new Map<string, AgentConversation>()
export function noteAgentFiles(agent: string, convo: AgentConversation | null | undefined) {
  if (convo) conversations.set(agent, convo)
  else conversations.delete(agent)
}
export const forgetAgentFiles = (agent: string) => conversations.delete(agent)

/** The files an agent's conversation read or wrote, newest first, 40 at most: worked out when a dictation needs them. */
export function filesOfAgent(agent: string): string[] {
  const convo = conversations.get(agent)
  if (!convo) return []
  const seen = new Set<string>()
  for (let t = convo.turns.length - 1; t >= 0 && seen.size < 40; t--) {
    const entries = convo.turns[t].entries
    for (let i = entries.length - 1; i >= 0 && seen.size < 40; i--) {
      const e = entries[i]
      if (e.type !== 'tool' || (e.kind !== 'read' && e.kind !== 'write') || !e.what) continue
      const name = e.what.trim().split(/[\\/]/).pop() ?? ''
      if (/^[\w.@-]+\.\w+$/.test(name)) seen.add(name)
    }
  }
  return [...seen]
}

/** The microphone muted, turned off or too low (the Sound block, `[sound]`): nothing would be heard. */
export function micWarning(): MicWarning | null {
  const s = live.snapshot?.blocks?.sound as SoundState | undefined
  const i = s?.input
  if (!s?.updated_at || !i) return null
  if (i.muted) return { kind: 'muted' }
  if (i.volume === null) return null
  if (i.volume < (s.limits.muted_under ?? 0)) return { kind: 'off', volume: i.volume }
  if (s.limits.min_input_gain > 0 && i.volume < s.limits.min_input_gain) return { kind: 'low', volume: i.volume }
  return null
}
/** What "Raise" sets the microphone to (`[sound] restore_gain`). */
export const restoreGain = () => (live.snapshot?.blocks?.sound as SoundState | undefined)?.limits.restore_gain ?? 75

/** Unmutes the microphone and raises it to `restore_gain`, then dictates. */
export async function raiseAndDictate(key: string) {
  const w = dictation.warn?.why
  dictation.warn = null
  try {
    if (w?.kind === 'muted') await call('sound', 'mute', { side: 'input', muted: false })
    await call('sound', 'volume', { side: 'input', value: restoreGain() })
  } catch (e) {
    dictation.error = { key, text: t('Microphone not raised: {error}', { error: String(e) }) }
    return
  }
  void startDictation(key, 'toggle', true)
}

/**
 * Starts dictating into `key`'s field: `hold`, it stops when released (`releaseDictation`);
 * `toggle`, at the next click. Warned first when the microphone would hear nothing, unless `force`.
 */
export async function startDictation(key: string, mode: 'hold' | 'toggle', force = false) {
  if (!dictationOn() || !targets.has(key)) return
  if (dictation.phase !== 'idle') return
  dictation.error = null
  dictation.done = null
  const why = force ? null : micWarning()
  if (why) {
    dictation.warn = { key, why }
    return
  }
  dictation.warn = null
  const f = targets.get(key)!.field()
  selection = f ? { start: f.selectionStart ?? f.value.length, end: f.selectionEnd ?? f.value.length } : null
  dictation.key = key
  dictation.mode = mode
  dictation.phase = 'starting'
  dictation.seconds = 0
  dictation.levels = []
  released = false
  const gen = ++generation
  try {
    const started = hasCore() ? await localInvoke<{ id: number; device: string; max_seconds: number }>('dictation_start') : pretendStart()
    if (gen !== generation) {
      // Cancelled while it started: this one, not a newer one.
      if (hasCore()) void localInvoke('dictation_cancel', { id: started.id })
      return
    }
    id = started.id
    dictation.device = started.device
    dictation.phase = 'recording'
    if (released) void stopDictation()
  } catch (e) {
    if (gen !== generation) return
    dictation.phase = 'idle'
    dictation.error = { key, text: t('Not recorded: {error}', { error: String(e) }) }
  }
}

/** The button or the key let go: a held dictation stops and is transcribed. */
export function releaseDictation() {
  if (dictation.mode !== 'hold') return
  if (dictation.phase === 'starting') released = true
  else if (dictation.phase === 'recording') void stopDictation()
}

/** Stops recording and puts the text in at the cursor. */
export async function stopDictation() {
  if (dictation.phase !== 'recording') return
  const key = dictation.key!
  const target = targets.get(key)
  dictation.phase = 'transcribing'
  const gen = generation
  const hints = dictationSettings().hints ? [...new Set((target?.hints?.() ?? []).map((h) => h.trim()).filter(Boolean))].slice(0, 80) : []
  try {
    const r = hasCore() ? await localInvoke<{ text: string; engine: 'system' | 'whisper'; model: string | null; millis: number }>('dictation_stop', { id, hints }) : await pretendStop()
    if (gen !== generation) return
    dictation.phase = 'idle'
    if (!r.text.trim()) {
      dictation.error = { key, text: t('Nothing heard') }
      return
    }
    if (target) insert(target, r.text.trim())
    const by = r.engine === 'system' ? t('macOS dictation') : `whisper · ${r.model ?? ''}`
    dictation.done = { key, text: `${by} · ${(r.millis / 1000).toFixed(1)} s` }
    clearTimeout(doneTimer)
    doneTimer = setTimeout(() => (dictation.done = null), 4000)
  } catch (e) {
    if (gen !== generation) return
    dictation.phase = 'idle'
    dictation.error = { key, text: t('Not transcribed: {error}', { error: String(e) }) }
  }
}

/** Drops the dictation under way: nothing goes in. */
export function cancelDictation() {
  generation++
  // Still starting: its start drops it once it answers (it knows its id then).
  if (dictation.phase === 'recording' && hasCore()) void localInvoke('dictation_cancel', { id })
  dictation.phase = 'idle'
  dictation.warn = null
}

/** A click on the button: starts (a toggle), or stops the one it started. */
export function clickDictation(key: string) {
  if (dictation.phase === 'recording' && dictation.key === key) void stopDictation()
  else if (dictation.phase === 'idle') void startDictation(key, 'toggle')
}

/** `text` put in the field at the selection it had when the dictation started, spaced from its neighbours. */
function insert(target: Target, text: string) {
  const value = target.text()
  const f = target.field()
  const focused = f && document.activeElement === f
  const at = focused ? { start: f.selectionStart ?? value.length, end: f.selectionEnd ?? value.length } : (selection ?? { start: value.length, end: value.length })
  const { value: next, caret } = insertAt(value, Math.min(at.start, value.length), Math.min(at.end, value.length), text)
  target.write(next)
  requestAnimationFrame(() => {
    const field = target.field()
    if (!field) return
    field.focus()
    field.setSelectionRange(caret, caret)
  })
}

/** `value` with `text` in place of `[start, end)`: a space before it after a word, one after it before a word. */
export function insertAt(value: string, start: number, end: number, text: string): { value: string; caret: number } {
  const before = value.slice(0, start)
  const after = value.slice(end)
  const lead = before && !/\s$/.test(before) ? ' ' : ''
  const trail = after && !/^[\s.,;:!?)]/.test(after) ? ' ' : ''
  const put = lead + text + trail
  return { value: before + put + after, caret: before.length + lead.length + text.length }
}

/* ---------- The meter, the keys ---------- */

let listening = false
/** Fn held: the dictation waits a moment, as Fn is also a modifier (Fn+←). */
let fnTimer: ReturnType<typeof setTimeout> | undefined
let fnStarted = false

/** The key of the focused field that takes dictation. */
function focusedKey(): string | null {
  const el = document.activeElement
  for (const [key, tg] of targets) if (tg.field() === el) return key
  return null
}

function listenOnce() {
  if (listening) return
  listening = true
  if (hasCore()) {
    void localListen<{ id: number; seconds: number; level: number; full: boolean; error: string | null }>('dictation-level', (e) => {
      const l = e.payload
      if (l.id !== id || dictation.phase !== 'recording') return
      meter(l.seconds, l.level)
      if (l.error) {
        const key = dictation.key
        cancelDictation()
        if (key) dictation.error = { key, text: t('The microphone stopped: {error}', { error: l.error }) }
      } else if (l.full) void stopDictation()
    })
    void localListen<boolean>('dictation-fn', (e) => fnKey(e.payload))
  }
  window.addEventListener('keydown', onKeydown, true)
  window.addEventListener('keyup', onKeyup, true)
}

function meter(seconds: number, level: number) {
  dictation.seconds = seconds
  dictation.levels = [...dictation.levels.slice(-23), level]
}

const shortcutIsFn = () => dictationSettings().shortcut.trim().toLowerCase() === 'fn'

function fnKey(down: boolean) {
  if (!isMac || !shortcutIsFn() || !dictationOn()) return
  if (down) {
    const key = focusedKey()
    if (!key || dictation.phase !== 'idle') return
    clearTimeout(fnTimer)
    fnTimer = setTimeout(() => {
      fnStarted = true
      void startDictation(key, 'hold')
    }, 250)
  } else {
    clearTimeout(fnTimer)
    if (fnStarted) {
      fnStarted = false
      releaseDictation()
    }
  }
}

/** The held key of a combination (`Ctrl+Shift+Space`): down starts, its release stops. */
let comboHeld: string | null = null

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && (dictation.phase !== 'idle' || dictation.warn)) {
    e.preventDefault()
    e.stopPropagation()
    cancelDictation()
    return
  }
  // Fn used as a modifier (Fn+←, Fn+Delete): not a dictation.
  if (fnTimer !== undefined && !fnStarted) {
    clearTimeout(fnTimer)
    fnTimer = undefined
  }
  const sc = dictationSettings().shortcut.trim()
  if (!sc || shortcutIsFn() || !dictationOn()) return
  if (comboHeld && e.repeat) {
    e.preventDefault()
    return
  }
  if (e.repeat || !matches(e, parseShortcut(sc))) return
  const key = focusedKey()
  if (!key) return
  e.preventDefault()
  e.stopPropagation()
  comboHeld = e.code
  void startDictation(key, 'hold')
}

function onKeyup(e: KeyboardEvent) {
  if (!comboHeld) return
  const sc = parseShortcut(dictationSettings().shortcut)
  const modifier = (e.key === 'Control' && sc.ctrl) || (e.key === 'Shift' && sc.shift) || (e.key === 'Alt' && sc.alt) || (e.key === 'Meta' && sc.meta)
  if (e.code === comboHeld || modifier) {
    comboHeld = null
    releaseDictation()
  }
}

/* ---------- Without a core: pretend ---------- */

let pretend: ReturnType<typeof setInterval> | undefined
let pretendAt = 0
function pretendStart() {
  pretendAt = Date.now()
  clearInterval(pretend)
  pretend = setInterval(() => {
    if (dictation.phase !== 'recording') return clearInterval(pretend)
    meter((Date.now() - pretendAt) / 1000, 0.2 + Math.random() * 0.6)
  }, 80)
  return { id: 0, device: 'MacBook Pro', max_seconds: 300 }
}
async function pretendStop() {
  clearInterval(pretend)
  await new Promise((r) => setTimeout(r, 700))
  return { text: t('For the pagination, keep the current API but replace the offset with a cursor, and add a test on the last page.'), engine: 'whisper' as const, model: 'base', millis: 700 }
}
