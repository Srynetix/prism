/**
 * Lines of a file handed to its agent (Ask agent, Add to prompt, a note): where they are, then
 * them as a fenced block, in the prompt language (`[agents] prompt_language`): the agent reads it
 * as it would a passage quoted in Chat.
 */
export interface Passage {
  path: string
  from: number
  to: number
  text: string
}

/** `src/a.ts:12` or `src/a.ts:12-15`. */
export const refOf = (p: Passage) => `${p.path}:${p.from}${p.to > p.from ? `-${p.to}` : ''}`

/** A fence the passage cannot close: longer than any run of backticks in it. */
function fence(text: string) {
  const longest = Math.max(2, ...[...text.matchAll(/`+/g)].map((m) => m[0].length))
  return '`'.repeat(longest + 1)
}

export function quote(p: Passage, lang: string): string {
  const where = lang === 'fr'
    ? `Dans \`${p.path}\`, ${p.to > p.from ? `lignes ${p.from}–${p.to}` : `ligne ${p.from}`} :`
    : `In \`${p.path}\`, ${p.to > p.from ? `lines ${p.from}–${p.to}` : `line ${p.from}`}:`
  const ext = /\.([a-z0-9]+)$/i.exec(p.path)?.[1]?.toLowerCase() ?? ''
  const f = fence(p.text)
  return `${where}\n\n${f}${ext}\n${p.text.replace(/\n$/, '')}\n${f}`
}

/** The passage, then what is asked about it. */
export const ask = (p: Passage, question: string, lang: string) => `${quote(p, lang)}\n\n${question.trim()}`
