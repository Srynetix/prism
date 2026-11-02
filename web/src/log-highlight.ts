/**
 * Syntax highlighting of an application log line (pod logs): what does not already have its
 * ANSI colors. A single expression, which recognizes in order: timestamp, level, HTTP method
 * and status code, URL or path, quoted string (JSON key apart), `key=value`, UUID,
 * IP address, number, and JSON literals. The rest keeps the text color. A quoted
 * request (`"GET /api HTTP/1.1"`) is highlighted inside; in JSON, the value of
 * `level` takes the level's color, and a string that is a URL, a URL's.
 */
export type Tok = 'time' | 'err' | 'warn' | 'info' | 'debug' | 'method' | 'ok' | 'redir' | 'cerr' | 'serr' | 'url' | 'key' | 'str' | 'id' | 'num' | 'lit'

export interface Piece {
  text: string
  tok?: Tok
}

const RULES: [Tok | 'status' | 'level' | 'kv', string][] = [
  ['time', String.raw`\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:[.,]\d+)?(?:Z|[+-]\d{2}:?\d{2})?|\b\d{2}:\d{2}:\d{2}(?:[.,]\d+)?\b`],
  ['level', String.raw`\b(?:FATAL|CRITICAL|ERROR|ERR|WARNING|WARN|INFO|DEBUG|TRACE|fatal|critical|error|warning|warn|info|debug|trace)\b`],
  ['method', String.raw`\b(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)\b`],
  ['url', String.raw`\bhttps?:\/\/[^\s"'<>]+|(?<![\w.])\/(?:[\w.~%-]+\/?)+(?:\?[^\s"']*)?`],
  ['key', String.raw`"[\w.@$-]+"(?=\s*:)`],
  ['str', String.raw`"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'`],
  ['kv', String.raw`\b[\w.-]+(?==[^=\s])`],
  ['id', String.raw`\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b|\b(?:\d{1,3}\.){3}\d{1,3}(?::\d+)?\b`],
  ['status', String.raw`(?<=[\s"'(=:])[1-5]\d{2}(?=[\s"',)]|$)`],
  ['num', String.raw`\b\d+(?:\.\d+)?(?:ms|s|µs|us|ns|B|KB|MB|GB|%)?\b`],
  ['lit', String.raw`\b(?:true|false|null|None|True|False)\b`],
]
const PATTERN = new RegExp(RULES.map(([, re]) => `(${re})`).join('|'), 'g')

function level(word: string): Tok {
  const w = word.toLowerCase()
  if (['fatal', 'critical', 'error', 'err'].includes(w)) return 'err'
  if (w.startsWith('warn')) return 'warn'
  if (w === 'info') return 'info'
  return 'debug'
}

const status = (code: string): Tok => (code[0] === '2' ? 'ok' : code[0] === '3' ? 'redir' : code[0] === '4' ? 'cerr' : code[0] === '5' ? 'serr' : 'num')

const REQUEST = /^"(?:GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS) /
const LEVEL_KEY = /^"(?:level|severity|levelname|lvl)"$/i
const LEVEL_WORD = new RegExp(`^"(?:${RULES[1][1]})"$`)

export function highlight(text: string): Piece[] {
  const out: Piece[] = []
  let last = 0
  let key = ''
  for (const m of text.matchAll(PATTERN)) {
    const i = m.index!
    if (i > last) out.push({ text: text.slice(last, i) })
    last = i + m[0].length
    const rule = RULES[m.slice(1).findIndex((g) => g !== undefined)][0]
    if (rule === 'str' && REQUEST.test(m[0])) {
      out.push({ text: '"', tok: 'str' }, ...highlight(m[0].slice(1, -1)), { text: '"', tok: 'str' })
      continue
    }
    let tok: Tok = rule === 'level' ? level(m[0]) : rule === 'status' ? status(m[0]) : rule === 'kv' ? 'key' : rule
    if (tok === 'str' && LEVEL_KEY.test(key) && LEVEL_WORD.test(m[0])) tok = level(m[0].slice(1, -1))
    else if (tok === 'str' && /^"https?:\/\//.test(m[0])) tok = 'url'
    key = rule === 'key' ? m[0] : ''
    out.push({ text: m[0], tok })
  }
  if (last < text.length) out.push({ text: text.slice(last) })
  return out
}
