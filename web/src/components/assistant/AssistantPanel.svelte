<script lang="ts">
  import { tick } from 'svelte'
  import Icon from '../Icon.svelte'
  import AgentChat from '../AgentChat.svelte'
  import Drawer from '../ui/Drawer.svelte'
  import AssistantMemory from './AssistantMemory.svelte'
  import { t, tn } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import { keys } from '../../platform'
  import { openAgents, resumeAgent } from '../../agents.svelte'
  import { nameOf } from '../../agent-cli'
  import { toastError } from '../../toast.svelte'
  import { askPrism, assistantView, byDay, closeAssistant, conversations, conversationState, loadMemory, memory } from '../../assistant.svelte'

  /**
   * The Assistant panel, over the page: its conversations by day on the left (one per request,
   * reopened in a click), the chosen one's Chat on the right (AgentChat: its proposals as cards);
   * the Memory tab reads, changes and empties what it remembers. « Ask » starts a new conversation.
   * Its shell is Drawer's (Escape, a click outside).
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

  const DOT = { wait: 'warn', run: 'ok pulse', done: 'info', idle: '' } as const

  const EXAMPLES = [
    t('Create a ticket for the shop app to export orders as CSV'),
    t('Implement ticket SHOP-214 on the back'),
    t('What awaits me this morning?'),
  ]
</script>

<Drawer open={assistantView.open} key="assistant" identity={assistantView.selected ?? ''} crumb={t('Assistant')} label={t('Assistant')} size="l" source="var(--info)" onclose={closeAssistant}>
  {#snippet head()}
    <b class="title"><Icon name="sparkles" />{t('Assistant')}</b>
    <div class="seg" role="tablist">
      <button role="tab" class:on={assistantView.tab === 'conversations'} aria-selected={assistantView.tab === 'conversations'} onclick={() => (assistantView.tab = 'conversations')}>{t('Conversations')}</button>
      <button role="tab" class:on={assistantView.tab === 'memory'} aria-selected={assistantView.tab === 'memory'} onclick={() => (assistantView.tab = 'memory')}>{t('Memory')}{#if memory.facts.length} · {memory.facts.length}{/if}</button>
    </div>
    <span class="grow"></span>
    <button class="btn sm" onclick={newRequest} title={t('A new conversation')}><Icon name="plus" size={12} />{t('Ask')}</button>
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
            {#if shown.state === 'exited' || shown.state === 'gone'}<button class="btn sm" onclick={() => resumeAgent(shown.id).catch((e) => toastError(String(e)))}>{t('Resume')}</button>{/if}
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
              onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); void ask() } }}></textarea>
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
  .none { margin: 10px 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .chat { display: flex; flex-direction: column; min-height: 0; }
  .ch { display: flex; align-items: center; gap: 8px; padding: 7px 12px; border-bottom: 1px solid var(--border); }
  .ch :global(svg) { color: var(--info); }
  .ch b { overflow: hidden; font-size: var(--fs-title); text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--text-muted); font-size: var(--fs-meta); white-space: nowrap; }
  .host { flex: 1; min-height: 0; display: flex; flex-direction: column; }
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
