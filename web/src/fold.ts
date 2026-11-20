/**
 * What a squeezed bar moves into its ⋯ (the status bar's items, a `Toolbar`'s tools): the lowest
 * `rank` first, and of two alike the one further right, until the rest fits `room` px; then those
 * that fit again in the room left come back. `widths`: each one's last width in the bar (0, or
 * not known yet: it says nothing, it stays). `more`: the ⋯'s own width, counted as soon as one goes.
 */
export function foldOut<T>(shown: T[], widths: Map<T, number>, room: number, more: number, rank: (id: T) => number = () => 0): T[] {
  let need = shown.reduce((n, id) => n + (widths.get(id) ?? 0), 0)
  if (need <= room) return []
  need += more
  const out: T[] = []
  const candidates = shown.filter((id) => (widths.get(id) ?? 0) > 0 && rank(id) < Infinity).sort((a, b) => rank(a) - rank(b) || shown.indexOf(b) - shown.indexOf(a))
  for (const id of candidates) {
    if (need <= room) break
    out.push(id)
    need -= widths.get(id)!
  }
  // A wide one gone may leave room for a narrower one taken before it: back in the bar.
  for (const id of [...out].reverse()) {
    if (need + widths.get(id)! > room) continue
    out.splice(out.indexOf(id), 1)
    need += widths.get(id)!
  }
  return out
}
