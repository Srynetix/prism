import { hasCore, isDesktop } from './bridge'
import { read, write } from './persist'
import { dayOf, pruned, type UsageDoc } from './cleanup'

/**
 * How much each feature is used, counted on this machine only (the profile's usage.json, through
 * the app's store; never sent anywhere): `used(id)` at a few chokepoints — a page shown counts
 * its blocks (`block:<id>`), a palette entry run (`palette:<kind>`), an agent action launched
 * (`action:<id>`), the Agents view, a quick answer, a thread, a page swipe. One count per day and
 * id, the days beyond `KEEP_DAYS` dropped; the writes are batched by the store. Settings › Cleanup
 * reads them (cleanup.ts).
 */
const state = $state<{ doc: UsageDoc | null }>({ doc: null })

const fresh = (): UsageDoc => ({ since: dayOf(new Date()), days: {}, kept: [] })

/** The counts as saved: read once from the store, a fresh document the first time. */
function loaded(): UsageDoc {
  const saved = read<Partial<UsageDoc>>('usage')
  return saved?.since ? { since: saved.since, days: saved.days ?? {}, kept: saved.kept ?? [] } : fresh()
}

/** The counts, for Settings › Cleanup. */
export const usageDoc = (): UsageDoc => state.doc ?? loaded()

function save(doc: UsageDoc) {
  state.doc = doc
  write('usage', doc)
}

/** One more use of `id` today. A phone's page counts nothing: its computer's window does. */
export function used(id: string) {
  if (hasCore() && !isDesktop()) return
  const doc = state.doc ?? loaded()
  const day = dayOf(new Date())
  const today = { ...doc.days[day], [id]: (doc.days[day]?.[id] ?? 0) + 1 }
  save({ ...doc, days: pruned({ ...doc.days, [day]: today }, day) })
}

/** The blocks of a page just shown: each counts once. */
export const pageShown = (blocks: string[]) => [...new Set(blocks)].forEach((b) => used(`block:${b}`))

/** "Keep" on a line of Settings › Cleanup: never listed again as unused. */
export function keepFeature(id: string) {
  const doc = usageDoc()
  if (!doc.kept.includes(id)) save({ ...doc, kept: [...doc.kept, id] })
}
