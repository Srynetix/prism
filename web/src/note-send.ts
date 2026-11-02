import { hasCore, invoke, isDesktop } from './bridge'
import { nameOf } from './agent-cli'
import { sendPrompt, waiting } from './agent-inbox.svelte'
import { setStatus, withoutTags, type Note } from './notes.svelte'
import { toast } from './toast.svelte'
import { t, tn } from './i18n.svelte'
import type { Agent } from './types'

/** At its prompt: a note goes in at once. */
export const isReady = (a: { status: string }) => ['needs_you', 'review_ready', 'completed', 'idle', 'background'].includes(a.status)

/**
 * A note sent as a prompt to a running agent: submitted at once when it waits at its prompt,
 * otherwise as soon as it does (`agent-inbox`), without its tags. From a phone, typed to it as its
 * answer box does (`agent_type`: `[remote] typing`, the passkey once per agent). A TODO note handed
 * over (sent or queued) is under way: it goes DOING.
 */
export async function sendNote(n: Note, a: Agent) {
  const prompt = withoutTags(n.text)
  if (!prompt) return toast(t('Only tags: nothing to send'), 3500)
  const name = a.title || nameOf(a)
  const started = n.status === 'todo'
  const doing = started ? t(' · now DOING') : ''
  try {
    if (hasCore() && !isDesktop()) {
      await invoke('agent_type', { id: a.id, text: prompt })
      if (started) setStatus(n.id, 'doing')
      return toast(t('Sent to {name}', { name }) + doing, 3500)
    }
    const how = await sendPrompt(a.id, prompt)
    if (how === 'already-queued') return toast(t('Already queued for {name}: sent once its menu is answered', { name }), 3500)
    if (how === 'already-sent') return toast(t('Already sent to {name}', { name }), 3500)
    if (started) setStatus(n.id, 'doing')
    toast((how === 'sent' ? t('Sent to {name}', { name }) : t('Queued: sent to {name} once its menu is answered', { name }) + (waiting(a.id) > 1 ? ` (${tn(waiting(a.id), '{n} waiting', '{n} waiting')})` : '')) + doing, 3500)
  } catch (e) {
    toast(String(e), 6000)
  }
}

/**
 * A prompt written for agent `a` from elsewhere than its Chat (lines of its Files tab): sent at
 * once, or queued until its menu is answered, as a note is; from a phone, typed to it. True once
 * it is on its way.
 */
export async function promptAgent(a: Agent, prompt: string): Promise<boolean> {
  const name = a.title || nameOf(a)
  try {
    if (hasCore() && !isDesktop()) {
      await invoke('agent_type', { id: a.id, text: prompt })
      toast(t('Sent to {name}', { name }), 3500)
      return true
    }
    const how = await sendPrompt(a.id, prompt)
    if (how === 'already-queued') toast(t('Already queued for {name}: sent once its menu is answered', { name }), 3500)
    else if (how === 'already-sent') toast(t('Already sent to {name}', { name }), 3500)
    else toast(how === 'sent' ? t('Sent to {name}', { name }) : t('Queued: sent to {name} once its menu is answered', { name }), 3500)
    return how === 'sent' || how === 'queued'
  } catch (e) {
    toast(String(e), 6000)
    return false
  }
}
