import type { Snapshot } from '../../types'
import { dayKey } from '../../filter.svelte'
import { meOn } from '../../forge'
import { isProtected } from '../../conventions'
import type { TrailEvent, TrailState } from '../trail/trail'
import { t, tn } from '../../i18n.svelte'

/**
 * What the Pet becomes with your work (`[pet] forms`, `trophies`, `diary`), from the Day trail's
 * journal and the snapshot. Its memory (`Growth`) lives with the Pet's, in the layout's prefs.
 */

export type Face = 'happy' | 'sad' | 'sick' | 'sleep' | 'wow' | 'party' | 'tired'
export type Stage = 'baby' | 'young' | 'grown-up' | 'legend'
export type Form = 'reviewer' | 'shipper' | 'fixer' | 'night' | 'architect'

export const FORMS: Record<Form, { label: string; why: string }> = {
  reviewer: { label: t('The Reviewer'), why: t('most reviews and comments') },
  shipper: { label: t('The Shipper'), why: t('most merged MRs') },
  fixer: { label: t('The Fixer'), why: t('most fixes and hotfixes merged') },
  night: { label: t('The Night Owl'), why: t('much of the work after 20:00') },
  architect: { label: t('The Architect'), why: t('most refinement and architecture work') },
}

export interface Page {
  /** The week's Monday (`YYYY-MM-DD`). */
  week: string
  lines: string[]
  stats: string
  sig: string
}

export interface Growth {
  form: Form | null
  /** When the form was last chosen: at most once a week. */
  formAt: number
  /** The trail's events counted up to this time. */
  upto: number
  counts: { merges: number; reviews: number; tiny: number; late: number; early: number }
  /** My open MRs' size (lines), by `repo!iid`: a merged one was tiny or not. */
  sizes: Record<string, number>
  /** The protected branches green day after day. */
  green: { day: string; red: boolean; streak: number }
  /** Sprints finished with all my tickets done. */
  sprints: string[]
  /** Trophies unlocked, and when. */
  trophies: Record<string, number>
  diary: Page[]
  /** Its photos (`[pet] album`), the oldest first; and the stage and form last photographed. */
  album: Shot[]
  shot: { stage: Stage | null; form: Form | null }
}

export const NEW_GROWTH: Growth = {
  form: null,
  formAt: 0,
  upto: 0,
  counts: { merges: 0, reviews: 0, tiny: 0, late: 0, early: 0 },
  sizes: {},
  green: { day: '', red: false, streak: 0 },
  sprints: [],
  trophies: {},
  diary: [],
  album: [],
  shot: { stage: null, form: null },
}

const DAY = 86_400_000
const isReview = (e: TrailEvent) => (e.kind === 'mr_commented' || e.kind === 'mr_approved') && e.others !== false
const isMyMerge = (e: TrailEvent) => e.kind === 'mr_merged' && e.others !== true
const isWork = (e: TrailEvent) => e.kind === 'commit' || e.kind.startsWith('mr_')
const hour = (e: TrailEvent) => new Date(e.at).getHours()
const FIX = /\b(hot)?fix(es|ed)?\b|\bbug\b|\bblank\b|\bbroken\b/i
const DESIGN = /\badr\b|architect|refine|design|spec/i

/* ---------- Forms ---------- */

