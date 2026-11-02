import { hasCore, isDesktop } from './bridge'
import { onShared, shareSave } from './shared.svelte'
import { canNotify, notify } from './notify'
import { read, write } from './persist'
import { t } from './i18n.svelte'

/**
 * Free notes, stored by the app in its data folder, shared by the window and the phones (the
 * remote) through the core (`shared.svelte.ts`). Three kinds:
 * - `sticky`: always displayed, whatever the day (the notes from before kinds existed);
 * - `day`: attached to a day, displayed only when that day is selected;
 * - `reminder`: a reminder at a given time, notified by macOS, then marked done or snoozed.
 * Any of them may belong to a repository (`repo`, its `[[repos]] name`): the Notes block filters
 * and groups on it, the Repos menu counts them, an agent working in it lists them.
 */
export type NoteKind = 'sticky' | 'day' | 'reminder'

/**
 * Status of a sticky or day note, to make it a task: to do, in progress, done
 * (folded with the done reminders). Without status, a note stays a plain note.
 */
export type NoteStatus = 'todo' | 'doing' | 'done'
export const STATUS_LABEL: Record<NoteStatus, string> = { todo: 'TODO', doing: 'DOING', done: 'DONE' }
const STATUSES = Object.keys(STATUS_LABEL) as NoteStatus[]

export interface Note {
  id: string
  text: string
  created_at: string
  updated_at: string
  kind: NoteKind
  /** `day`: the day (`YYYY-MM-DD`). */
  day?: string
  /** `reminder`: the due time (ISO). */
  remind_at?: string
  /** `reminder`: the notification has gone out (only once per due time). */
  notified?: boolean
  /** `reminder`: done (kept, folded in the block). */
  done_at?: string | null
  /** `sticky`, `day`: task status, and since when. */
  status?: NoteStatus | null
  status_at?: string
  /** The repository it is about (`[[repos]] name`), if any. */
  repo?: string | null
}

function load(): Note[] {
  return normalize(read<Partial<Note>[]>('notes'))
}

function normalize(saved: unknown): Note[] {
  if (!Array.isArray(saved)) return []
  return saved
    .filter((n: Partial<Note>): n is Note => typeof n?.id === 'string' && typeof n?.text === 'string')
    .map((n) => ({
      ...n,
      kind: n.kind === 'day' || n.kind === 'reminder' ? n.kind : 'sticky',
      status: n.status && STATUSES.includes(n.status) ? n.status : null,
      repo: typeof n.repo === 'string' && n.repo ? n.repo : null,
    }))
}

export const notes = $state<Note[]>(load())

/** True if the last write failed (disk, permissions): the block reports it. */
export const storage = $state({ failed: false })

function save() {
  storage.failed = false
  // The browser preview: its own storage.
  if (!hasCore()) return write('notes', notes, () => (storage.failed = true))
  shareSave('notes', () => $state.snapshot(notes), () => (storage.failed = true))
}

// Written elsewhere (the window, a phone): read again.
onShared('notes', (saved) => notes.splice(0, notes.length, ...normalize(saved)), () => (storage.failed = true))

export function addNote(text: string, kind: NoteKind = 'sticky', opts: { day?: string; remind_at?: string; status?: NoteStatus | null; repo?: string | null } = {}) {
  const now = new Date().toISOString()
  notes.unshift({ id: crypto.randomUUID(), text, created_at: now, updated_at: now, kind, ...opts, ...(opts.status ? { status_at: now } : {}) })
  save()
}

export function setStatus(id: string, status: NoteStatus | null) {
  const n = notes.find((x) => x.id === id)
  if (!n || (n.status ?? null) === status) return
  n.status = status
  n.status_at = new Date().toISOString()
  save()
}

/** Moved to a repository, or out of any (`null`). */
export function setRepo(id: string, repo: string | null) {
  const n = notes.find((x) => x.id === id)
  if (!n || (n.repo ?? null) === repo) return
  n.repo = repo
  n.updated_at = new Date().toISOString()
  save()
}

/** Still to deal with: neither a done reminder nor a DONE note (a day's notes, whatever the day). */
export const isOpen = (n: Note) => (n.kind === 'reminder' ? !n.done_at : n.status !== 'done')

