/**
 * The palette's matching: every character typed, in order, in the text (`kmr` finds “Kubernetes
 * MR”), scored higher when they follow each other and start words; a typed space separates words
 * that may come in any order. `null`: no match. `at`: the matched positions, to mark them.
 */
export interface Match {
  score: number
  at: number[]
}

const WORD_START = /[\s/!#›:._-]/

function one(text: string, word: string, from = 0): Match | null {
  let i = from
  let score = 0
  let last = -2
  const at: number[] = []
  for (const c of word) {
    const j = text.indexOf(c, i)
    if (j < 0) return null
    score += (j === last + 1 ? 3 : 1) + (j === 0 || WORD_START.test(text[j - 1]) ? 2 : 0)
    at.push(j)
    last = j
    i = j + 1
  }
  // The whole word as is: better still (an id, a number).
  if (text.includes(word)) score += word.length
  return { score, at }
}

export function fuzzy(text: string, query: string): Match | null {
  const t = text.toLowerCase()
  const words = query.toLowerCase().trim().split(/\s+/).filter(Boolean)
  if (!words.length) return { score: 0, at: [] }
  let score = 0
  const at: number[] = []
  for (const w of words) {
    // Each word on its own, best where it reads as is.
    const exact = t.indexOf(w)
    const m = exact >= 0 ? one(t, w, exact) : one(t, w)
    // Its characters too scattered (“tooltip” across “The BigQuery cost block”): no match.
    if (!m || m.score < w.length * 1.5) return null
    score += m.score
    at.push(...m.at)
  }
  // Shorter texts first for the same match: “CI” before “CI health”.
  return { score: score - t.length / 100, at: [...new Set(at)].sort((a, b) => a - b) }
}
