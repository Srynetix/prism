// Prism's plugin for the OpenCode agents it launches (`OPENCODE_CONFIG_CONTENT` adds it to the
// user's plugins): the counterpart of the Claude Code hooks that call `prism hook`. It appends a
// line per lifecycle event to the agent's log, `$PRISM_EVENTS/$PRISM_AGENT.jsonl`, in the format of
// agents/hook.rs `Line`, and Prism follows the agent's status from it.
//
// Only metadata is kept (event, tool, what it touches, the context's token count and model):
// never the prompt, the response or a tool's output. Outside a Prism agent (no PRISM_AGENT), it does nothing.
//
// OpenCode loads every export of a plugin file as a plugin: this one exports nothing else.

import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'

export const PrismPlugin = async () => {
  const dir = process.env.PRISM_EVENTS
  const agent = process.env.PRISM_AGENT
  if (!dir || !agent || !/^[0-9a-f-]+$/i.test(agent)) return {}
  const file = join(dir, `${agent}.jsonl`)
  const log = (line: Record<string, unknown>) => {
    try {
      mkdirSync(dir, { recursive: true })
      // A single append per line: two events at once do not mix.
      appendFileSync(file, JSON.stringify({ t: Date.now(), ...line }) + '\n')
    } catch {
      // The log is Prism's business: never the agent's failure.
    }
  }

  // Subagents run in sessions of their own (with a parent): their turns are not the agent's.
  const children = new Set<string>()
  let busy = false

  return {
    event: async ({ event }: { event: { type: string; properties: any } }) => {
      const p = event.properties ?? {}
      const session: string | undefined = p.sessionID ?? p.info?.id
      switch (event.type) {
        case 'session.created':
          if (p.info?.parentID) {
            if (session) children.add(session)
          } else {
            // A new conversation (start, /new): the one to resume.
            busy = false
            log({ event: 'SessionStart', reason: 'startup', session })
          }
          break
        case 'session.status':
          if (session && children.has(session)) break
          if (p.status?.type === 'busy' && !busy) {
            busy = true
            log({ event: 'UserPromptSubmit' })
          } else if (p.status?.type === 'idle' && busy) {
            busy = false
            log({ event: 'Stop' })
          }
          break
        // Before 1.1: `permission.updated`, with a type and a pattern rather than a permission and patterns.
        case 'permission.asked':
        case 'permission.updated':
          log({ event: 'PermissionRequest', tool: p.permission ?? p.type, what: summarize(p.patterns?.[0] ?? p.pattern, p.permission ?? p.type) })
          break
        case 'permission.replied':
          log({ event: 'PermissionReplied' })
          break
        // The context, as OpenCode counts it: everything the last request sent (cache included) and
        // its answer, which the next one carries. The model's window is looked up by Prism.
        case 'message.updated': {
          const info = p.info
          if (info?.role !== 'assistant' || !info.time?.completed || (session && children.has(session))) break
          const t = info.tokens ?? {}
          const used = (t.input ?? 0) + (t.output ?? 0) + (t.reasoning ?? 0) + (t.cache?.read ?? 0) + (t.cache?.write ?? 0)
          if (used > 0) log({ event: 'Context', context: { used }, model: info.providerID && info.modelID ? `${info.providerID}/${info.modelID}` : undefined })
          break
        }
        case 'session.compacted':
          if (!session || !children.has(session)) log({ event: 'Compacted' })
          break
      }
    },
    'tool.execute.before': async ({ tool }: { tool: string }) => log({ event: 'PreToolUse', tool }),
    'tool.execute.after': async ({ tool }: { tool: string }) => log({ event: 'PostToolUse', tool }),
  }
}

/** What a permission touches, in a few words: its command, or the name of its file. */
function summarize(pattern: unknown, permission?: string): string | undefined {
  const text = Array.isArray(pattern) ? pattern[0] : pattern
  if (typeof text !== 'string' || !text.trim() || text === '*') return undefined
  // A command whole as Allow is decided on it: its start and end, its other lines counted.
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean)
  const first = lines[0] ?? ''
  const short = first.length <= 100 ? first : `${first.slice(0, 66)}…${first.slice(-33)}`
  return permission === 'bash' && lines.length > 1 ? `${short} (+${lines.length - 1} lines)` : short
}