/** A repository's open notes, in the Notes block's default order (reminders first, by time). */
export function repoNotes(repo: string, list: Note[] = notes): Note[] {
  const rank = (n: Note) => (n.kind === 'reminder' ? 0 : 1)
  return list
    .filter((n) => n.repo === repo && isOpen(n))
    .sort((a, b) => rank(a) - rank(b) || (a.kind === 'reminder' && b.kind === 'reminder' ? (a.remind_at ?? '').localeCompare(b.remind_at ?? '') : byStatus(a, b)))
}

/** The repositories notes are about, with how many open notes each has, by name. */
export function allRepos(list: Note[] = notes): { repo: string; count: number }[] {
  const counts = new Map<string, number>()
  for (const n of list) if (n.repo) counts.set(n.repo, (counts.get(n.repo) ?? 0) + (isOpen(n) ? 1 : 0))
  return [...counts].map(([repo, count]) => ({ repo, count })).sort((a, b) => a.repo.localeCompare(b.repo))
}

/** `TODO appeler X` or `doing: revue !1793`: the typed prefix gives the status, and disappears from the text. */
export function parseStatus(text: string): { text: string; status: NoteStatus | null } {
  const m = /^(todo|doing|done)\b[\s:]*/i.exec(text)
  if (!m || !text.slice(m[0].length).trim()) return { text, status: null }
  return { text: text.slice(m[0].length).trim(), status: m[1].toLowerCase() as NoteStatus }
}

/**
 * Tags: `#infra`, `#perso/admin` written in the text, at its start or after a space, a letter
 * first (`#1957` stays a GitHub reference, `C#` no tag). Kept in the text: editing it changes them.
 */
const TAG = /(^|\s)#(\p{L}[\p{L}\p{N}_/-]*)/gu

/** A note's tags, lower case, in order, once each. */
export function tagsOf(text: string): string[] {
  return [...new Set([...text.matchAll(TAG)].map((m) => m[2].replace(/[/-]+$/, '').toLowerCase()))]
}

/** The text without its tags, spaces tidied: what an agent receives (tags only sort my notes). */
export function withoutTags(text: string): string {
  return text.replace(TAG, '$1').replace(/[ \t]{2,}/g, ' ').replace(/[ \t]+$/gm, '').trim()
}

/** A piece of text cut around its tags, for display. */
export function splitTags(text: string): ({ text: string } | { tag: string; text: string })[] {
  const out: ({ text: string } | { tag: string; text: string })[] = []
  let at = 0
  for (const m of text.matchAll(TAG)) {
    const start = m.index + m[1].length
    if (start > at) out.push({ text: text.slice(at, start) })
    const raw = `#${m[2].replace(/[/-]+$/, '')}`
    out.push({ tag: raw.slice(1).toLowerCase(), text: raw })
    at = start + raw.length
  }
  if (at < text.length) out.push({ text: text.slice(at) })
  return out
}

/** The tags in use, with how many notes carry each, by name. */
export function allTags(list: Note[] = notes): { tag: string; count: number }[] {
  const counts = new Map<string, number>()
  for (const n of list) for (const t of tagsOf(n.text)) counts.set(t, (counts.get(t) ?? 0) + 1)
  return [...counts].map(([tag, count]) => ({ tag, count })).sort((a, b) => a.tag.localeCompare(b.tag))
}

/** By first tag (untagged last), then by status. */
export const byTag = (a: Note, b: Note) => {
  const ta = tagsOf(a.text)[0]
  const tb = tagsOf(b.text)[0]
  if (ta !== tb) return ta === undefined ? 1 : tb === undefined ? -1 : ta.localeCompare(tb)
  return byStatus(a, b)
}

/** Order within a section: in progress, to do, plain notes. */
const RANK: Record<string, number> = { doing: 0, todo: 1, none: 2, done: 3 }
export const byStatus = (a: Note, b: Note) => RANK[a.status ?? 'none'] - RANK[b.status ?? 'none']

