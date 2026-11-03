<script lang="ts">
  import Icon from './Icon.svelte'
  import ChatView from './chat/ChatView.svelte'
  import SlashMenu from './SlashMenu.svelte'
  import QuickAnswers from './QuickAnswers.svelte'
  import ContextDetail from './ContextDetail.svelte'
  import AgentUsageChip from './AgentUsage.svelte'
  import AttachButton from './chat/AttachButton.svelte'
  import AttachStrip from './chat/AttachStrip.svelte'
  import { sendPrompt } from '../agent-inbox.svelte'
  import { allowAgent, attachToDraft, filesOf, stopAgent } from '../agents.svelte'
  import { cliOf, nameOf } from '../agent-cli'
  import { suggestionOf } from '../quick-answers'
  import { draftOf, pendingOf, setDraft, setPending } from '../chat-drafts.svelte'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * An agent's Chat on the computer, in place of its terminal (which stays attached under it): its
   * conversation from its transcript, its questions answered as cards, and a box to write to it.
   * The text goes through the inbox, as a quick answer: at once, even while it works or once
   * interrupted (its CLI reads it when its turn is over); held only while a menu of its waits. Enter sends, ⇧Enter adds a line, `/` offers its CLI's commands, Tab takes the
   * prompt it suggests (the box's ghost text); images attached (the button, or pasted) go with the
   * words as their `@path`, shown over the box. Its menus and its permissions' other choices stay
   * the terminal's.
   * One dock under the conversation: what it does (ChatView's line), its permission as a card,
   * the box with its quick answers while it waits for you, then its context, its usage and Stop.
   */
  let { agent, onterminal, onplan }: { agent: Agent; onterminal: () => void; onplan?: () => void } = $props()

  /** Its draft: kept when you leave it, until sent. */
  const text = $derived(draftOf(agent.id))
  const write = (v: string) => setDraft(agent.id, v)
  let busy = $state(false)
  let note = $state<{ ok: boolean; text: string } | null>(null)
  /** Sent, not yet in its transcript: shown in the conversation meanwhile, kept while you look at its terminal. */
  const pending = $derived(pendingOf(agent.id))
  let field = $state<HTMLTextAreaElement>()
  let slash = $state<SlashMenu>()
  const running = $derived(agent.state === 'running')
  const suggested = $derived(suggestionOf(agent))
  let error = $state<string | null>(null)
  /** The box grows with its lines, up to its max-height (WebKit without `field-sizing` too). */
  $effect(() => {
    void text
    if (!field) return
    field.style.height = 'auto'
    field.style.height = `${field.scrollHeight}px`
  })

  async function send() {
    const typed = text.trim()
    if (!typed || busy || !running) return
    busy = true
    note = null
    const id = agent.id
    setPending(id, typed)
    setDraft(id, '')
    try {
      const r = await sendPrompt(agent.id, typed)
      note = { ok: true, text: r === 'sent' ? (agent.status === 'running' ? t('Sent: it reads it once its turn is over') : t('Sent')) : r === 'queued' ? t('Sent once its menu is answered') : t('Already sent') }
      setTimeout(() => note?.ok && (note = null), 4000)
    } catch (e) {
      setPending(id, null)
      if (!draftOf(id)) setDraft(id, typed)
      note = { ok: false, text: t('Not sent: {error}', { error: String(e) }) }
    } finally {
      busy = false
    }
  }
  function onkeydown(e: KeyboardEvent) {
    if (slash?.keydown(e)) return
    if (e.key === 'Tab' && !e.shiftKey && !text && suggested) {
      e.preventDefault()
      write(suggested)
    } else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      void send()
    }
  }
  /** Images pasted in the box (⌘V): attached as the button does. */
  function onpaste(e: ClipboardEvent) {
    const files = filesOf(e.clipboardData?.files).filter((f) => f.type.startsWith('image/'))
    if (!files.length || !running) return
    e.preventDefault()
    error = null
    attachToDraft(agent.id, files, text, write).catch((err) => (error = t('Not attached: {error}', { error: String(err) })))
  }
  async function act(f: () => Promise<unknown>) {
    error = null
    try {
      await f()
    } catch (e) {
      error = String(e)
    }
  }
</script>

<div class="achat">
  <ChatView id={agent.id} {agent} working={agent.status === 'running'} acts={running} typing={running} wide {pending} onlanded={() => setPending(agent.id, null)} {onterminal} {onplan} />
  <div class="dock" class:under={agent.status === 'running'}>
    {#if agent.status === 'waiting_tool'}
      <div class="perm" role="group" aria-label={t('Its permission prompt')}>
        <b><Icon name="warning" /> {t('{name} asks', { name: nameOf(agent) })}</b>
        {#if agent.needs_action}<code>{agent.needs_action}</code>{/if}
        <div class="pr">
          <button class="btn allow" onclick={() => act(() => allowAgent(agent.id))} title={t('Answers {key}', { key: cliOf(agent.provider).allow })}><Icon name="check" />{t('Allow once')}</button>
          <button class="btn" onclick={onterminal} title={t('Its other choices (don’t ask again…), in its terminal')}>{t('Its other choices…')}</button>
        </div>
      </div>
    {/if}
    <form class="box" onsubmit={(e) => { e.preventDefault(); void send() }}>
      <AttachStrip id={agent.id} {text} {write} />
      <div class="field">
        <SlashMenu bind:this={slash} id={agent.id} bind:text={() => text, write} {field} />
        <textarea bind:this={field} bind:value={() => text, write} rows="2" {onkeydown} {onpaste} oninput={() => (note = null)} disabled={!running} placeholder={!running ? t('It is not running: Resume it first') : suggested ? `${suggested}   ⇥ Tab` : t('Write to {name}…', { name: agent.title })} spellcheck="true" writingsuggestions="false"></textarea>
      </div>
      <div class="foot">
        <AttachButton id={agent.id} {text} {write} disabled={!running} onerror={(e) => (error = e)} />
        <QuickAnswers {agent} />
        <span class="grow"></span>
        <span class="hint">{t('⇧↵ a line · / its commands')}</span>
        <button type="submit" class="send" disabled={busy || !running || !text.trim()} aria-label={t('Send')} title={t('Sent at once, even while it works: it reads it once its turn is over (Enter)')}><Icon name="send" /></button>
      </div>
    </form>
    <div class="meta">
      {#if agent.context || agent.usage}<ContextDetail {agent} />{/if}
      {#if agent.usage}<AgentUsageChip usage={agent.usage} provider={agent.provider} />{/if}
      <span class="grow"></span>
      {#if error}<span class="ko">{error}</span>{:else if note}<span class:ok={note.ok} class:ko={!note.ok}>{note.ok ? '✓ ' : ''}{note.text}</span>{/if}
      {#if running}<button class="btn stop" onclick={() => act(() => stopAgent(agent.id))} title={t('Ctrl-C twice: {name} exits, its last screen stays', { name: nameOf(agent) })}><Icon name="stop" />{t('Stop')}</button>{/if}
    </div>
  </div>
</div>

<style>
  .achat { position: absolute; inset: 0; display: flex; flex-direction: column; min-height: 0; }
  .dock { flex: none; display: flex; flex-direction: column; gap: 6px; padding: 7px max(12px, calc((100% - 820px) / 2)) 5px; border-top: 1px solid var(--border); background: var(--chat-dock); position: relative; }
  .dock::before { content: ''; position: absolute; inset: 0; z-index: -1; backdrop-filter: var(--chat-blur); -webkit-backdrop-filter: var(--chat-blur); pointer-events: none; }
  /* Under what it does: one block with it. */
  .dock.under { border-top: none; padding-top: 2px; }
  .perm { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; border-radius: 9px; background: var(--warn-bg, var(--surface-muted)); color: var(--warn); font-size: var(--fs-meta); }
  .perm b { display: inline-flex; align-items: center; gap: 6px; }
  .perm code { display: block; padding: 4px 8px; border-radius: 6px; background: var(--surface); color: var(--text); font: 11.5px ui-monospace, 'SF Mono', Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
  .pr { display: flex; gap: 6px; }
  .pr button { display: inline-flex; align-items: center; gap: 5px; }
  .box { display: flex; flex-direction: column; gap: 4px; border: 1px solid var(--border); border-radius: 10px; background: var(--chat-field); padding: 5px 7px 5px 10px; }
  .box:focus-within { border-color: var(--accent); }
  .field { position: relative; display: flex; min-width: 0; }
  textarea { flex: 1; min-width: 0; resize: none; border: none; background: none; color: var(--text); padding: 2px 0; font: inherit; font-size: 13.5px; line-height: 1.4; field-sizing: content; min-height: 2.8em; max-height: 14em; }
  textarea:focus { outline: none; }
  .foot { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .grow { flex: 1; }
  .hint { flex: none; font-size: var(--fs-tag); color: var(--text-muted); }
  .send { flex: none; display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: none; border-radius: 50%; background: var(--accent); color: var(--on-accent); cursor: pointer; }
  .send:disabled { opacity: 0.35; cursor: default; }
  .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 2px 12px; min-height: 22px; font-size: var(--fs-tag); color: var(--text-muted); }
  /* The Agents view's button style does not reach here: these are framed, with a pointer, to read as buttons. */
  .btn { display: inline-flex; align-items: center; gap: 4px; padding: 2px 9px; border: 1px solid color-mix(in srgb, var(--text-muted) 55%, transparent); border-radius: 6px; background: var(--surface-muted); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .btn:hover { border-color: var(--accent); }
  .meta .btn { padding: 1px 8px; font-size: var(--fs-tag); }
  /* Stop quits the agent: red on hover. */
  .btn.stop:hover { border-color: var(--ko); color: var(--ko); }
  .btn.allow { border-color: var(--warn); background: var(--warn); color: var(--surface); font-weight: 600; }
  .btn.allow:hover { filter: brightness(1.08); }
  .ok { color: var(--ok); }
  .ko { color: var(--ko); }
</style>