/** What you did most over the last 14 days, each kind weighted. */
export function scores(trail: TrailState | undefined, s: Snapshot, now = Date.now()): Record<Form, number> {
  const recent = (trail?.events ?? []).filter((e) => now - Date.parse(e.at) < 14 * DAY)
  const work = recent.filter(isWork)
  const late = work.filter((e) => hour(e) >= 20 || hour(e) < 6).length
  const agents = (s.agents ?? []).filter((a) => now - Date.parse(a.created_at) < 14 * DAY)
  return {
    reviewer: recent.filter(isReview).length,
    shipper: recent.filter(isMyMerge).length * 1.5,
    // Fixes merged, and commits on hotfix branches: not every `fix:` commit makes a fixer.
    fixer: recent.filter((e) => isMyMerge(e) && FIX.test(e.title)).length * 2 + recent.filter((e) => e.kind === 'commit' && /^hotfix\//.test(e.branch ?? '')).length * 0.5,
    night: work.length >= 10 && late / work.length >= 0.25 ? late : 0,
    architect: (recent.filter((e) => DESIGN.test(e.title)).length + agents.filter((a) => DESIGN.test(`${a.category ?? ''} ${a.title}`)).length) * 2,
  }
}

/** The form that wins, if anything stands out (at least 4 points). */
export function pickForm(score: Record<Form, number>): Form | null {
  const [best, n] = (Object.entries(score) as [Form, number][]).sort((a, b) => b[1] - a[1])[0]
  return n >= 4 ? best : null
}

/* ---------- Counting ---------- */

/** Adds the trail's new events to the counters, and keeps my open MRs' size. */
export function count(g: Growth, trail: TrailState | undefined, s: Snapshot) {
  for (const m of s.merge_requests) if (m.author === meOn(s, m) && m.state === 'opened' && m.size) g.sizes[`${m.repo ?? ''}!${m.iid}`] = m.size.additions + m.size.deletions
  let upto = g.upto
  for (const e of trail?.events ?? []) {
    const at = Date.parse(e.at)
    if (at <= g.upto) continue
    upto = Math.max(upto, at)
    if (isReview(e)) g.counts.reviews++
    if (isMyMerge(e)) {
      g.counts.merges++
      const primary = s.repositories?.find((r) => r.primary)?.name
      const lines = g.sizes[`${e.repo && e.repo !== primary ? e.repo : ''}!${e.mr}`]
      if (lines !== undefined && lines < 50) g.counts.tiny++
    }
    if (isWork(e) && hour(e) >= 22) g.counts.late++
    if (e.kind === 'commit' && hour(e) < 7) g.counts.early++
  }
  g.upto = upto
  // Sizes of MRs long merged are of no use.
  const open = new Set(s.merge_requests.filter((m) => m.state === 'opened').map((m) => `${m.repo ?? ''}!${m.iid}`))
  const keys = Object.keys(g.sizes)
  if (keys.length > 60) for (const k of keys) if (!open.has(k)) delete g.sizes[k]
}

/** Each working day without a red protected pipeline makes the streak longer; a red one ends it. */
export function trackGreen(g: Growth, s: Snapshot, now = Date.now()) {
  const today = dayKey(new Date(now))
  const red = s.pipelines.some((p) => p.status === 'failed' && isProtected(p.target))
  if (g.green.day !== today) {
    const weekend = [0, 6].includes(new Date(`${g.green.day}T12:00`).getDay())
    if (g.green.day && !weekend) g.green.streak = g.green.red ? 0 : g.green.streak + 1
    g.green = { day: today, red, streak: g.green.streak }
  } else if (red) g.green.red = true
}

/** The sprint ends today (or ended) with all my tickets done: once per sprint. */
export function trackSprint(g: Growth, s: Snapshot, now = Date.now()) {
  const sp = s.sprint
  const me = s.tracker_me?.id
  if (!sp?.end || me === undefined || g.sprints.includes(sp.name)) return
  if (dayKey(new Date(sp.end)) > dayKey(new Date(now))) return
  const mine = sp.tickets.filter((tk) => tk.assignees.some((a) => a.id === me))
  if (mine.length && mine.every((tk) => tk.closed)) g.sprints = [...g.sprints, sp.name].slice(-20)
}

/* ---------- Trophies ---------- */

export interface Trophy {
  id: string
  icon: string
  label: string
  what: string
  /** Where you are, out of what it takes. */
  progress: (g: Growth, level: number, days: number) => [number, number]
}

export const TROPHIES: Trophy[] = [
  { id: 'merges', icon: '🪴', label: t('First ten merges'), what: t('Ten of your MRs merged.'), progress: (g) => [g.counts.merges, 10] },
  { id: 'green', icon: '☀️', label: t('Green week'), what: t('Five working days in a row without a red protected pipeline.'), progress: (g) => [g.green.streak, 5] },
  { id: 'sprint', icon: '🏆', label: t('Sprint finisher'), what: t('All your sprint tickets done by its last day.'), progress: (g) => [g.sprints.length, 1] },
  { id: 'bookworm', icon: '📚', label: t('Bookworm'), what: t('Fifty reviews and comments on others’ MRs.'), progress: (g) => [g.counts.reviews, 50] },
  { id: 'tiny', icon: '🚀', label: t('Tiny diffs'), what: t('Ten MRs merged under 50 lines.'), progress: (g) => [g.counts.tiny, 10] },
  { id: 'month', icon: '🎾', label: t('A month together'), what: t('Thirty days since it hatched.'), progress: (_g, _l, days) => [days, 30] },
  { id: 'owl', icon: '🦉', label: t('Night shift'), what: t('A commit or a merge after 22:00.'), progress: (g) => [g.counts.late, 1] },
  { id: 'bird', icon: '🐦', label: t('Early bird'), what: t('A commit before 7:00.'), progress: (g) => [g.counts.early, 1] },
  { id: 'legend', icon: '👑', label: t('Legend'), what: t('Level 10.'), progress: (_g, level) => [level, 10] },
]

/** The trophies just reached: marked unlocked. */
export function unlock(g: Growth, level: number, days: number, now = Date.now()): Trophy[] {
  const fresh = TROPHIES.filter((tr) => !g.trophies[tr.id] && (([n, of]) => n >= of)(tr.progress(g, level, days)))
  for (const tr of fresh) g.trophies[tr.id] = now
  return fresh
}

/* ---------- Diary ---------- */

/** The Monday (`YYYY-MM-DD`) of the week of `at`. */
export function weekOf(at: number): string {
  const d = new Date(at)
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7))
  return dayKey(d)
}

