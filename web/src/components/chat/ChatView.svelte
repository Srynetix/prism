<script lang="ts">
  import { untrack } from 'svelte'
  import { pollsShown } from '../../polls.svelte'
  import { clock } from '../../clock.svelte'
  import Icon from '../Icon.svelte'
  import Markdown from '../Markdown.svelte'
  import ChatLegs from './ChatLegs.svelte'
  import AgentChain from '../AgentChain.svelte'
  import { agentAttachment, agentConversation, agentPastedImage, agentsSettings, chatFontSize, interruptAgent, modelName, type AgentConversation, type BackgroundTask, type ConversationEntry, type ConversationTool } from '../../agents.svelte'
  import { chainActive, chainOf } from '../../agent-chains.svelte'
  import type { IconName } from '../../icons'
  import { span } from '../../time'
  import type { Agent } from '../../types'
  import { base, matches, promptImages, segments, summarize, sumSteps } from './turns'
  import QuestionCard from './QuestionCard.svelte'
  import PlanCard from './PlanCard.svelte'
  import AsideCard from '../AsideCard.svelte'
  import { asideOfCall, isSuggest } from '../../asides.svelte'
  import CommentedAnswer from './CommentedAnswer.svelte'
  import PromptImages, { type PromptImage } from './PromptImages.svelte'
  import type { PastedImages } from '../../bindings/PastedImages'
  import { commentable } from '../../comments'
  import { isDesktop } from '../../bridge'
  import { openInPrism } from '../../ref-links'
  import { pendingAfter, setPendingAfter } from '../../chat-drafts.svelte'
  import { noteAgentFiles } from '../../dictation.svelte'
  import { t as tr, tc, tn } from '../../i18n.svelte'

  /**
   * An agent's conversation on a phone (its Chat tab), from its transcript rather than its
   * terminal: your prompts as bubbles, its words as Markdown, its tool calls a line each (a tap:
   * a command's last lines, an edit's diff), files read in a row folded into one. Read again every
   * few seconds while shown, kept at the bottom unless scrolled up; earlier turns on demand. Under
   * it, the terminal's last lines, live: a tap opens the terminal (its menus). Its questions
   * (`AskUserQuestion`) as cards, answered here.
   * On the computer (`wide`), the same in place of the agent's terminal, which stays attached under it.
   * Turns: the conversation as a list of its turns, each summed up (files edited, commands, how
   * long, its last words), searched; a tap goes to that turn.
   * As it works: its thinking (its words when its CLI keeps them), each call's time, a subagent's
   * own calls, its plan pinned on top, its background tasks under it (running, then those just
   * over: what they are, for how long, how they ended), and under the conversation what its CLI shows it doing
   * (its spinner's words, the call under way and its lines so far, the model answering), Esc at
   * hand. Each turn opens with its time and what it took: time, tokens, cost, and the model that
   * answered it.
   * Calm: its plan, its background tasks, its chain and its turns as chips on one rail, each a
   * popover (a sheet on a phone) rather than a bar; the calls and thoughts between two of its
   * answers folded into one line (`[agents] fold_steps`), the turn under way showing its last
   * ones; on a phone, the terminal's lines behind a tap on the line of what it does.
   * Its plans (plan mode) as cards, commented on passage by passage and answered here (PlanCard);
   * its long answers commented on too (`[agents] comment_answers`, CommentedAnswer).
   */
  let { id, agent, working, acts, typing, passkey = false, tail = [], wide = false, pending = null, onlanded, onterminal, onplan }: {
    id: string
    /** Its chain, as a chip of the rail. */
    agent?: Agent
    /** Its turn is under way. */
    working: boolean
    /** It may be acted on (a phone: `[remote] actions`), and typed to (`[remote] typing`). */
    acts: boolean
    typing: boolean
    /** Typed words ask the phone's passkey. */
    passkey?: boolean
    /** The terminal's last lines, cleaned (a phone), behind a tap on what it does. */
    tail?: string[]
    /** On the computer: a reading width, in a wide pane. */
    wide?: boolean
    /** Words just sent, shown at once until its transcript has them (`onlanded`). */
    pending?: string | null
    onlanded?: () => void
    onterminal: () => void
    /** Opens its Plan tab. */
    onplan?: () => void
  } = $props()

  const PAGE = 30
  /** All its turns (the backend's limit too): Prism reads its transcript further back for them. */
  const ALL = 5000
  let turns = $state(PAGE)
  /**
   * Raw, replaced whole at each change: a deep proxy cost each read a trap and each comparison a
   * walk through it. Its turns' signatures go with each read: those unchanged come back by number
   * only (`kept`) and keep their object, so their block (their Markdown above all) is not rendered
   * again; while it works, only the last turn travels and moves.
   */
  let convo = $state.raw<AgentConversation | null | undefined>(undefined)
  // The files it read or wrote: words a dictation into its box expects.
  $effect(() => noteAgentFiles(id, convo))
  /** The last answer as read, to tell a change without walking `convo`. */
  let lastJson = ''
  let error = $state<string | null>(null)
  let pane: HTMLDivElement | undefined = $state()
  let atBottom = $state(true)
  let open = $state<Record<string, boolean>>({})

  let loading = false
  /** The next read asks all of the turns, none said held. */
  let resync = false
  async function load() {
    if (loading) return
    loading = true
    const asked = turns
    const whole = resync
    resync = false
    try {
      const held = whole ? [] : (convo?.turns ?? [])
      const next = await agentConversation(id, asked, held.flatMap((t) => (t.sig ? [t.sig] : [])))
      error = null
      const json = JSON.stringify(next)
      if (json !== lastJson) {
        lastJson = json
        const byN = new Map(held.map((t) => [t.n, t]))
        const kept = (next?.kept ?? []).map((n) => byN.get(n))
        // A turn said held that is not (another agent's meanwhile): all of them again, at once.
        if (kept.some((t) => !t)) {
          lastJson = ''
          resync = true
          return
        }
        convo = next ? { ...next, turns: [...next.turns, ...(kept as typeof held)].sort((a, b) => a.n - b.n), kept: undefined } : next
        // At the bottom, it stays there; the top still in sight (a short conversation), earlier turns follow.
        if (atBottom && anchor === null) requestAnimationFrame(() => (toBottom(), pane && pane.scrollTop < NEAR_TOP && earlier()))
      }
      if (anchor !== null && asked === turns) requestAnimationFrame(landed)
    } catch (e) {
      error = String(e)
      older = false
      anchor = null
    } finally {
      loading = false
      // More asked meanwhile (scrolled up): read again at once, not at the next tick.
      if (turns !== asked || resync) void load()
    }
  }
  // Read again every second while it works or words wait to show, every 3 s otherwise; at once
  // when either changes.
  const fast = $derived(working || !!pending)
  const shown = pollsShown()
  $effect(() => {
    void id
    void turns
    // Hidden (another view, the window): read again once shown.
    if (!shown()) return
    // load() reads the turns held: tracked, each answer would read again at once (3 reads a second).
    untrack(() => void load())
    const t = setInterval(load, fast ? 1000 : 3000)
    return () => clearInterval(t)
  })
  /**
   * When the last turn started as the words were sent (kept with them, so that the Chat opened
   * again, from its terminal, still waits): a later one (or theirs) means they landed.
   */
  const started = (t: { at: string | null } | undefined) => (t?.at ? Date.parse(t.at) || 0 : 0)
  $effect(() => {
    if (!pending || !convo) return
    if (untrack(() => pendingAfter(id)) === undefined) setPendingAfter(id, started(convo.turns.at(-1)))
    if (atBottom) requestAnimationFrame(toBottom)
  })
  $effect(() => {
    const last = convo?.turns.at(-1)
    const after = pendingAfter(id)
    if (!pending || !last || after === undefined) return
    if (started(last) > after || last.prompt.trim() === pending.trim()) onlanded?.()
  })
  /** The rail's chip open: its popover, or its sheet on a phone. */
  type Drop = 'plan' | 'bg' | 'chain' | 'turns'
  let drop = $state<Drop | null>(null)
  /** The list of turns, searched; all of them read while it is open. */
  let query = $state('')
  function show(d: Drop) {
    drop = drop === d ? null : d
    if (drop === 'turns') turns = ALL
  }
  // Back at the bottom for a while, the list of turns closed: the last page again, so that a long
  // session's turns read once (the list, scrolling up) no longer render and travel at each read
  // (1000 turns: 166k nodes, 11 ms a read).
  const SETTLED_MS = 10_000
  $effect(() => {
    if (!atBottom || drop === 'turns' || turns <= PAGE) return
    const t = setTimeout(() => (turns = PAGE), SETTLED_MS)
    return () => clearTimeout(t)
  })
  const found = $derived(convo ? convo.turns.filter((t) => matches(t, query)) : [])
  /** Back to the conversation, at that turn. */
  function jump(n: number) {
    drop = null
    atBottom = false
    requestAnimationFrame(() => document.getElementById(`turn-${n}`)?.scrollIntoView({ block: 'start' }))
  }

  /** Its background tasks: shown while one runs, or one ended in the last minutes, or open. */
  const background = $derived(convo?.background ?? [])
  const bgRunning = $derived(background.filter((b) => b.state === 'running'))
  const bgOpen = $derived(drop === 'bg')
  /** The time, every second while it works or a task runs: the call under way counts up. */
  const now = $derived(working || bgRunning.length > 0 ? clock(1000).now : Date.now())
  const bgShown = $derived(bgOpen || bgRunning.length > 0 || background.some((b) => b.ended && now - Date.parse(b.ended) < 10 * 60_000))
  /** The call under way: the last turn's last running one. */
  const running = $derived.by(() => {
    const entries = convo?.turns.at(-1)?.entries ?? []
    for (let i = entries.length - 1; i >= 0; i--) {
      const e = entries[i]
      if (e.type === 'tool' && e.state === 'running' && !e.ask && !e.proposal) return e
    }
    return null
  })
  const live = $derived(working ? (convo?.live ?? null) : null)
  let stopping = $state(false)
  async function stop() {
    stopping = true
    try {
      await interruptAgent(id)
    } catch (e) {
      error = String(e)
    } finally {
      stopping = false
    }
  }

  /** Its plan, while a step is left. */
  const plan = $derived(convo?.plan ?? [])
  const planDone = $derived(plan.filter((p) => p.status === 'completed').length)
  const planNow = $derived(plan.find((p) => p.status === 'in_progress') ?? plan.find((p) => p.status !== 'completed'))
  const planShown = $derived(plan.length > 0 && planDone < plan.length)

  /** Its plans proposed (plan mode), in order: the last one is answered; each shows its changes since the one before. */
  const proposals = $derived((convo?.turns ?? []).flatMap((t) => t.entries.flatMap((e) => (e.type === 'tool' && e.proposal ? [e.proposal] : []))))
  const lastPlan = $derived(proposals.at(-1)?.call)
  const before = (call: string) => {
    const i = proposals.findIndex((p) => p.call === call)
    return i > 0 && proposals[i - 1].text ? proposals[i - 1].text : null
  }
  /** Its long answers commented on (`[agents] comment_answers`). */
  const commentAnswers = $derived(agentsSettings().comment_answers ?? true)

  /**
   * A link in its conversation to an MR or a ticket Prism shows opens its panel (on the computer, as in
   * its Terminal); any other — and with ⇧ or ⌘ — goes on to the browser (main.ts).
   */
  function onlink(e: MouseEvent) {
    const a = (e.target as HTMLElement | null)?.closest?.('a[href]')
    if (!a || e.defaultPrevented || e.shiftKey || e.metaKey || e.ctrlKey || !isDesktop()) return
    if (openInPrism(a.getAttribute('href') ?? '')) e.preventDefault()
  }

  /** Its chain (Then…), while it has one. */
  const chain = $derived(agent ? chainOf(agent.id) : undefined)
  const chainHeld = $derived(!!chain && chain.runs[chain.current]?.state === 'held')
  const chainDone = $derived(chain ? chain.runs.filter((r) => r.state === 'done' || r.state === 'skipped' || r.state === 'failed').length : 0)

  /** A phone: the terminal's lines, unfolded under what it does. */
  let peek = $state(false)
  /** Its steps folded (`[agents] fold_steps`), the turn under way showing its last ones. */
  const fold = $derived(agentsSettings().fold_steps ?? true)
  const LIVE_SHOWN = 3

  const toBottom = () => pane && (pane.scrollTop = pane.scrollHeight)
  // The terminal's lines under it come and go: at the bottom, it stays there.
  $effect(() => {
    if (!pane) return
    const seen = new ResizeObserver(() => atBottom && toBottom())
    seen.observe(pane)
    return () => seen.disconnect()
  })
  function onscroll() {
    if (!pane) return
    atBottom = pane.scrollHeight - pane.scrollTop - pane.clientHeight < 40
    if (pane.scrollTop < NEAR_TOP) earlier()
  }
  /**
   * Earlier turns, read as you scroll up near the top: 30 more each time, until its session's
   * first. The view stays where it was (`anchor`: its distance from the bottom), and while the
   * top is still in sight, the next ones follow.
   */
  const NEAR_TOP = 400
  let older = $state(false)
  let anchor: number | null = null
  const more = $derived(!!convo && (convo.total > convo.turns.length || convo.partial) && turns < ALL)
  function earlier() {
    if (older || !more || !pane) return
    older = true
    anchor = pane.scrollHeight - pane.scrollTop
    turns = Math.min(ALL, turns + PAGE)
  }
  function landed() {
    if (pane && anchor !== null) pane.scrollTop = pane.scrollHeight - anchor
    anchor = null
    older = false
    if (pane && pane.scrollTop < NEAR_TOP) earlier()
  }

  type Row = { type: 'entry'; entry: ConversationEntry; key: string } | { type: 'reads'; tools: ConversationTool[]; key: string }
  /** A turn's entries (from its `from`th), the files read (searched) in a row as one line. */
  function rows(n: number, entries: ConversationEntry[], from = 0): Row[] {
    const out: Row[] = []
    entries.forEach((e, i) => {
      const key = `${n}:${from + i}`
      const last = out[out.length - 1]
      if (e.type === 'tool' && e.kind === 'read' && e.state !== 'failed') {
        if (last?.type === 'reads') last.tools.push(e)
        else out.push({ type: 'reads', tools: [e], key })
      } else out.push({ type: 'entry', entry: e, key })
    })
    return out.map((r) => (r.type === 'reads' && r.tools.length === 1 ? { type: 'entry', entry: { type: 'tool', ...r.tools[0] }, key: r.key } : r))
  }

  const BG_ICONS: Record<BackgroundTask['kind'], IconName> = { command: 'terminal', agent: 'group', monitor: 'activity' }
  const BG_LABEL: Record<BackgroundTask['kind'], string> = { command: tr('Command'), agent: tr('Subagent'), monitor: tr('Monitor') }
  const BG_STATE: Record<BackgroundTask['state'], string> = { running: tr('running'), completed: tr('done'), failed: tr('failed'), stopped: tc('chat', 'stopped') }
  /** How long it has run, or ran. */
  const ran = (b: BackgroundTask) => (b.at ? dur(Math.max(0, (b.ended ? Date.parse(b.ended) : now) - Date.parse(b.at))) : '')
  const ICONS: Record<ConversationTool['kind'], IconName> = { read: 'files', write: 'edit', command: 'terminal', web: 'web', agent: 'group', other: 'code' }
  const time = (iso: string | null) => (iso ? new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : '')
  const toggle = (key: string) => (open[key] = !open[key])
  const detailed = (t: ConversationTool) => !!t.output || t.diff.length > 0 || !!t.sub
  /** A subagent still at work: its call under way, or its task in the background. */
  const busy = (t: ConversationTool) => t.state === 'running' || !!t.sub?.running
  /** A subagent's task in the background (Claude Code launches them so), found by its description. */
  const taskOf = (t: ConversationTool) => (t.what ? background.findLast((b) => b.kind === 'agent' && b.what === t.what) : undefined)
  /** How long a subagent has worked, or worked: its task's time when it ran in the background. */
  function worked(t: ConversationTool): string {
    const b = taskOf(t)
    if (b) return ran(b)
    if (t.state === 'running') return since(t.at)
    return t.ms !== null && t.ms >= 1000 ? dur(t.ms) : ''
  }
  /** A time: tenths under 10 s. */
  const dur = (ms: number) => (ms < 10_000 ? `${(ms / 1000).toFixed(1)}s` : span(ms))
  const since = (at: string | null) => (at ? dur(Math.max(0, now - Date.parse(at))) : '')
  const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : `${n}`)
  /** A folded run's counts, the failed ones apart. */
  function counts(entries: ConversationEntry[]): { text: string; failed: number; added: number; removed: number } {
    const s = sumSteps(entries)
    const parts = [
      s.read ? (s.read < 2 ? tr('{n} read', { n: s.read }) : tc('plural', '{n} read', { n: s.read })) : null,
      s.edited ? (s.edited < 2 ? tr('{n} edited', { n: s.edited }) : tc('plural', '{n} edited', { n: s.edited })) : null,
      s.commands ? tn(s.commands, '{n} command', '{n} commands') : null,
      s.others ? tn(s.others, '{n} other', '{n} others') : null,
    ]
    return { text: parts.filter(Boolean).join(' · '), failed: s.failed, added: s.added, removed: s.removed }
  }
  /**
   * A prompt's bubble: its words, and its images (those Prism attached, named by their path in
   * them; those pasted in its CLI, whose `[Image #n]` marks go).
   */
  function bubble(text: string, pasted?: PastedImages): { text: string; images: PromptImage[] } {
    const p = promptImages(text)
    const images: PromptImage[] = p.images.map((path) => ({ key: path, name: base(path), load: () => agentAttachment(id, path) }))
    if (!pasted) return { text: p.text, images }
    for (let n = 0; n < pasted.count; n++) images.push({ key: `#${n}`, name: tr('Image #{n}', { n: n + 1 }), load: () => agentPastedImage(id, pasted.line, n) })
    return { text: p.text.replace(/\[Image #\d+\] ?/g, '').trim(), images }
  }
  /** What a turn took, once over, and the model that answered it. */
  function took(turn: { at: string | null; last: string | null; tokens: number; cost: number | null; model: string | null }): string {
    const ms = turn.at && turn.last ? Date.parse(turn.last) - Date.parse(turn.at) : null
    return [ms !== null && ms >= 0 ? span(ms) : null, turn.tokens ? tr('{n} tokens', { n: tokens(turn.tokens) }) : null, turn.cost !== null ? `$${turn.cost.toFixed(2)}` : null, turn.model ? modelName(turn.model) : null].filter(Boolean).join(' · ')
  }
</script>

<svelte:window onkeydown={(e) => { if (drop && e.key === 'Escape') { e.stopPropagation(); drop = null } }} />

<!-- Your prompt as a bubble, the images it attaches shown in it. -->
{#snippet prompt(cls: string, text: string, pasted?: PastedImages)}
  {@const p = bubble(text, pasted)}
  <div class={cls}>{#if p.images.length}<PromptImages images={p.images} />{/if}{p.text}</div>
{/snippet}

{#snippet entryRow(row: Row)}
  {#if row.type === 'reads'}
    <button class="tool" onclick={() => toggle(row.key)} aria-expanded={!!open[row.key]}>
      <Icon name="files" /><b>{tr('Read')}</b><span class="what">{tn(row.tools.length, '{n} file', '{n} files')}</span>
      <span class="st ok">✓</span>
    </button>
    {#if open[row.key]}
      <div class="reads">{#each row.tools as t, i (i)}<div><b>{t.tool}</b> {t.what ?? ''}</div>{/each}</div>
    {/if}
  {:else if row.entry.type === 'text'}
    {#if commentAnswers && acts && commentable(row.entry.text)}
      <div class="ai"><CommentedAnswer {id} text={row.entry.text} {acts} {typing} /></div>
    {:else}
      <div class="ai"><Markdown text={row.entry.text} reading mrs /></div>
    {/if}
  {:else if row.entry.type === 'note'}
    <div class="note">{row.entry.text}</div>
  {:else if row.entry.type === 'thinking'}
    {@const th = row.entry}
    <button class="think" class:open={!!open[row.key]} onclick={() => th.text && toggle(row.key)} aria-expanded={th.text ? !!open[row.key] : undefined} disabled={!th.text}>
      <span class="tt">{tr('Thought')}{#if th.ms !== null && th.ms >= 1000}{` · ${dur(th.ms)}`}{/if}</span>
      {#if th.text}<span class="tx">{th.text}</span>{/if}
    </button>
  {:else if row.entry.proposal}
    {@const p = row.entry.proposal}
    <PlanCard {id} plan={p} previous={before(p.call)} latest={p.call === lastPlan} {acts} {typing} onwide={onplan} />
  {:else if row.entry.ask}
    <QuestionCard {id} ask={row.entry.ask} status={row.entry.state} {acts} {typing} {passkey} />
  {:else if row.entry.aside}
    <!-- A task flagged in passing (suggest_task): its card, where the agent saw it; withdrawn (dismiss_task): struck out. -->
    {@const call = row.entry}
    {@const line = asideOfCall(call)}
    {#if line && isSuggest(call)}<AsideCard a={line} chat />
    {:else if line}<AsideCard a={line} chat withdrawn />
    {:else}<div class="note">↗ {isSuggest(call) ? tr('In passing') : tr('Withdrawn')}: {call.what ?? ''}{call.state === 'failed' && call.output ? ` — ${call.output}` : ''}</div>{/if}
  {:else if row.entry.kind === 'agent'}
    {@const t = row.entry}
    {@const b = busy(t)}
    {@const shown = !!t.sub && (open[row.key] || b)}
    <div class="subagent" class:run={b} class:failed={t.state === 'failed'}>
      <button class="sh" onclick={() => t.sub && toggle(row.key)} aria-expanded={t.sub ? shown : undefined} title={t.sub ? (shown ? tr('Hide its calls') : tr('Show its calls')) : undefined}>
        <Icon name="group" /><b>{t.agent ?? tr('Subagent')}</b><span class="w">{t.what ?? ''}</span>
        {#if t.sub}<span class="n">{tn(t.sub.count, '{n} call', '{n} calls')}</span>{/if}
        {#if worked(t)}<span class="dur">{worked(t)}</span>{/if}
        {#if b}<span class="st running"><span class="dot"></span>{tr('running')}</span>{:else if t.state === 'failed' || taskOf(t)?.state === 'failed'}<span class="st ko">✗</span>{:else}<span class="st ok">✓</span>{/if}
      </button>
      {#if shown && t.sub}
        <div class="sub">
          {#if t.sub.count > t.sub.steps.length}<div class="more">{tr('{n} earlier calls', { n: t.sub.count - t.sub.steps.length })}</div>{/if}
          {#each t.sub.steps as s, i (i)}
            <div><b>{s.tool}</b><span class="m">{s.what ?? ''}</span>{#if s.state === 'running'}<span class="dot"></span>{:else if s.state === 'failed'}<span class="st ko">✗</span>{:else}<span class="st ok">✓</span>{/if}</div>
          {/each}
          {#if t.sub.said}<div class="said">{t.sub.said}</div>{/if}
        </div>
      {:else if t.sub?.said}
        <div class="said1">{t.sub.said}</div>
      {/if}
    </div>
  {:else}
    {@const t = row.entry}
    <button class="tool" class:failed={t.state === 'failed'} onclick={() => detailed(t) && toggle(row.key)} aria-expanded={detailed(t) ? !!open[row.key] : undefined}>
      <Icon name={ICONS[t.kind]} /><b>{t.tool}</b><span class="what" class:path={t.kind === 'write' || t.kind === 'read'}><bdi>{t.what ?? ''}</bdi></span>
      {#if t.added || t.removed}<span class="n"><i class="add">+{t.added}</i> <i class="del">−{t.removed}</i></span>{/if}
      {#if t.sub}<span class="n">{tn(t.sub.count, '{n} call', '{n} calls')}</span>{/if}
      {#if t.state === 'running' && t.at}<span class="dur">{since(t.at)}</span>{:else if t.ms !== null && t.ms >= 1000}<span class="dur">{dur(t.ms)}</span>{/if}
      {#if t.state === 'running'}<span class="st run"><span class="dot"></span></span>{:else if t.state === 'failed'}<span class="st ko">✗</span>{:else}<span class="st ok">✓</span>{/if}
    </button>
    {#if t === running && live?.output.length}
      <pre class="out">{live.output.join('\n')}</pre>
    {/if}
    {#if t.sub && (open[row.key] || t.state === 'running')}
      <div class="sub">
        {#if t.sub.count > t.sub.steps.length}<div class="more">{tr('{n} earlier calls', { n: t.sub.count - t.sub.steps.length })}</div>{/if}
        {#each t.sub.steps as s, i (i)}
          <div><b>{s.tool}</b><span class="m">{s.what ?? ''}</span>{#if s.state === 'running'}<span class="dot"></span>{:else if s.state === 'failed'}<span class="st ko">✗</span>{:else}<span class="st ok">✓</span>{/if}</div>
        {/each}
        {#if t.sub.said}<div class="said">{t.sub.said}</div>{/if}
      </div>
    {/if}
    {#if open[row.key]}
      {#if t.diff.length}
        <div class="diff">{#each t.diff as l, i (i)}<div class:add={l[0] === '+'} class:del={l[0] === '-'}>{l}</div>{/each}</div>
      {/if}
      {#if t.output}<pre class="out">{t.output}</pre>{/if}
    {/if}
  {/if}
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions : the links' clicks (Enter on a link clicks it too) -->
<div class="chat" class:wide onclick={onlink}>
  {#if planShown || (background.length && bgShown) || chain || (convo && convo.total > 1)}
    <div class="rail" role="toolbar" aria-label={tr('Its plan, background tasks, chain and turns')}>
      {#if planShown}
        <button class="rc" class:open={drop === 'plan'} onclick={() => show('plan')} aria-expanded={drop === 'plan'} title="{tr('Its plan')}{planNow ? `: ${planNow.text}` : ''}">
          <Icon name="queue" /><b>{planDone}/{plan.length}</b><span class="prog"><i style:width="{(planDone / plan.length) * 100}%"></i></span><span class="c">{planNow?.text ?? ''}</span>
        </button>
      {/if}
      {#if background.length && bgShown}
        {@const last = background.at(-1)}
        <button class="rc" class:open={bgOpen} onclick={() => show('bg')} aria-expanded={bgOpen} title={tr('Its background tasks: commands, subagents, monitors')}>
          {#if bgRunning.length}<span class="bgd"></span><b>{bgRunning.length}</b> {tr('in the background')}{:else}<Icon name="running" />{tr('Background')}{#if last}<span class="st {last.state}">{BG_STATE[last.state]}</span>{/if}{/if}
        </button>
      {/if}
      {#if chain}
        <button class="rc" class:open={drop === 'chain'} class:held={chainHeld} onclick={() => show('chain')} aria-expanded={drop === 'chain'} title={tr('Its chain: the steps after its work')}>
          ⛓ {chainHeld ? tr('Then: your go') : chainActive(chain) ? `${tr('Then')} ${chainDone}/${chain.steps.length}` : tr('Then: over')}
        </button>
      {/if}
      <span class="grow"></span>
      {#if convo && convo.total > 1}
        <button class="rc" class:open={drop === 'turns'} onclick={() => show('turns')} aria-expanded={drop === 'turns'} title={tr('Its turns, searched')}><Icon name="search" /><b>{convo.total}</b> {tr('turns')}</button>
      {/if}
    </div>
  {/if}
  {#if drop}
    <button class="scrim" onclick={() => (drop = null)} aria-label={tr('Close')} tabindex="-1"></button>
    <div class="drop" class:sheet={!wide} class:right={drop === 'turns'} role="dialog">
      {#if !wide}<div class="grab"></div>{/if}
      {#if drop === 'plan'}
        <div class="dh"><b>{tr('Plan')}</b><span>{tr('{done} of {total} done', { done: planDone, total: plan.length })}</span></div>
        <ul class="steps">
          {#each plan as p, i (i)}<li class={p.status}><span class="ck">{p.status === 'completed' ? '✓' : p.status === 'in_progress' ? '▸' : '○'}</span>{p.text}</li>{/each}
        </ul>
      {:else if drop === 'bg'}
        <div class="dh"><b>{tr('Background')}</b><span>{bgRunning.length ? tr('{n} running', { n: bgRunning.length }) : tr('none running')}</span></div>
        <ul class="tasks">
          {#each background as b, i (i)}
            <li class={b.state}>
              <span class="row">
                <Icon name={BG_ICONS[b.kind]} title={BG_LABEL[b.kind]} />
                <span class="w" title={b.what ?? ''}>{b.what ?? BG_LABEL[b.kind]}</span>
                {#if b.kind === 'monitor' && b.events}<span class="meta">{tn(b.events, '{n} event', '{n} events')}</span>{/if}
                <span class="meta">{ran(b)}</span>
                <span class="st {b.state}">{#if b.state === 'running'}<span class="dot"></span>{/if}{BG_STATE[b.state]}</span>
              </span>
              {#if b.summary}<span class="sum">{b.summary}</span>{/if}
            </li>
          {/each}
        </ul>
      {:else if drop === 'chain' && agent}
        <div class="dh"><b>{tr('Then')}</b><span>{tr('the steps after its work')}</span></div>
        <AgentChain {agent} />
      {:else if drop === 'turns' && convo}
        <!-- svelte-ignore a11y_autofocus : opened to search -->
        <input type="search" bind:value={query} placeholder={tr('Search the conversation')} enterkeyhint="search" autofocus={wide} />
        <div class="list">
          {#each [...found].reverse() as turn (turn.n)}
            {@const s = summarize(turn)}
            <button class="card" class:cur={turn === convo.turns[convo.turns.length - 1] && working} onclick={() => jump(turn.n)}>
              <span class="top"><b>#{turn.n}</b> {time(turn.at)}{#if s.ms !== null}{` · ${span(s.ms)}`}{/if}</span>
              <span class="p">{turn.prompt || tr('(its start not read)')}</span>
              {#if s.files.length || s.commands}
                <span class="chips">
                  {#each s.files.slice(0, 4) as f (f.path)}<i>{base(f.path)} <em class="add">+{f.added}</em> <em class="del">−{f.removed}</em></i>{/each}
                  {#if s.files.length > 4}<i>{tr('+{n} files', { n: s.files.length - 4 })}</i>{/if}
                  {#if s.commands}<i class:ko={s.failed > 0}>{tn(s.commands, '{n} command', '{n} commands')}{#if s.failed}{` · ${s.failed} ✗`}{/if}</i>{/if}
                </span>
              {/if}
              {#if s.last}<span class="sum">{s.last}</span>{/if}
            </button>
          {:else}
            <p class="muted">{tr('No turn says “{query}”.', { query })}</p>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
  <!-- On the computer, its text size (`[agents] chat_font_size`, 13.5 the base): the conversation scaled. -->
  <div class="pane" bind:this={pane} {onscroll} style:zoom={wide && chatFontSize() !== 13.5 ? chatFontSize() / 13.5 : undefined}>
    {#if convo === undefined && !error}
      <p class="muted">…</p>
    {:else if convo === null}
      {#if agent?.legs?.length}<ChatLegs {agent} />{/if}
      <p class="muted">{tr('No conversation to read yet: its CLI keeps none, or it has not started. The Terminal tab shows it.')}</p>
    {:else if convo}
      <!-- Earlier turns come as you scroll up (`earlier`); above them, the CLIs it ran before (Continue with…). -->
      {#if !more && agent?.legs?.length}<ChatLegs {agent} />{/if}
      {#if more}
        <p class="muted earlier">{older ? tr('Loading earlier turns…') : tr('↑ Earlier turns')}</p>
      {:else if convo.partial}
        <p class="muted">{tr('Its oldest turns are beyond what Prism reads.')}</p>
      {/if}
      {#each convo.turns as turn (turn.n)}
        {@const under = working && turn === convo.turns[convo.turns.length - 1]}
        <div class="turnh" id="turn-{turn.n}"><b>#{turn.n}</b> {time(turn.at)}<span class="ln"></span>{#if under}<span>{tr('under way')}</span>{:else if took(turn)}<span>{took(turn)}</span>{/if}</div>
        {#if turn.prompt}{@render prompt('me', turn.prompt, turn.pasted)}{/if}
        {#if !fold}
          {#each rows(turn.n, turn.entries) as row (row.key)}{@render entryRow(row)}{/each}
        {:else}
          {@const segs = segments(turn.entries)}
          {#each segs as seg, si (seg.type === 'steps' ? `s${seg.from}` : `o${seg.at}`)}
            {#if seg.type === 'one'}
              {@render entryRow({ type: 'entry', entry: seg.entry, key: `${turn.n}:${seg.at}` })}
            {:else}
              {@const rs = rows(turn.n, seg.entries, seg.from)}
              {#if rs.length === 1}
                {@render entryRow(rs[0])}
              {:else}
                {@const gk = `g${turn.n}:${seg.from}`}
                {@const isLive = under && si === segs.length - 1}
                {@const c = counts(seg.entries)}
                {@const shown = isLive ? (open[gk] ? rs : rs.slice(-LIVE_SHOWN)) : open[gk] ? rs : []}
                <div class="work" class:live={isLive} class:open={shown.length > 0}>
                  <button class="wh" onclick={() => toggle(gk)} aria-expanded={!!open[gk]} title={open[gk] ? tr('Fold its steps') : tr('Show each of its steps')}>
                    <b>{tn(seg.entries.length, '{n} step', '{n} steps')}</b>
                    <span class="bits">{c.text}{#if c.added || c.removed}{' '}<i class="add">+{c.added}</i> <i class="del">−{c.removed}</i>{/if}{#if c.failed}{' '}<i class="ko">{c.failed} ✗</i>{/if}</span>
                    {#if isLive}<span class="dot"></span>{/if}
                    <Icon name={open[gk] ? 'up' : 'down'} />
                  </button>
                  {#if shown.length}
                    <div class="in">
                      {#if isLive && !open[gk] && rs.length > LIVE_SHOWN}<button class="more" onclick={() => toggle(gk)}>{tr('{n} earlier', { n: rs.length - LIVE_SHOWN })}</button>{/if}
                      {#each shown as row (row.key)}{@render entryRow(row)}{/each}
                    </div>
                  {/if}
                </div>
              {/if}
            {/if}
          {/each}
        {/if}
      {/each}
      {#if pending}{@render prompt('me pending', pending)}{/if}
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>
  {#if !atBottom}<button class="down" onclick={toBottom} aria-label={tr('To the latest')}><Icon name="down" /></button>{/if}
  {#if working && convo !== undefined}
    <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events : the terminal's lines also have the Terminal tab -->
    <div class="status" class:peekable={tail.length > 0} onclick={(e) => tail.length && !(e.target as HTMLElement).closest('button') && (peek = !peek)}>
      <span class="dot"></span>
      <b>{live?.verb ?? tr('Working…')}</b>
      {#if live?.detail.length}<span class="meta">{live.detail.join(' · ')}</span>{/if}
      {#if convo?.model}<span class="meta" title={tr('The model answering')}>{modelName(convo.model)}</span>{/if}
      <span class="act">{#if running}<b>{running.tool}</b> <bdi>{running.what ?? ''}</bdi>{/if}</span>
      {#if tail.length}<span class="meta" aria-hidden="true">{peek ? '▾' : '▴'}</span>{/if}
      {#if acts}<button class="esc" onclick={stop} disabled={stopping} title={tr('Interrupt its turn: the agent stays, its prompt waits for you (Esc)')}><Icon name="pause" size={11} />{tr('Interrupt')}<kbd>Esc</kbd></button>{/if}
    </div>
    {#if peek && tail.length}
      <button class="live" onclick={onterminal} title={tr('The terminal: its menus')}>
        <span class="lbl"><span>{tr('Terminal · live')}</span><Icon name="expand" /></span>
        {#each tail as l, i (i)}<div class="tl">{l || ' '}</div>{/each}
      </button>
    {/if}
  {/if}
</div>

<style>
  /* Its glass (`--chat-blur`) on a layer behind it, as a block's (app.css). */
  .chat::before { content: ''; position: absolute; inset: 0; z-index: -1; backdrop-filter: var(--chat-blur); -webkit-backdrop-filter: var(--chat-blur); pointer-events: none; }
  .chat { flex: 1; min-height: 0; display: flex; flex-direction: column; position: relative; background: var(--chat-bg); color: var(--text); }
  .pane { flex: 1; min-height: 0; overflow-y: auto; -webkit-overflow-scrolling: touch; overscroll-behavior: contain; padding: 10px 12px 14px; display: flex; flex-direction: column; gap: 8px; }
  /* Scrolled, never squeezed. */
  .pane > :global(*) { flex-shrink: 0; }
  .card { display: flex; flex-direction: column; gap: 4px; width: 100%; text-align: left; font: inherit; color: var(--text); background: var(--chat-card); border: 1px solid var(--border); border-radius: 12px; padding: 9px 11px; }
  .card.cur { border-color: var(--warn); }
  .card .top { font-size: 11.5px; color: var(--text-muted); }
  .card .top b { color: var(--text); }
  .card .p { font-size: 14px; font-weight: 600; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .card .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .card .chips i { font-style: normal; font-size: 11.5px; padding: 1px 7px; border-radius: 7px; background: var(--surface-muted); border: 1px solid var(--border); color: var(--text-muted); }
  .card .chips i.ko { color: var(--ko); }
  .card em { font-style: normal; }
  .card .add { color: var(--ok); } .card .del { color: var(--ko); }
  .card .sum { font-size: 13px; color: var(--text-muted); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .muted, .error { margin: 12px 4px; font-size: 13px; color: var(--text-muted); }
  .error { color: var(--ko); }
  .earlier { align-self: center; margin: 0; font-size: 12px; }
  .me { align-self: flex-end; max-width: 85%; background: var(--accent); color: var(--on-accent); padding: 7px 12px; border-radius: 16px 16px 4px 16px; font-size: 14.5px; line-height: 1.4; white-space: pre-wrap; overflow-wrap: anywhere; }
  .me.pending { opacity: 0.6; }
  .ai { font-size: 14.5px; line-height: 1.5; overflow-wrap: anywhere; }
  .ai :global(p:first-child) { margin-top: 0; }
  .ai :global(p:last-child) { margin-bottom: 0; }
  .ai :global(pre) { overflow-x: auto; font-size: 12px; }
  .note { align-self: center; font-size: 12px; color: var(--text-muted); font-style: italic; }
  .tool { display: flex; align-items: center; gap: 7px; width: 100%; text-align: left; font: inherit; font-size: 13px; color: var(--text-muted); background: var(--chat-card); border: 1px solid var(--border); border-radius: 10px; padding: 6px 10px; min-width: 0; }
  .tool b { color: var(--text); font-weight: 600; flex: none; }
  .tool .what { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: ui-monospace, 'SF Mono', Menlo, monospace; font-size: 12px; }
  /* A path cut at its start: its file name shows. */
  .tool .what.path { direction: rtl; text-align: left; }
  .tool .n { flex: none; font-size: 12px; }
  .tool i { font-style: normal; }
  .tool .add { color: var(--ok); } .tool .del { color: var(--ko); }
  .tool .st { flex: none; font-weight: 700; }
  .st.ok { color: var(--ok); } .st.ko { color: var(--ko); }
  .tool.failed { border-color: color-mix(in srgb, var(--ko) 45%, var(--border)); }
  .reads { margin: -4px 0 0 30px; font: 12px/1.5 ui-monospace, 'SF Mono', Menlo, monospace; color: var(--text-muted); overflow-wrap: anywhere; }
  .reads b { color: var(--text); font-weight: 600; font-family: var(--sans); }
  .diff { margin-top: -4px; border: 1px solid var(--border); border-radius: 10px; overflow: hidden; background: var(--chat-card); font: 11.5px/1.45 ui-monospace, 'SF Mono', Menlo, monospace; }
  .diff div { padding: 0 10px; white-space: pre-wrap; overflow-wrap: anywhere; color: var(--text-muted); }
  .diff .add { background: var(--ok-bg); color: var(--ok); }
  .diff .del { background: color-mix(in srgb, var(--ko) 14%, transparent); color: var(--ko); }
  .out { margin: -4px 0 0; padding: 8px 10px; border-radius: 10px; background: var(--chat-code); color: var(--text); font: 11.5px/1.45 ui-monospace, 'SF Mono', Menlo, monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
  .tool .dur { flex: none; font: 11px ui-monospace, 'SF Mono', Menlo, monospace; color: var(--text-muted); }
  .think { display: flex; flex-direction: column; gap: 2px; width: 100%; text-align: left; font: inherit; border: none; border-left: 2px solid var(--violet, #7b68ee); background: none; color: var(--text-muted); padding: 1px 0 1px 10px; }
  .think:disabled { cursor: default; }
  .think .tt { font-size: 11.5px; font-weight: 600; color: var(--violet, #7b68ee); }
  .think .tx { font-size: 12.5px; font-style: italic; line-height: 1.45; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .think.open .tx { white-space: pre-wrap; overflow-wrap: anywhere; }
  .subagent { border: 1px solid var(--border); border-left: 3px solid color-mix(in srgb, var(--accent) 55%, var(--border)); border-radius: 10px; background: var(--chat-card); overflow: hidden; }
  .subagent.run { border-left-color: var(--info); }
  .subagent.failed { border-left-color: var(--ko); }
  .subagent .sh { display: flex; align-items: center; gap: 7px; width: 100%; text-align: left; font: inherit; font-size: 13px; color: var(--text-muted); background: none; border: none; padding: 6px 10px; min-width: 0; }
  .subagent .sh b { color: var(--text); font-weight: 600; flex: none; }
  .subagent .sh .w { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .subagent .sh .n { flex: none; font-size: 12px; }
  .subagent .sh .dur { flex: none; font: 11px ui-monospace, 'SF Mono', Menlo, monospace; }
  .subagent .sh .st { flex: none; font-weight: 700; }
  .subagent .sh .st.running { display: inline-flex; align-items: center; gap: 4px; font-size: 11px; font-weight: 500; padding: 0 7px; border-radius: 7px; }
  .subagent .sh .st.running .dot { width: 6px; height: 6px; background: var(--info); }
  .subagent .sub { margin: 0 10px 6px 18px; }
  .subagent .said1 { padding: 0 10px 7px 31px; font-size: 12px; font-style: italic; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { margin: -4px 0 0 18px; padding: 4px 10px 6px; border-left: 1px dashed var(--border); font-size: 12px; color: var(--text-muted); display: flex; flex-direction: column; gap: 1px; }
  .sub > div { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .sub b { color: var(--text); font-weight: 600; flex: none; }
  .sub .m { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: 11.5px ui-monospace, 'SF Mono', Menlo, monospace; }
  .sub .more { font-size: 11px; }
  .sub .said { display: block; font-style: italic; padding-top: 3px; overflow-wrap: anywhere; }
  .tasks li { display: flex; flex-direction: column; gap: 1px; padding: 3px 0; color: var(--text); }
  .tasks .row { display: flex; align-items: center; gap: 7px; min-width: 0; }
  .tasks .row :global(svg) { flex: none; color: var(--text-muted); }
  .tasks .w { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tasks .meta { flex: none; font-size: 11.5px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .tasks .sum { padding-left: 21px; font-size: 11.5px; color: var(--text-muted); overflow-wrap: anywhere; }
  .tasks li.completed .w, .tasks li.stopped .w { color: var(--text-muted); }
  /* The rail: its plan, background tasks, chain and turns, one chip each. */
  .rail { flex: none; display: flex; align-items: center; gap: 6px; padding: 5px 12px; background: var(--chat-bar); border-bottom: 1px solid var(--border); min-width: 0; overflow-x: auto; scrollbar-width: none; }
  .rail::-webkit-scrollbar { display: none; }
  .rail::-webkit-scrollbar { display: none; }
  .rc { flex: none; display: inline-flex; align-items: center; gap: 6px; height: 24px; max-width: 280px; padding: 0 10px; border: 1px solid var(--border); border-radius: 12px; background: var(--chat-card); color: var(--text-muted); font: inherit; font-size: 11.5px; white-space: nowrap; cursor: pointer; }
  .rc b { color: var(--text); font-weight: 600; }
  .rc .c { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .rc.open { border-color: var(--accent); color: var(--text); }
  .rc.held { border-color: var(--warn); color: var(--warn); }
  .rc :global(svg) { flex: none; }
  .rc .prog { flex: none; width: 36px; height: 4px; border-radius: 2px; background: var(--surface-muted); overflow: hidden; }
  .rc .prog i { display: block; height: 100%; background: var(--ok); transition: width 0.4s; }
  .bgd { width: 6px; height: 6px; border-radius: 50%; background: var(--info); animation: pulse 1.2s infinite; }
  .grow { flex: 1; }
  .rc .st, .tasks .st { flex: none; display: inline-flex; align-items: center; gap: 4px; font-size: 11px; padding: 0 7px; border-radius: 7px; background: var(--surface-muted); color: var(--text-muted); }
  .st.running { background: var(--info-bg); color: var(--info); }
  .st.completed { background: var(--ok-bg); color: var(--ok); }
  .st.failed { background: color-mix(in srgb, var(--ko) 14%, transparent); color: var(--ko); }
  .tasks .st .dot { width: 6px; height: 6px; background: var(--info); }
  /* A chip's popover (a sheet on a phone), over the conversation: a click aside or Esc closes it. */
  .scrim { position: absolute; inset: 0; z-index: 4; border: none; padding: 0; background: transparent; cursor: default; }
  .drop { position: absolute; z-index: 5; top: 34px; left: max(12px, calc((100% - 820px) / 2)); width: 380px; max-width: calc(100% - 24px); max-height: 60%; display: flex; flex-direction: column; gap: 6px; overflow: hidden; padding: 9px 11px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.18); font-size: 12.5px; }
  .drop.right { left: auto; right: max(12px, calc((100% - 820px) / 2)); }
  .drop.sheet { top: auto; left: 0; right: 0; bottom: 0; width: auto; max-width: none; max-height: 75%; border-radius: 18px 18px 0 0; border-bottom: none; padding: 8px 14px calc(14px + env(safe-area-inset-bottom)); font-size: 14px; box-shadow: 0 -8px 30px rgba(0, 0, 0, 0.25); }
  .chat:not(.wide) .scrim { background: rgba(0, 0, 0, 0.3); }
  .grab { flex: none; width: 38px; height: 5px; border-radius: 3px; background: var(--border); margin: 0 auto 4px; }
  .dh { flex: none; display: flex; justify-content: space-between; align-items: baseline; gap: 10px; }
  .dh span { color: var(--text-muted); font-size: 11.5px; }
  .drop ul { list-style: none; margin: 0; padding: 0; overflow-y: auto; }
  .steps li { display: flex; gap: 7px; padding: 2px 0; color: var(--text-muted); }
  .steps li.completed { text-decoration: line-through; opacity: 0.7; }
  .steps li.in_progress { color: var(--text); font-weight: 600; }
  .steps .ck { flex: none; width: 14px; text-align: center; }
  .drop input { flex: none; width: 100%; border: 1px solid var(--border); background: var(--surface-muted); color: var(--text); border-radius: 8px; padding: 5px 10px; font: inherit; }
  .sheet input { font-size: 16px; padding: 8px 12px; border-radius: 12px; }
  .list { flex: 1; min-height: 0; overflow-y: auto; overscroll-behavior: contain; display: flex; flex-direction: column; gap: 6px; }
  .list > :global(*) { flex-shrink: 0; }
  .drop :global(.chain) { padding: 0; }
  /* A turn's line: its number and time, what it took. */
  .turnh { display: flex; align-items: center; gap: 7px; margin-top: 6px; font-size: 11px; color: var(--text-muted); }
  .turnh b { color: var(--text); font-weight: 600; }
  .turnh .ln { flex: 1; height: 1px; background: var(--border); }
  /* Its steps between two answers, folded. */
  .work { border: 1px solid var(--border); border-radius: 10px; background: var(--chat-card); overflow: hidden; }
  .work.live { border-color: color-mix(in srgb, var(--warn) 40%, var(--border)); }
  .wh { display: flex; align-items: center; gap: 8px; width: 100%; min-height: 30px; padding: 5px 10px; border: none; background: none; color: var(--text-muted); font: inherit; font-size: 12.5px; text-align: left; cursor: pointer; }
  .wh b { flex: none; color: var(--text); font-weight: 600; }
  .wh .bits { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wh i { font-style: normal; }
  .wh .add { color: var(--ok); } .wh .del, .wh .ko { color: var(--ko); }
  .wh :global(svg) { flex: none; }
  .work .in { display: flex; flex-direction: column; gap: 6px; padding: 7px 8px 8px; border-top: 1px solid var(--border); background: var(--chat-code); }
  .work .in > :global(*) { flex-shrink: 0; }
  .work .more { align-self: flex-start; border: none; background: none; padding: 0 2px; color: var(--info); font: inherit; font-size: 11.5px; cursor: pointer; }
  .chat:not(.wide) .wh { min-height: 40px; font-size: 13px; }
  /* A phone: every chip in its width. */
  .chat:not(.wide) .rail { gap: 5px; padding: 5px 8px; }
  .chat:not(.wide) .rc { padding: 0 8px; }
  .chat:not(.wide) .rc .c { max-width: 90px; }
  .status.peekable { cursor: pointer; }
  .status { flex: none; display: flex; align-items: center; gap: 8px; padding: 6px 12px; background: var(--chat-bar); border-top: 1px solid var(--border); font-size: 12.5px; min-width: 0; }
  .status .meta { flex: none; color: var(--text-muted); font: 11.5px ui-monospace, 'SF Mono', Menlo, monospace; }
  .status .act { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: 12px; }
  .status .act b { color: var(--text); font-weight: 600; }
  /* A button, not a key hint: framed, a pointer, warned on hover; its key beside its word. */
  .status .esc { flex: none; display: inline-flex; align-items: center; gap: 5px; border: 1px solid color-mix(in srgb, var(--text-muted) 55%, transparent); background: var(--surface-muted); color: var(--text); border-radius: 6px; padding: 2px 8px; font: inherit; font-size: 11.5px; font-weight: 500; cursor: pointer; box-shadow: 0 1px 0 color-mix(in srgb, var(--text) 8%, transparent); }
  .status .esc:hover:not(:disabled) { border-color: var(--warn); color: var(--warn); }
  .status .esc:disabled { opacity: 0.5; cursor: default; }
  .status .esc kbd { padding: 0 4px; border-radius: 3px; background: var(--surface-muted); color: var(--text-muted); font: 10px ui-monospace, 'SF Mono', Menlo, monospace; }
  /* No Esc key on a phone. */
  :global(html.remote) .status .esc kbd { display: none; }
  .chat:not(.wide) .status { flex-wrap: wrap; row-gap: 2px; }
  .chat:not(.wide) .status .act { flex-basis: 100%; order: 9; }
  .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--warn); animation: pulse 1.2s infinite; }
  @keyframes pulse { 50% { opacity: 0.3; } }
  .down { position: absolute; right: 14px; bottom: 96px; width: 36px; height: 36px; border-radius: 50%; border: 1px solid var(--border); background: var(--surface); color: var(--info); box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25); display: grid; place-items: center; }
  .live { display: block; width: 100%; text-align: left; border: none; border-top: 1px solid var(--border); background: var(--chat-code); color: var(--text); padding: 5px 10px 7px; font: 11px/1.4 ui-monospace, 'SF Mono', Menlo, monospace; }
  .live .lbl { display: flex; justify-content: space-between; align-items: center; font: 600 10px var(--sans); letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-muted); margin-bottom: 2px; }
  /* On the computer: a column to read, in a pane of any width. */
  .wide .pane { padding-left: max(16px, calc((100% - 820px) / 2)); padding-right: max(16px, calc((100% - 820px) / 2)); }
  .wide .me, .wide .ai { font-size: 13.5px; }
  .wide .status, .wide .rail { padding-left: max(12px, calc((100% - 820px) / 2)); padding-right: max(12px, calc((100% - 820px) / 2)); }
  .wide .down { bottom: 16px; }
  .tl { white-space: pre; overflow: hidden; text-overflow: ellipsis; }
</style>