/** By text, tags left out (`#infra appeler X` sorts under A), numbers as numbers; then by status. */
const nameOf = (n: Note) => n.text.replace(TAG, '$1').trim()
export const byName = (a: Note, b: Note) => nameOf(a).localeCompare(nameOf(b), undefined, { sensitivity: 'base', numeric: true }) || byStatus(a, b)

/** Last changed first (its text, or when written). */
export const byDate = (a: Note, b: Note) => (b.updated_at || b.created_at).localeCompare(a.updated_at || a.created_at)

/** The orders the Notes block offers. */
export type NoteSort = 'status' | 'name' | 'date' | 'tag'
export const SORTS: Record<NoteSort, { label: string; compare: (a: Note, b: Note) => number; title: string }> = {
  status: { label: t('Status'), compare: byStatus, title: t('In progress, to do, then plain notes') },
  name: { label: t('Name'), compare: byName, title: t('By text, A to Z (tags left out)') },
  date: { label: t('Date'), compare: byDate, title: t('Last changed first') },
  tag: { label: t('Tag'), compare: byTag, title: t('By first tag, untagged last') },
}

export function updateNote(id: string, text: string) {
  const n = notes.find((x) => x.id === id)
  if (!n || n.text === text) return
  n.text = text
  n.updated_at = new Date().toISOString()
  save()
}

export function removeNote(id: string) {
  const i = notes.findIndex((x) => x.id === id)
  if (i >= 0) {
    notes.splice(i, 1)
    save()
  }
}

/** Reminder done, or reopened. */
export function setDone(id: string, done: boolean) {
  const n = notes.find((x) => x.id === id)
  if (!n) return
  n.done_at = done ? new Date().toISOString() : null
  save()
}

/** New due time: the reminder will be notified again. */
export function reschedule(id: string, at: Date) {
  const n = notes.find((x) => x.id === id)
  if (!n) return
  n.remind_at = at.toISOString()
  n.notified = false
  n.done_at = null
  n.updated_at = new Date().toISOString()
  save()
}

export const isPending = (n: Note) => n.kind === 'reminder' && !n.done_at && !!n.remind_at
export const isDue = (n: Note, now = Date.now()) => isPending(n) && new Date(n.remind_at!).getTime() <= now

/** Reminders to do, nearest to furthest (overdue ones first). */
export const pendingReminders = () => notes.filter(isPending).sort((a, b) => a.remind_at!.localeCompare(b.remind_at!))

/** Proposed due times: in 30 min, in 1 h, this afternoon, tomorrow morning, Monday morning. */
export function presets(now = new Date()): { label: string; at: Date }[] {
  const at = (d: Date, h: number, m = 0) => {
    const x = new Date(d)
    x.setHours(h, m, 0, 0)
    return x
  }
  const tomorrow = new Date(now)
  tomorrow.setDate(now.getDate() + 1)
  const monday = new Date(now)
  monday.setDate(now.getDate() + ((8 - now.getDay()) % 7 || 7))
  const list = [
    { label: t('in 30 min'), at: new Date(now.getTime() + 30 * 60_000) },
    { label: t('in 1 h'), at: new Date(now.getTime() + 60 * 60_000) },
    { label: '14:00', at: at(now, 14) },
    { label: t('tomorrow 9:00'), at: at(tomorrow, 9) },
    { label: t('Monday 9:00'), at: at(monday, 9) },
  ]
  return list.filter((p) => p.at.getTime() > now.getTime() + 60_000)
}

const CHECK_MS = 20_000

/**
 * macOS notification when each reminder is due, only once (even after a restart:
 * the state is kept with the note). Its own clock, as for meetings.
 */
export function startReminders() {
  if (!isDesktop()) return () => {}
  const check = async () => {
    const due = notes.filter((n) => isDue(n) && !n.notified)
    if (!due.length || !(await canNotify())) return
    for (const n of due) {
      n.notified = true
      const late = Date.now() - new Date(n.remind_at!).getTime() > 5 * 60_000
      notify({ title: late ? t('Reminder (missed)') : t('Reminder'), body: n.text })
    }
    save()
  }
  check()
  const timer = setInterval(check, CHECK_MS)
  return () => clearInterval(timer)
}