const DAYS = [t('Monday'), t('Tuesday'), t('Wednesday'), t('Thursday'), t('Friday')]
/** A variant of `list`, the same for the same day: the diary does not change on a re-read. */
const pick = (list: string[], seed: string) => list[[...seed].reduce((n, c) => (n * 31 + c.charCodeAt(0)) >>> 0, 7) % list.length]

/** The weeks whose page is due: this one from Friday 15:00, the last one if it was missed. */
export function dueWeeks(g: Growth, now = Date.now()): string[] {
  const d = new Date(now)
  const thisWeek = weekOf(now)
  const lastWeek = weekOf(now - 7 * DAY)
  const friday = (d.getDay() === 5 && d.getHours() >= 15) || d.getDay() === 6 || d.getDay() === 0
  return [lastWeek, ...(friday ? [thisWeek] : [])].filter((w) => !g.diary.some((p) => p.week === w))
}

/** Its page on the week of Monday `week`, from the trail; `null` with too little to tell. */
export function writePage(week: string, trail: TrailState | undefined, g: Growth, sig: string): Page | null {
  const start = Date.parse(`${week}T00:00`)
  const events = (trail?.events ?? []).filter((e) => {
    const at = Date.parse(e.at)
    return at >= start && at < start + 7 * DAY
  })
  if (events.length < 3) return null
  const lines: string[] = []
  DAYS.forEach((name, i) => {
    const day = dayKey(new Date(start + i * DAY + 12 * 3600_000))
    const of = events.filter((e) => dayKey(new Date(e.at)) === day)
    const merges = of.filter(isMyMerge)
    const commits = of.filter((e) => e.kind === 'commit')
    const reviews = of.filter(isReview)
    const meetings = of.filter((e) => e.kind === 'meeting')
    const ticket = commits.find((e) => e.ticket)?.ticket
    const said: string[] = []
    const cookies = '🍪'.repeat(Math.min(3, merges.length))
    const mrs = merges.map((m) => `!${m.mr}`).join(t(' and '))
    const where = ticket ?? t('something tricky')
    if (merges.length) said.push(pick([merges.length > 1 ? t('{n} merges! I ate them all. {cookies}', { n: merges.length, cookies }) : t('A merge! I ate it. {cookies}', { cookies }), t('we merged {mrs}. Delicious.', { mrs }), tn(merges.length, '{n} MR shipped. I did a little dance.', '{n} MRs shipped. I did a little dance.')], day))
    else if (commits.length) said.push(pick([tn(commits.length, 'we worked on {where}: {n} commit. I helped by sitting on the keyboard.', 'we worked on {where}: {n} commits. I helped by sitting on the keyboard.', { where }), ticket ? tn(commits.length, '{n} commit on {ticket}. Busy paws.', '{n} commits on {ticket}. Busy paws.', { ticket }) : tn(commits.length, '{n} commit. Busy paws.', '{n} commits. Busy paws.'), t('we were deep in {where} all day.', { where: ticket ?? t('the code') })], day))
    if (reviews.length) said.push(pick([tn(reviews.length, 'we read {mr} very carefully: {n} comment.', 'we read {mr} very carefully: {n} comments.', { mr: reviews[0].mr ? `!${reviews[0].mr}` : t('a colleague’s MR') }), tn(reviews.length, 'review time: {n} comment for the team.', 'review time: {n} comments for the team.'), tn(reviews.length, 'we helped the others: {n} review.', 'we helped the others: {n} reviews.')], `${day}r`))
    if (meetings.length >= 2) said.push(pick([t('lots of meetings. I napped.'), t('{n} meetings. I counted the slides.', { n: meetings.length }), t('meetings, meetings. I guarded the keyboard.')], `${day}m`))
    if (!said.length) said.push(pick([t('a quiet day. I chased the cursor.'), t('not much happened. I watched the CI.'), t('we thought a lot. Thinking counts too.')], `${day}q`))
    lines.push(t('{day}: {said}', { day: name, said: said.join(t(' And ')) }))
  })
  for (const tr of TROPHIES.filter((tr) => g.trophies[tr.id] >= start && g.trophies[tr.id] < start + 7 * DAY)) lines.push(t('A new trophy: {label}! {icon}', { label: tr.label, icon: tr.icon }))
  const n = (f: (e: TrailEvent) => boolean) => events.filter(f).length
  const stats = [tn(n(isMyMerge), '{n} merge', '{n} merges'), tn(n(isReview), '{n} review', '{n} reviews'), tn(n((e) => e.kind === 'commit'), '{n} commit', '{n} commits'), tn(n((e) => e.kind === 'meeting'), '{n} meeting', '{n} meetings')].join(' · ')
  return { week, lines, stats, sig }
}

