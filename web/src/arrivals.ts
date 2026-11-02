/**
 * A row that arrives in a block (a new MR, a new alert…), marked `data-arrive` a moment for the
 * theme to show it (app.css): Synthwave's wipe, Hacker's typing and, in Alerts, its glitch. Only
 * with `[appearance] theme_motion` on a theme that has one. Not an arrival: what a block shows
 * when it opens, nor rows that come all at once (a filter, a page changed), nor what follows a
 * click or a key in the block.
 */
const THEMES = new Set(['synthwave', 'hacker'])
/** A block settles this long after it shows, or after the user acted in it. */
const SETTLE_MS = 2500
/** More rows than this in one go: the block shows something else, nothing arrived. */
const AT_ONCE = 3
const ARRIVAL_MS = 1500

export function watchArrivals(root: HTMLElement) {
  const html = document.documentElement
  const quiet = new WeakMap<Element, number>()
  const shown = (n: Element, now: number) => {
    for (const b of n.matches('[data-block]') ? [n] : n.querySelectorAll('[data-block]')) quiet.set(b, now)
  }
  const acted = (e: Event) => {
    const block = (e.target as Element | null)?.closest?.('[data-block]')
    if (block) quiet.set(block, performance.now())
  }
  const seen = new MutationObserver((records) => {
    const now = performance.now()
    // Blocks are followed whatever the theme: one chosen later sees its first arrival.
    const on = html.dataset.themeMotion === 'on' && THEMES.has(html.dataset.theme ?? '')
    const byBlock = new Map<Element, HTMLElement[]>()
    // A keyed list that reorders moves its rows: taken out and put back, they did not arrive.
    const moved = new Set(records.flatMap((r) => [...r.removedNodes]))
    for (const r of records)
      for (const n of r.addedNodes) {
        if (!(n instanceof HTMLElement) || !n.parentElement || moved.has(n)) continue
        const block = n.closest('[data-block]')
        // Blocks are never inside one another: only what lands outside a block can bring one.
        if (!block || block === n) {
          shown(n, now)
          continue
        }
        // A row among its likes: a sibling of the same kind already there.
        if (!on || !n.className || !sameKind(n)) continue
        byBlock.set(block, [...(byBlock.get(block) ?? []), n])
      }
    for (const [block, rows] of byBlock) {
      if (now - (quiet.get(block) ?? 0) < SETTLE_MS || rows.length > AT_ONCE) continue
      for (const row of rows) {
        row.dataset.arrive = ''
        setTimeout(() => delete row.dataset.arrive, ARRIVAL_MS)
      }
    }
  })
  shown(root, performance.now())
  seen.observe(root, { childList: true, subtree: true })
  root.addEventListener('pointerdown', acted, true)
  root.addEventListener('keydown', acted, true)
  return () => {
    seen.disconnect()
    root.removeEventListener('pointerdown', acted, true)
    root.removeEventListener('keydown', acted, true)
  }
}

function sameKind(n: HTMLElement) {
  const kind = n.classList[0]
  for (const s of n.parentElement!.children) if (s !== n && s.tagName === n.tagName && s.classList[0] === kind) return true
  return false
}
