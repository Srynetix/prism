<script lang="ts">
  import { tick, untrack } from 'svelte'
  import Icon from '../Icon.svelte'
  import AgentChat from '../AgentChat.svelte'
  import Drawer from '../ui/Drawer.svelte'
  import AssistantMemory from './AssistantMemory.svelte'
  import { t, tn } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import { keys } from '../../platform'
  import { hit } from '../../shortcuts'
  import { interruptAgent, openAgents, resumeAgent } from '../../agents.svelte'
  import { nameOf } from '../../agent-cli'
  import { toastError } from '../../toast.svelte'
  import { askPrism, assistantView, byDay, closeAssistant, conversations, conversationState, loadMemory, memory, proposalsOf } from '../../assistant.svelte'
  import type { Agent } from '../../types'
  import { isArchived, setArchived } from '../../agent-archive.svelte'

  /**
   * The Assistant panel, over the page: its conversations by day on the left (one per request,
   * reopened in a click), the chosen one's Chat on the right (AgentChat: its proposals as cards);
   * the Memory tab reads, changes and empties what it remembers. « New request » starts a new conversation.
   * Its shell is Drawer's (Escape, a click outside); ⌥↵ closes it, from its question and its
   * Chat too; ⌘. interrupts the conversation shown. **Archive** takes a conversation out of the
   * list (`agents.archived`, as an agent's): **Archived · N** at its foot shows them, and one that
   * waits for you again comes back by itself, a proposal to validate too.
   */
  const all = $derived(conversations(live.snapshot))
  /** Out of the list: archived, and no proposal waiting for your click. */
  const parted = $derived.by(() => {
    const out = { list: [] as Agent[], archived: [] as Agent[] }
    for (const a of all) (isArchived(a) && !proposalsOf(live.snapshot, a.id).length ? out.archived : out.list).push(a)
    return out
  })
  const list = $derived(parted.list)
  const archived = $derived(parted.archived)
  const isOut = (id: string) => archived.some((a) => a.id === id)
  let showArchived = $state(false)
  const viewingArchived = $derived(showArchived && archived.length > 0)
  const days = $derived(byDay(viewingArchived ? archived : list))
  const shown = $derived(all.find((a) => a.id === assistantView.selected) ?? null)
  // An archived one opened from elsewhere (the palette): the archived ones listed, it among them.
  $effect(() => {
    const id = assistantView.selected
    if (id && untrack(() => isOut(id))) showArchived = true
  })

  /** Archived: the next one of the list shown in its place, the question box after the last. */
  function archive(a: Agent) {
    const i = list.findIndex((x) => x.id === a.id)
    const next = list[i + 1] ?? list[i - 1] ?? null
    setArchived(a, true)
    assistantView.selected = viewingArchived ? a.id : (next?.id ?? null)
  }
  let draft = $state('')
  let box: HTMLTextAreaElement | undefined = $state()

  $effect(() => {
    if (!memory.loaded) void loadMemory()
  })

  async function ask() {
    const text = draft.trim()
    if (!text || assistantView.asking) return
    const id = await askPrism(text)
    if (id) draft = ''
  }

  async function newRequest() {
    assistantView.selected = null
    assistantView.tab = 'conversations'
    await tick()
    box?.focus()
  }

  let panel: HTMLElement | undefined = $state()
  /** ⌘. in the panel: the conversation shown, at work, stops its turn, as in the Agents view. */
  function interrupt(e: KeyboardEvent) {
    if (!hit('interrupt', e) || !(e.target instanceof Node) || !panel?.contains(e.target)) return
    if (assistantView.tab !== 'conversations' || shown?.state !== 'running' || shown.status !== 'running') return
    e.preventDefault()
    interruptAgent(shown.id).catch((err) => toastError(String(err)))
  }

  const DOT = { wait: 'warn', run: 'ok pulse', done: 'info', idle: '' } as const

  const EXAMPLES = [
    t('Create a ticket for the shop app to export orders as CSV'),
    t('Implement ticket SHOP-214 on the back'),
    t('What awaits me this morning?'),
  ]