/* ---------- Album ---------- */

/** A photo: how it looked at a milestone, and its caption. */
export interface Shot {
  at: number
  caption: string
  stage: Stage
  form: Form | null
  /** The trophy it shows off, if any. */
  icon?: string
}

const STAGE_CAPTION: Record<Stage, string> = { baby: t('Hatched!'), young: t('Young now'), 'grown-up': t('All grown up'), legend: t('A legend') }

/**
 * The photos of the moments just reached: its hatching (the first time, at its birth), each stage,
 * each new form, each trophy. Taken once; the last 24 are kept.
 */
export function takeShots(g: Growth, look: { stage: Stage; form: Form | null }, born: number, won: Trophy[], now = Date.now()): Shot[] {
  const fresh: Shot[] = []
  if (!g.album.length && !g.shot.stage) {
    fresh.push({ at: born, caption: STAGE_CAPTION.baby, stage: 'baby', form: null })
    g.shot.stage = 'baby'
  }
  if (look.stage !== g.shot.stage) {
    fresh.push({ at: now, caption: STAGE_CAPTION[look.stage], stage: look.stage, form: look.form })
    g.shot.stage = look.stage
  }
  if (look.form && look.form !== g.shot.form) fresh.push({ at: now, caption: FORMS[look.form].label, stage: look.stage, form: look.form })
  g.shot.form = look.form ?? g.shot.form
  for (const tr of won) fresh.push({ at: now, caption: tr.label, stage: look.stage, form: look.form, icon: tr.icon })
  if (fresh.length) g.album = [...g.album, ...fresh].sort((a, b) => a.at - b.at).slice(-24)
  return fresh
}
