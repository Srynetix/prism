import { conventions } from './conventions'
import { t } from './i18n.svelte'
import { dayKey } from './filter.svelte'
import { celebrateFrom, fireworks } from './fun.svelte'
import { pref, setPref } from './layout.svelte'
import { live } from './live.svelte'
import { isDone } from './rules'
import { meOn } from './forge'
import { toast } from './toast.svelte'
import type { MergeRequest, Snapshot } from './types'
import { mrKey } from './mr-ref'

/**
 * Prism celebrations, only for my events and by comparing two snapshots (nothing at
 * launch): my MR merged (confetti from its line), my MR approved by someone (a toast naming them,
 * and its ticket), my MR's pipeline back to green after a failure (small green burst), the sprint
 * at 100 % (fireworks, once per sprint), a day's hours all logged (confetti from the time
 * chip, once per day).
 * `celebrate` does nothing without `[appearance] celebrations` or with "Reduce motion".
 */
const GREENS = ['#7ee0a8', '#c0dd97', '#3b6d11', '#a6e3a1']

/** Sprint progress, as in the Sprint block: share of High and Urgent tickets done. */
function sprintDone(s: Snapshot): boolean {
  const key = s.sprint?.tickets.filter((t) => t.priority === 'urgent' || t.priority === 'high') ?? []
  return key.length > 0 && key.every(isDone)
}

/** Time logged per day (local `YYYY-MM-DD`). */
function dayTotals(s: Snapshot): Map<string, number> {
  const totals = new Map<string, number>()
  for (const e of s.time.entries) {
    const k = dayKey(new Date(e.start))
    totals.set(k, (totals.get(k) ?? 0) + e.duration_ms)
  }
  return totals
}

const rowOf = (key: string) => document.querySelector(`[data-mr="${CSS.escape(key)}"]`)

export function watchCelebrations() {
  let prev: Snapshot | null = null
  return $effect.root(() => {
    $effect(() => {
      const s = live.snapshot
      if (!s?.me) return
      const before = prev
      prev = s
      if (!before?.me) return
      const mine = (m: MergeRequest) => m.author === meOn(s, m)
      const was = new Map(before.merge_requests.filter(mine).map((m) => [mrKey(m), m]))
      const approved: string[] = []
      const asked: string[] = []
      for (const m of s.merge_requests.filter(mine)) {
        const old = was.get(mrKey(m))
        if (!old) continue
        // Approved by someone new (not me, not one who already had): one line per MR, in one toast.
        const by = m.state === 'opened' ? m.approved_by.filter((u) => u !== meOn(s, m) && !old.approved_by.includes(u)) : []
        // Changes requested by someone new: as an approval, but to act on.
        const asking = m.state === 'opened' ? (m.changes_requested_by ?? []).filter((u) => u !== meOn(s, m) && !(old.changes_requested_by ?? []).includes(u)) : []
        if (asking.length) asked.push(t('{people} requested changes on {mr}', { people: asking.join(', '), mr: `${mrKey(m)}${m.ticket ? ` (${m.ticket})` : ''}` }))
        if (by.length) approved.push(`${t('{mr} approved by {people}', { mr: `${mrKey(m)}${m.ticket ? ` (${m.ticket})` : ''}`, people: by.join(', ') })}${m.approvals_left === 0 && old.approvals_left > 0 ? t(': fully approved') : ''}`)
        if (old.state === 'opened' && m.state === 'merged') {
          celebrateFrom(rowOf(mrKey(m)))
          toast(t('{mr} merged', { mr: mrKey(m) }))
        } else if (m.state === 'opened' && old.pipeline === 'failed' && m.pipeline === 'success') {
          celebrateFrom(rowOf(mrKey(m)), { count: 30, spread: Math.PI / 3, colors: GREENS })
        }
      }
      if (approved.length) toast(approved.join(' · '), 4000)
      if (asked.length) toast(asked.join(' · '), 6000)
      // A day's hours all logged (`[clickup] day_hours`): once per day, even after a restart.
      const target = conventions().day_hours * 3600_000
      const logged = dayTotals(before)
      for (const [day, ms] of dayTotals(s)) {
        if (target > 0 && ms >= target && (logged.get(day) ?? 0) < target && pref<string>('fun.dayFull', '') !== day) {
          setPref('fun.dayFull', day)
          celebrateFrom(document.querySelector('[data-time-chip]'), { count: 60 })
        }
      }
      // Sprint wrapped up: once per sprint, even after a restart.
      if (s.sprint && sprintDone(s) && !sprintDone(before) && pref<string>('fun.sprintDone', '') !== s.sprint.name) {
        setPref('fun.sprintDone', s.sprint.name)
        fireworks()
        toast(t('{sprint}: every high and urgent ticket is done', { sprint: s.sprint.name }))
      }
    })
  })
}