</script>

<svelte:window onkeydown={interrupt} />

<Drawer bind:el={panel} open={assistantView.open} key="assistant" identity={assistantView.selected ?? ''} crumb={t('Assistant')} label={t('Assistant')} size="l" source="var(--info)" onclose={closeAssistant}>
  {#snippet head()}
    <b class="title"><Icon name="sparkles" />{t('Assistant')}</b>
    <div class="seg" role="tablist">
      <button role="tab" class:on={assistantView.tab === 'conversations'} aria-selected={assistantView.tab === 'conversations'} onclick={() => (assistantView.tab = 'conversations')}>{t('Conversations')}</button>
      <button role="tab" class:on={assistantView.tab === 'memory'} aria-selected={assistantView.tab === 'memory'} onclick={() => (assistantView.tab = 'memory')}>{t('Memory')}{#if memory.facts.length} · {memory.facts.length}{/if}</button>
    </div>
    <span class="grow"></span>
    <button class="btn sm" onclick={newRequest} title={t('A new conversation')}><Icon name="plus" size={12} />{t('New request')}</button>
  {/snippet}

  {#if assistantView.tab === 'memory'}
    <div class="body"><AssistantMemory /></div>
  {:else}
    <div class="app">
      <nav class="convs" aria-label={t('Conversations')}>
        {#each days as d (d.day)}
          <div class="day">{d.day}</div>
          {#each d.list as a (a.id)}
            {@const st = conversationState(a, live.snapshot)}
            <button class="conv" class:on={a.id === assistantView.selected} onclick={() => (assistantView.selected = a.id)}>
              <b>{a.title}</b>
              <small><span class="dot {DOT[st.tone]}"></span>{st.label}</small>
            </button>
          {/each}
        {:else}
          <p class="none">{archived.length ? t('Every conversation is archived.') : t('No conversation yet: ask your first question on the right.')}</p>
        {/each}
        {#if archived.length}
          <button class="arch" class:on={viewingArchived} onclick={() => (showArchived = !viewingArchived)} title={viewingArchived ? t('Back to the conversations') : t('The conversations you archived')}><Icon name={viewingArchived ? 'back' : 'hide'} size={12} />{viewingArchived ? t('Conversations') : t('Archived · {n}', { n: archived.length })}</button>
        {/if}
      </nav>
      <section class="chat">
        {#if shown}
          {@const st = conversationState(shown, live.snapshot)}
          <div class="ch">
            <Icon name="sparkles" size={13} />
            <b>{shown.title}</b>
            <span class="meta">{nameOf(shown)} · {st.label}</span>
            <span class="grow"></span>
            {#if shown.state === 'exited' || shown.state === 'gone'}<button class="btn sm" onclick={() => resumeAgent(shown.id).catch((e) => toastError(String(e)))}>{t('Resume')}</button>{/if}
            {#if isOut(shown.id)}
              <button class="btn sm ghost" onclick={() => setArchived(shown, false)} title={t('Back in the list of conversations')}>{t('Unarchive')}</button>
            {:else}
              <button class="btn sm ghost" onclick={() => archive(shown)} disabled={proposalsOf(live.snapshot, shown.id).length > 0} title={proposalsOf(live.snapshot, shown.id).length ? t('A proposal waits for your click: validate or dismiss it first') : t('Out of the list, nothing stopped nor removed; back by itself when it waits for you')}>{t('Archive')}</button>
            {/if}
            <button class="btn sm ghost" onclick={() => { closeAssistant(); openAgents(shown.id) }} title={t('Its terminal and its tabs, in the Agents view')}>{t('Open as an agent')}</button>
          </div>
          {#key shown.id}
            <div class="host"><AgentChat agent={shown} onterminal={() => { closeAssistant(); openAgents(shown.id) }} /></div>
          {/key}
        {:else}
          <div class="ask">
            <h2><Icon name="sparkles" />{t('Ask Prism')}</h2>
            <p class="lead">{t('In your own words: it finds the repository, reads the tickets and the code, and proposes a ticket or an agent to launch. Nothing is created nor launched without your click.')}</p>
            <!-- svelte-ignore a11y_autofocus -->
            <textarea bind:this={box} bind:value={draft} rows="3" autofocus placeholder={t('Create a ticket for…, Implement…, Where are we on…?')}
              onkeydown={(e) => { if (hit('assistant', e)) { e.preventDefault(); closeAssistant() } else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); void ask() } }}></textarea>
            <div class="row">
              <button class="btn primary" style:--btn-c="var(--info)" style:--btn-on="var(--surface)" onclick={ask} disabled={!draft.trim() || assistantView.asking}>{assistantView.asking ? t('Starting…') : t('Ask')}</button>
              <span class="hint">{keys('↵')} {t('asks')} · {keys('⇧↵')} {t('a new line')} · {t('from anywhere: the palette, then ⌥↵')}</span>
            </div>
            <div class="ex">
              {#each EXAMPLES as e (e)}<button class="chip" onclick={() => { draft = e; box?.focus() }}>{e}</button>{/each}
            </div>
            {#if list.length}<p class="count">{tn(list.length, '{n} conversation on the left', '{n} conversations on the left')}</p>{/if}
          </div>
        {/if}
      </section>
    </div>
  {/if}
</Drawer>

<style>
  b.title { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-title); color: var(--text); }
  b.title :global(svg) { color: var(--info); }
  .grow { flex: 1; }
  .body { flex: 1; min-height: 0; overflow: auto; }
  .app { flex: 1; min-height: 0; display: grid; grid-template-columns: 230px minmax(0, 1fr); }
  .convs { overflow: auto; padding: 4px; border-right: 1px solid var(--border); background: var(--surface-muted); }
  .day { padding: 8px 8px 2px; color: var(--text-muted); font-size: var(--fs-tag); }
  .conv { display: block; width: 100%; padding: 6px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .conv:hover { background: var(--surface); }
  .conv.on { background: color-mix(in srgb, var(--info) 14%, var(--surface)); }
  .conv b { display: block; overflow: hidden; font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
  .conv small { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-tag); }
  .arch { display: flex; align-items: center; gap: 6px; width: 100%; margin-top: 8px; padding: 6px 8px; border: none; border-top: 1px solid var(--border); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .arch:hover, .arch.on { color: var(--text); }
  .none { margin: 10px 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .chat { display: flex; flex-direction: column; min-height: 0; }
  .ch { display: flex; align-items: center; gap: 8px; padding: 7px 12px; border-bottom: 1px solid var(--border); }
  .ch :global(svg) { color: var(--info); }
  .ch b { overflow: hidden; font-size: var(--fs-title); text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--text-muted); font-size: var(--fs-meta); white-space: nowrap; }
  /* The Chat fills its host (`.achat` is absolute): positioned, or it would cover the whole drawer. */
  .host { position: relative; flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .ask { max-width: 620px; margin: 8vh auto 0; padding: 0 20px; }
  .ask h2 { display: flex; align-items: center; gap: 8px; margin: 0 0 6px; font-size: var(--fs-big); }
  .ask h2 :global(svg) { color: var(--info); }
  .lead { margin: 0 0 12px; color: var(--text-muted); }
  .ask textarea { width: 100%; box-sizing: border-box; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8, 8px); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-title); resize: vertical; }
  .row { display: flex; align-items: center; gap: 10px; margin-top: 8px; }
  .hint { color: var(--text-muted); font-size: var(--fs-tag); }
  .ex { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 16px; }
  .chip { padding: 3px 9px; border: 1px solid var(--border); border-radius: var(--r12); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .chip:hover { border-color: var(--info); color: var(--text); }
  .count { margin-top: 18px; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
