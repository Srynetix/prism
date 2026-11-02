// Prism's mod for the Command Code agents it launches (`cmd --mod <this file>`): the counterpart
// of the Claude Code hooks that call `prism hook`. It appends a line per lifecycle event to the
// agent's log, `$PRISM_EVENTS/$PRISM_AGENT.jsonl`, in the format of agents/hook.rs `Line`, and
// Prism follows the agent's status from it.
//
// Only metadata is kept (event, tool, what it touches, the context's token count and model):
// never the prompt, the response or a tool's output. Outside a Prism agent (no PRISM_AGENT), it does nothing.

import { appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'

/** Command Code has no permission event: a tool still queued after this delay is waiting for one. */
const ASK_AFTER_MS = 1500

export default function (cmd: any): void {
  const dir = process.env.PRISM_EVENTS
  const agent = process.env.PRISM_AGENT
  if (!dir || !agent || !/^[0-9a-f-]+$/i.test(agent)) return
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

  const pending = new Map<string, ReturnType<typeof setTimeout>>()
  const settle = (id?: string) => {
    for (const [key, timer] of pending) {
      if (id === undefined || key === id) {
        clearTimeout(timer)
        pending.delete(key)
      }
    }
  }

  cmd.hooks({
    onSessionStart: ({ source, sessionId }: { source?: string; sessionId?: string }) => log({ event: 'SessionStart', reason: source, session: sessionId }),
    onSessionEnd: ({ reason }: { reason?: string }) => log({ event: 'SessionEnd', reason }),
  })
  cmd.on('run_start', () => log({ event: 'UserPromptSubmit' }))
  cmd.on('tool_queued', ({ toolCallId, toolName, input }: { toolCallId: string; toolName: string; input: unknown }) => {
    const what = summarize(input)
    const timer = setTimeout(() => {
      pending.delete(toolCallId)
      log({ event: 'PermissionRequest', tool: toolName, what })
    }, ASK_AFTER_MS)
    timer.unref?.()
    pending.set(toolCallId, timer)
  })
  cmd.on('tool_running', ({ toolCallId, toolName }: { toolCallId: string; toolName: string }) => {
    settle(toolCallId)
    log({ event: 'PreToolUse', tool: toolName })
  })
  for (const event of ['tool_completed', 'tool_errored']) {
    cmd.on(event, ({ toolCallId, toolName }: { toolCallId: string; toolName: string }) => {
      settle(toolCallId)
      log({ event: 'PostToolUse', tool: toolName })
    })
  }
  // A refusal aborts the whole batch; a mod's block ends that call without running it.
  cmd.on('tool_denied', () => settle())
  cmd.on('tool_hook_blocked', ({ toolCallId }: { toolCallId?: string }) => settle(toolCallId))
  cmd.on('run_end', () => {
    settle()
    log({ event: 'Stop' })
  })
  // The context, as Command Code counts it: the request's input (cache included) and its answer,
  // which the next one carries. Its window is not told to mods: Prism looks it up.
  cmd.on('model_request_end', ({ model, usage }: { model?: string; usage?: { inputTokens?: number; outputTokens?: number } }) => {
    const used = (usage?.inputTokens ?? 0) + (usage?.outputTokens ?? 0)
    if (used > 0) log({ event: 'Context', context: { used }, model })
  })
  cmd.on('compaction_done', () => log({ event: 'Compacted' }))
}

/**
 * What a tool touches, what Allow is decided on: its command (start and end, its other lines
 * counted) then the description the model gives it, never that alone; its file's path; a URL's host.
 */
function summarize(input: unknown): string | undefined {
  if (!input || typeof input !== 'object') return undefined
  const get = (key: string) => {
    const v = (input as Record<string, unknown>)[key]
    return typeof v === 'string' && v.trim() ? v : undefined
  }
  const middle = (text: string, max: number) => (text.length <= max ? text : `${text.slice(0, max - Math.floor(max / 3) - 1)}…${text.slice(-Math.floor(max / 3))}`)
  const command = get('command')
  if (command) {
    const lines = command.split('\n').map((l) => l.trim()).filter(Boolean)
    const description = get('description')
    return `${middle(lines[0] ?? '', 100)}${lines.length > 1 ? ` (+${lines.length - 1} lines)` : ''}${description ? ` — ${middle(description.split('\n')[0].trim(), 60)}` : ''}`
  }
  const path = get('file_path') ?? get('path') ?? get('notebook_path')
  if (path) return middle(path, 100)
  const url = get('url')
  return url ? url.split('://')[1]?.split('/')[0] : undefined
}
