/**
 * Quick open's matching of a path: the characters typed, in order, in the file's name first
 * (`chkts` finds `checkout.ts`), else across its folders too; scored higher when they follow
 * each other and start a part of the name (after `/`, `.`, `-`, `_`, or a capital). A space
 * separates parts matched in any order (`cart test`). `null`: no match. `at`: matched positions
 * in the path, to mark them.
 */
export interface PathMatch {
  score: number
  at: number[]
}

const START = /[/._\-\s]/

function starts(text: string, i: number) {
  return i === 0 || START.test(text[i - 1]) || (text[i] !== text[i].toLowerCase() && text[i - 1] === text[i - 1].toLowerCase())
}

/**
 * `word` in `text` from `from`, its characters in order, placed where they score best (runs of
 * them, starts of parts): every placement weighed, not the first one found.
 */
/** `along`'s tables, kept from one path to the next: 20,000 paths were 200,000 arrays a key. */
let bestBuf = new Float64Array(1024)
let prevBuf = new Int32Array(1024)

function along(text: string, lower: string, word: string, from: number): PathMatch | null {
  const n = lower.length
  const m = word.length
  if (bestBuf.length < m * n) {
    bestBuf = new Float64Array(m * n * 2)
    prevBuf = new Int32Array(m * n * 2)
  }
  // best[i * n + j]: the best score with character i of the word at j; prev: where i - 1 was.
  const best = bestBuf
  const prev = prevBuf
  best.fill(-1, 0, m * n)
  prev.fill(-1, 0, m * n)
  for (let i = 0; i < m; i++) {
    const row = i * n
    const above = row - n
    // The best of the row above before j - 1, as j goes.
    let run = -1
    let runAt = -1
    for (let j = from; j < n; j++) {
      if (i > 0 && j >= 2 && best[above + j - 2] > run) {
        run = best[above + j - 2]
        runAt = j - 2
      }
      if (lower[j] !== word[i]) continue
      const own = 1 + (starts(text, j) ? 3 : 0)
      if (i === 0) {
        best[row + j] = own
        continue
      }
      const next = j > 0 && best[above + j - 1] >= 0 ? best[above + j - 1] + own + 4 : -1
      const apart = run >= 0 ? run + own : -1
      if (next < 0 && apart < 0) continue
      if (next >= apart) {
        best[row + j] = next
        prev[row + j] = j - 1
      } else {
        best[row + j] = apart
        prev[row + j] = runAt
      }
    }
  }
  const last = (m - 1) * n
  let end = -1
  for (let j = from; j < n; j++) if (best[last + j] >= 0 && (end < 0 || best[last + j] > best[last + end])) end = j
  if (end < 0) return null
  const score = best[last + end]
  // Its characters too scattered (one here, one there across the path): no match.
  if (m > 2 && score < m * 2.5) return null
  const at: number[] = []
  for (let i = m - 1, j = end; i >= 0; j = prev[i * n + j], i--) at.unshift(j)
  return { score, at }
}

/** `word`'s characters in `lower`, in order: what `along` needs to find anything, checked in one pass. */
function inOrder(lower: string, word: string) {
  let j = 0
  for (let i = 0; i < word.length; i++) {
    j = lower.indexOf(word[i], j) + 1
    if (!j) return false
  }
  return true
}

export function matchPath(path: string, query: string): PathMatch | null {
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean)
  if (!words.length) return { score: 0, at: [] }
  const lower = path.toLowerCase()
  // Most paths do not hold the letters at all: refused before weighing every placement (20,000
  // paths: 41 → 3 ms a key for most queries).
  if (!words.every((w) => inOrder(lower, w))) return null
  const nameAt = path.lastIndexOf('/') + 1
  let score = 0
  const at: number[] = []
  for (const w of words) {
    // In the name: worth more; else from the start of the path.
    const inName = along(path, lower, w, nameAt)
    const m = inName ?? along(path, lower, w, 0)
    if (!m) return null
    score += m.score + (inName ? 10 + (m.at[0] === nameAt ? 8 : 0) : 0)
    at.push(...m.at)
  }
  // The shorter path first for the same match.
  return { score: score - path.length / 50, at: [...new Set(at)].sort((a, b) => a - b) }
}

/**
 * `paths` matching `query`, best first, at most `max`; `boost` adds to a path's score (open and
 * recent files, changed ones). An empty query: the boosted ones first, in their order.
 */
export function rankPaths(paths: string[], query: string, boost: (path: string) => number, max = 50): { path: string; at: number[] }[] {
  const out: { path: string; at: number[]; score: number }[] = []
  for (const path of paths) {
    const m = matchPath(path, query)
    if (m) out.push({ path, at: m.at, score: m.score + boost(path) })
  }
  out.sort((a, b) => b.score - a.score || a.path.localeCompare(b.path))
  return out.slice(0, max)
}

/** What quick open's field asks for: a file, a line (`:42`), a symbol (`@name`), text (`#words`). */
export type Ask = { kind: 'file'; query: string; line: number | null } | { kind: 'line'; line: number } | { kind: 'symbol'; query: string } | { kind: 'text'; query: string }

export function parseAsk(input: string): Ask {
  const s = input.trimStart()
  if (s.startsWith(':')) return { kind: 'line', line: Number.parseInt(s.slice(1), 10) || 0 }
  if (s.startsWith('@')) return { kind: 'symbol', query: s.slice(1).trim() }
  if (s.startsWith('#')) return { kind: 'text', query: s.slice(1).trim() }
  // `checkout.ts:42`: the file, at that line.
  const m = /^(.*?):(\d+)$/.exec(s)
  return m ? { kind: 'file', query: m[1], line: Number(m[2]) } : { kind: 'file', query: s, line: null }
}
