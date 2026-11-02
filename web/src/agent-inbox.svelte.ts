import { invoke, isDesktop } from './bridge'
import { live } from './live.svelte'
import { pref, setPref } from './layout.svelte'

/**
 * Prompts sent to an agent (its Chat, a note, a quick answer): submitted at once, whatever it
 * does — working, interrupted, at its prompt: its CLI takes a prompt typed while it works in its
 * own queue, and reads it once its turn is over. Only a menu on its screen (a permission, its
 * question) holds it: Enter would pick that menu's choice. Held, it is submitted, in order, once
 * the menu is answered — never left typed in its input. Prism keeps the queue
 * (`prism_agents::chains`), whatever the window shows; an agent that exits takes its queue with it.
 * Sent twice by mistake (a double click, a second click while it works): the same text already
 * waiting for it, or sent to it a moment ago, is not sent again.
 */

/** Its question waits for an answer (`status.rs`'s QUESTION): its menu is on its screen. */
const QUESTION = 'Answer its question'
const agentOf = (id: string) => live.snapshot?.agents?.find((a) => a.id === id)
/** Running, no menu on its screen: a prompt typed now reaches its input (the Diff review's too). */
export const ready = (id: string) => {
  const a = agentOf(id)
  return !!a && a.state === 'running' && a.status !== 'waiting_tool' && a.needs_action !== QUESTION
}

/** Prompts waiting for agent `id`. */
export const waiting = (id: string) => live.snapshot?.inbox?.[id] ?? 0

/**
 * Sends `text` to agent `id`: `sent` at once, or `queued` until its menu is answered; `already-queued`
 * when the same text waits for it, `already-sent` when it was just sent to it — nothing done.
 */
export const sendPrompt = (id: string, text: string) => invoke<'sent' | 'queued' | 'already-queued' | 'already-sent'>('agent_send', { id, text })

// The queue the window kept before Prism did: given to Prism once.
const OLD = 'agents.inbox'
if (isDesktop()) {
  const old = pref<Record<string, string[]>>(OLD, {})
  if (Object.keys(old).length) {
    setPref(OLD, {})
    void (async () => {
      for (const [id, queue] of Object.entries(old)) for (const text of queue) await sendPrompt(id, text).catch(() => null)
    })()
  }
}
