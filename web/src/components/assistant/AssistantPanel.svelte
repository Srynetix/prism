<script lang="ts">
  import { tick } from 'svelte'
  import Icon from '../Icon.svelte'
  import AgentChat from '../AgentChat.svelte'
  import AssistantMemory from './AssistantMemory.svelte'
  import { t, tn } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import { keys } from '../../platform'
  import { openAgents, resumeAgent } from '../../agents.svelte'
  import { nameOf } from '../../agent-cli'
  import { toast } from '../../toast.svelte'
  import { askPrism, assistantView, byDay, closeAssistant, conversations, conversationState, loadMemory, memory } from '../../assistant.svelte'

  /**
   * The Assistant panel, over the page: its conversations by day on the left (one per request,
   * reopened in a click), the chosen one's Chat on the right (AgentChat: its proposals as cards);
   * the Memory tab reads, changes and empties what it remembers. « Ask » starts a new conversation.
   */
  const list = $derived(conversations(live.snapshot))
  const days = $derived(byDay(list))
  const shown = $derived(list.find((a) => a.id === assistantView.selected) ?? null)
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

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !assistantView.open) return
    // A card's field, a menu: theirs first.
    if ((e.target as HTMLElement | null)?.closest('input, textarea, [role=menu]')) return
    e.stopPropagation()
    closeAssistant()
  }

  const EXAMPLES = [
    t('Create a ticket for the shop app to export orders as CSV'),
    t('Implement ticket SHOP-214 on the back'),
    t('What awaits me this morning?'),
  ]
</script>

<svelte:window onkeydown={onKey} />

<aside class="drawer" data-drawer="52" aria-label={t('Assistant')}>
  <header>
    <b><Icon name="sparkles" />{t('Assistant')}</b>
    <div class="tabs" role="tablist">
      <button role="tab" class:on={assistantView.tab === 'conversations'} aria-selected={assistantView.tab === 'conversations'} onclick={() => (assistantView.tab = 'conversations')}>{t('Conversations')}</button>
      <button role="tab" class:on={assistantView.tab === 'memory'} aria-selected={assistantView.tab === 'memory'} onclick={() => (assistantView.tab = 'memory')}>{t('Memory')}{#if memory.facts.length} · {memory.facts.length}{/if}</button>
    </div>
    <span class="grow"></span>
    <button class="btn" onclick={newRequest} title={t('A new conversation')}><Icon name="plus" size={12} />{t('Ask')}</button>
    <button class="x" onclick={closeAssistant} aria-label={t('Close (Esc)')}><Icon name="close" /></button>
  </header>

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
              <small><span class="st {st.tone}"></span>{st.label}</small>
            </button>
          {/each}
        {:else}
          <p class="none">{t('No conversation yet: ask your first question on the right.')}</p>
        {/each}
      </nav>
      <section class="chat">
        {#if shown}
          {@const st = conversationState(shown, live.snapshot)}
          <div class="ch">
            <Icon name="sparkles" size={13} />
            <b>{shown.title}</b>
            <span class="meta">{nameOf(shown)} · {st.label}</span>
            <span class="grow"></span>
            {#if shown.state === 'exited' || shown.state === 'gone'}<button class="btn" onclick={() => resumeAgent(shown.id).catch((e) => toast(String(e)))}>{t('Resume')}</button>{/if}
            <button class="btn ghost" onclick={() => { closeAssistant(); openAgents(shown.id) }} title={t('Its terminal and its tabs, in the Agents view')}>{t('Open as an agent')}</button>
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
              onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); void ask() } }}></textarea>
            <div class="row">
              <button class="btn pri" onclick={ask} disabled={!draft.trim() || assistantView.asking}>{assistantView.asking ? t('Starting…') : t('Ask')}</button>
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
</aside>

<style>
  .drawer { position: fixed; top: 0; right: 0; bottom: 0; z-index: 52; width: min(1040px, 94vw); display: flex; flex-direction: column; background: var(--surface); border-left: 1px solid color-mix(in srgb, var(--info) 45%, var(--border)); box-shadow: var(--shadow-drawer); animation: slide 0.16s ease-out; }
  @keyframes slide { from { transform: translateX(24px); opacity: 0; } }
  :global(html[data-reduce-motion]) .drawer { animation: none; }
  header { display: flex; align-items: center; gap: 10px; padding: 8px 12px; flex-shrink: 0; border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--info); }
  header b { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; }
  header b :global(svg) { color: var(--info); }
  .tabs { display: flex; gap: 2px; }
  .tabs button { padding: 3px 9px; border: none; border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .tabs button.on { background: var(--surface-muted); color: var(--text); font-weight: 600; }
  .grow { flex: 1; }
  .x { display: inline-flex; padding: 3px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .btn { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn.pri { border-color: var(--info); background: var(--info); color: var(--surface); }
  .btn.ghost { border-color: transparent; background: none; color: var(--text-muted); }
  .btn:disabled { opacity: 0.55; cursor: default; }
  .body { flex: 1; min-height: 0; overflow: auto; }
  .app { flex: 1; min-height: 0; display: grid; grid-template-columns: 230px minmax(0, 1fr); }
  .convs { overflow: auto; padding: 4px; border-right: 1px solid var(--border); background: var(--surface-muted); }
  .day { padding: 8px 8px 2px; color: var(--text-muted); font-size: var(--fs-tag); }
  .conv { display: block; width: 100%; padding: 6px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .conv:hover { background: var(--surface); }
  .conv.on { background: color-mix(in srgb, var(--info) 14%, var(--surface)); }
  .conv b { display: block; overflow: hidden; font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
  .conv small { display: flex; align-items: center; gap: 6px; color: var(--text-muted); font-size: var(--fs-tag); }
  .st { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); }
  .st.wait { background: var(--warn); }
  .st.run { background: var(--ok); animation: pulse 1.4s infinite; }
  .st.done { background: var(--info); }
  @keyframes pulse { 50% { opacity: 0.35; } }
  :global(html[data-reduce-motion]) .st.run { animation: none; }
  .none { margin: 10px 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .chat { display: flex; flex-direction: column; min-height: 0; }
  .ch { display: flex; align-items: center; gap: 8px; padding: 7px 12px; border-bottom: 1px solid var(--border); }
  .ch :global(svg) { color: var(--info); }
  .ch b { overflow: hidden; font-size: 13px; text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--text-muted); font-size: var(--fs-meta); white-space: nowrap; }
  .host { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .ask { max-width: 620px; margin: 8vh auto 0; padding: 0 20px; }
  .ask h2 { display: flex; align-items: center; gap: 8px; margin: 0 0 6px; font-size: 18px; }
  .ask h2 :global(svg) { color: var(--info); }
  .lead { margin: 0 0 12px; color: var(--text-muted); }
  .ask textarea { width: 100%; box-sizing: border-box; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8, 8px); background: var(--bg); color: var(--text); font: inherit; font-size: 13px; resize: vertical; }
  .row { display: flex; align-items: center; gap: 10px; margin-top: 8px; }
  .hint { color: var(--text-muted); font-size: var(--fs-tag); }
  .ex { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 16px; }
  .chip { padding: 3px 9px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .chip:hover { border-color: var(--info); color: var(--text); }
  .count { margin-top: 18px; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
