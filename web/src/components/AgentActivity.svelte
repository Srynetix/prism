<script lang="ts">
  import { isDesktop } from '../bridge'
  import { locale } from '../locale'
  import { agentActivity, insertInAgent, interruptAgent, type AgentActivity } from '../agents.svelte'
  import type { Agent } from '../types'
  import AgentLegs from './AgentLegs.svelte'
  import { pollsShown } from '../polls.svelte'
  import { t, tn, tc } from '../i18n.svelte'

  /**
   * The Activity tab: what the agent did, from its hook log (metadata only: tool, file, command,
   * duration, failure, permission waits). A summary, a strip of the time spent on each kind, the
   * timeline (filtered) by turn, from your prompt to its end, each folded or shown with its header,
   * the files touched; and, when it runs the same failing command again and again, a word to stop
   * it or redirect it.
   */
  let { agent }: { agent: Agent } = $props()

  let act = $state<AgentActivity | null>(null)
  let error = $state('')
  async function load() {
    try {
      act = await agentActivity(agent.id)
      error = ''
    } catch (e) {
      error = String(e)
    }
  }
  // Read again when the agent moves (its last activity), and every few seconds while it runs.
  // Hidden (another view, the window): read again once shown.
  const shown = pollsShown()
  $effect(() => {
    void agent.last_activity
    if (shown()) void load()
  })
  $effect(() => {
    if (agent.state !== 'running' || !shown()) return
    const timer = setInterval(load, 4000)
    return () => clearInterval(timer)
  })

  type Filter = 'all' | 'files' | 'commands' | 'waits'
  let filter = $state<Filter>('all')
  type Item = AgentActivity['items'][number]
  const keep = (i: Item) =>
    filter === 'all' ? true : filter === 'files' ? i.kind === 'read' || i.kind === 'write' : filter === 'commands' ? i.kind === 'command' : i.kind === 'wait' || i.kind === 'stop'

  /**
   * The turns: each opens with your prompt (the first, with the log, before any) and ends with its
   * "Turn over". The prompt is the turn's header, its other items the detail, filtered; a filter
   * leaves out the turns it empties, except "All" and "Waits & turns", which show every turn. The
   * log lists the newest first: the turns are cut in the order of time, then shown newest first too.
   */
  interface Turn { key: number; prompt: boolean; items: Item[]; shown: Item[]; actions: number; written: number; failed: number; ms: number | null; over: boolean }
  const turns = $derived.by(() => {
    const out: Turn[] = []
    for (const i of [...(act?.items ?? [])].sort((a, b) => a.t - b.t)) {
      if (i.kind === 'prompt' || !out.length) out.push({ key: i.t, prompt: i.kind === 'prompt', items: [], shown: [], actions: 0, written: 0, failed: 0, ms: null, over: false })
      const turn = out[out.length - 1]
      if (i.kind !== 'prompt') turn.items.push(i)
    }
    for (const [n, turn] of out.entries()) {
      const done = turn.items.filter((i) => i.kind !== 'wait' && i.kind !== 'stop')
      turn.actions = done.length
      turn.written = done.filter((i) => i.kind === 'write').length
      turn.failed = done.filter((i) => i.failed).length
      turn.over = turn.items.some((i) => i.kind === 'stop')
      const end = Math.max(turn.key, ...turn.items.map((i) => i.t + (i.ms ?? 0)))
      // Running: the last turn, still going on; an earlier one without its end was interrupted.
      turn.ms = turn.over || agent.state !== 'running' || n < out.length - 1 ? end - turn.key : null
      turn.shown = turn.items.filter(keep).reverse()
    }
    out.reverse()
    return filter === 'all' || filter === 'waits' ? out : out.filter((tu) => tu.shown.length)
  })

  /** Folded turns, by their start: kept while the log is read again. */
  let folded = $state<Set<number>>(new Set())
  const toggle = (key: number) => {
    const next = new Set(folded)
    if (!next.delete(key)) next.add(key)
    folded = next
  }
  const allFolded = $derived(turns.length > 0 && turns.every((tu) => folded.has(tu.key)))
  const foldAll = () => (folded = allFolded ? new Set() : new Set(turns.map((tu) => tu.key)))
  const ICON: Record<string, string> = { read: '👁', write: '✎', command: '▶', web: '🌐', agent: '✳', other: '·', wait: '⏸', prompt: '›', stop: '■' }
  const clock = (t: number) => new Date(t).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })
  const dur = (ms: number) => (ms < 1000 ? `${ms} ms` : ms < 60_000 ? `${Math.round(ms / 1000)}s` : `${Math.floor(ms / 60_000)}m${String(Math.round((ms % 60_000) / 1000)).padStart(2, '0')}`)
  const mins = (ms: number) => (ms < 60_000 ? `${Math.round(ms / 1000)}s` : `${Math.round(ms / 60_000)}m`)
  const label = (i: Item) =>
    i.kind === 'prompt' ? t('Your prompt') : i.kind === 'stop' ? t('Turn over') : i.kind === 'wait' ? t('Permission: {tool}', { tool: `${i.tool ?? t('a tool')}${i.what ? ` ${i.what}` : ''}` }) : (i.path ?? i.what ?? i.tool ?? '')

  /** The strip, from the log's first event to its last (or now, while running). */
  const span = $derived.by(() => {
    if (!act?.from) return null
    const to = agent.state === 'running' ? Math.max(Date.now(), act.to ?? 0) : (act.to ?? Date.now())
    return { from: act.from, to: Math.max(to, act.from + 1) }
  })
  const pct = (t: number) => (span ? ((t - span.from) / (span.to - span.from)) * 100 : 0)

  let noted = $state(false)
  async function note() {
    await insertInAgent(agent.id, 'Step back: the same command keeps failing. Explain the failure first, then change your approach.')
    noted = true
  }
</script>

<div class="act">
  <AgentLegs {agent} />
  {#if error}<p class="empty">{error}</p>
  {:else if !act}<p class="empty">{t('Reading its log…')}</p>
  {:else if !act.items.length}<p class="empty">{t('Nothing yet: its activity shows here as it works.')}</p>
  {:else}
    <div class="sum">
      <span><b>{act.summary.read}</b> {tn(act.summary.read, 'file read', 'files read')}</span>
      <span><b>{act.summary.written}</b> {act.summary.written < 2 ? t('written') : tc('plural', 'written')}</span>
      <span><b>{act.summary.commands}</b> {tn(act.summary.commands, 'command', 'commands')}{#if act.summary.failed}{' '}· <span class="ko">{t('{n} failed', { n: act.summary.failed })}</span>{/if}</span>
      {#if act.summary.waits}<span><b>{act.summary.waits}</b> {tn(act.summary.waits, 'wait', 'waits')} · {mins(act.summary.waited_ms)}</span>{/if}
    </div>
    {#if span}
      <div class="strip" title={t('Reading · writing · commands · web · subagents · waiting for you')}>
        {#each act.strip as s, i (i)}<i class="k-{s.kind}" style:left="{pct(s.from)}%" style:width="{Math.max(0.4, pct(s.to) - pct(s.from))}%"></i>{/each}
      </div>
      <div class="axis"><span>{clock(span.from)}</span><span>{agent.state === 'running' ? t('now') : clock(span.to)}</span></div>
    {/if}
    {#if agent.looping}
      <div class="stuck">
        <b>{t('Going in circles?')}</b> <code>{agent.looping.command}</code> {t('failed {n} times in a row, nothing written in between.', { n: agent.looping.count })}
        <div class="row">
          <button onclick={() => interruptAgent(agent.id)}>{t('Interrupt')}</button>
          {#if isDesktop()}<button class="primary" onclick={note} disabled={noted} title={t('Typed in its prompt, not sent: complete it, then Enter')}>{noted ? t('Note typed in its prompt') : t('Note: step back, explain the failure')}</button>{/if}
        </div>
      </div>
    {/if}
    <div class="filters" role="radiogroup" aria-label={t('Show')}>
      {#each [['all', t('All')], ['files', t('Files')], ['commands', t('Commands')], ['waits', t('Waits & turns')]] as const as [f, name] (f)}
        <button role="radio" aria-checked={filter === f} class:on={filter === f} onclick={() => (filter = f)}>{name}</button>
      {/each}
      {#if turns.length > 1}<button class="fold" onclick={foldAll}>{allFolded ? t('Expand all') : t('Collapse all')}</button>{/if}
    </div>
    <div class="items">
      {#each turns as turn (turn.key)}
        {@const open = !folded.has(turn.key)}
        <button class="turn" class:open aria-expanded={open} onclick={() => toggle(turn.key)} title={open ? t('Fold this turn') : t('Show this turn')}>
          <span class="t">{clock(turn.key)}</span>
          <span class="ic" aria-hidden="true">{open ? '▾' : '▸'}</span>
          <span class="w">{turn.prompt ? t('Your prompt') : tc('turn', 'Start')}</span>
          <span class="x">
            {tn(turn.actions, '{n} action', '{n} actions')}{#if turn.written}{' '}· {turn.written < 2 ? t('{n} written', { n: turn.written }) : tc('plural', '{n} written', { n: turn.written })}{/if}{#if turn.failed}{' '}· <span class="ko">{t('{n} failed', { n: turn.failed })}</span>{/if}
            · {#if turn.ms === null}<span class="run">{t('running…')}</span>{:else}{dur(turn.ms)}{/if}
          </span>
        </button>
        {#if open}
          {#each turn.shown as i, n (n)}
            <div class="it k-{i.kind}" class:failed={i.failed} class:wait={i.kind === 'wait'} title={i.path ?? i.what ?? ''}>
              <span class="t">{clock(i.t)}</span>
              <span class="ic" aria-hidden="true">{ICON[i.kind]}</span>
              <span class="w">{#if i.kind === 'command' || i.kind === 'read' || i.kind === 'write'}<code>{label(i)}</code>{:else}{label(i)}{/if}</span>
              <span class="x">{#if i.failed}<span class="ko">{t('failed')} · </span>{/if}{#if i.ms === null}<span class="run">{i.kind === 'wait' ? t('waiting for you') : t('running…')}</span>{:else if i.ms > 0}{i.kind === 'wait' ? t('you, {time} later', { time: dur(i.ms) }) : dur(i.ms)}{/if}</span>
            </div>
          {/each}
        {/if}
      {/each}
    </div>
    {#if act.files.length}
      <details class="files" open={act.files.length <= 6}>
        <summary>{t('Files touched')} · {act.files.length}</summary>
        {#each act.files.slice(0, 40) as f (f.path)}
          <div class="file"><code title={f.path}>{f.path}</code><span>{#if f.reads}{t('read {n}', { n: f.reads })}{/if}{#if f.reads && f.writes}{' '}·{' '}{/if}{#if f.writes}<b>{t('written {n}', { n: f.writes })}</b>{/if}</span></div>
        {/each}
      </details>
    {/if}
  {/if}
</div>

<style>
  .act { flex: 1; min-height: 0; display: flex; flex-direction: column; font-size: var(--fs-meta); }
  .empty { margin: 0; padding: 14px; color: var(--text-muted); }
  .sum { display: flex; flex-wrap: wrap; gap: 5px; padding: 8px 10px; border-bottom: 1px solid var(--border); }
  .sum > span { padding: 1px 7px; border-radius: var(--r5); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }
  .sum b { color: var(--text); }
  .ko { color: var(--ko); }
  .strip { position: relative; height: 10px; margin: 8px 10px 2px; border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .strip i { position: absolute; top: 0; bottom: 0; }
  .k-read { --k: #6f8fb8; }
  .k-write { --k: var(--src-claude); }
  .k-command { --k: var(--ok); }
  .k-web { --k: var(--info); }
  .k-agent { --k: var(--accent); }
  .k-other { --k: var(--text-muted); }
  .k-wait { --k: var(--warn); }
  .strip i { background: var(--k); }
  .axis { display: flex; justify-content: space-between; margin: 0 10px 6px; color: var(--text-muted); font: 9.5px var(--mono); }
  .stuck { margin: 6px 10px; padding: 7px 9px; border: 1px solid color-mix(in srgb, var(--warn) 50%, var(--border)); border-radius: var(--r7); background: color-mix(in srgb, var(--warn) 8%, transparent); line-height: 1.45; }
  .stuck b { color: var(--warn); }
  .stuck code { font-family: var(--mono); font-size: var(--fs-tag); }
  .row { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 6px; }
  .row button, .filters button { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .row button.primary { border-color: color-mix(in srgb, var(--src-claude) 60%, var(--border)); color: var(--src-claude); }
  .row button:disabled { opacity: 0.6; cursor: default; }
  .filters { display: flex; flex-wrap: wrap; gap: 4px; padding: 6px 10px 4px; }
  .filters button { border-radius: var(--r9); color: var(--text-muted); }
  .filters button.on { border-color: var(--src-claude); color: var(--text); }
  .filters .fold { margin-left: auto; border-color: transparent; background: none; }
  .filters .fold:hover { color: var(--text); }
  .items { flex: 1; min-height: 0; overflow: auto; padding-bottom: 6px; }
  .it { display: grid; grid-template-columns: max-content 16px minmax(0, 1fr) auto; gap: 0 6px; align-items: baseline; padding: 2px 10px; }
  .it:hover, .turn:hover { background: var(--surface-muted); }
  .turn { display: grid; grid-template-columns: max-content 16px minmax(0, 1fr) auto; gap: 0 6px; align-items: baseline; width: 100%; margin-top: 4px; padding: 3px 10px; border: 0; border-top: 1px solid var(--border); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .turn:first-child { margin-top: 0; border-top: 0; }
  .turn .w { font-weight: 600; }
  .turn .ic { color: var(--text-muted); }
  .it { padding-left: 16px; }
  .it.wait { background: color-mix(in srgb, var(--warn) 7%, transparent); }
  /* Two-digit and monospace (09:05, 09:05 AM): as wide on every row, so the columns line up unwrapped. */
  .it .t, .turn .t { color: var(--text-muted); font: 10px var(--mono); white-space: nowrap; }
  .it .ic { color: var(--k); text-align: center; font-size: 10px; }
  .it .w, .turn .w { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .it .w code { font-family: var(--mono); font-size: 10.5px; }
  .it.failed .w code { color: var(--ko); }
  .it .x, .turn .x { color: var(--text-muted); font-size: var(--fs-tag); white-space: nowrap; }
  .run { color: var(--info); }
  .files { padding: 4px 10px 8px; border-top: 1px solid var(--border); }
  .files summary { padding: 3px 0; color: var(--text-muted); font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; cursor: pointer; }
  .file { display: flex; gap: 8px; padding: 1px 0; color: var(--text-muted); font-size: var(--fs-tag); }
  .file code { flex: 1; min-width: 0; overflow: hidden; color: var(--text); font-family: var(--mono); font-size: 10.5px; text-overflow: ellipsis; white-space: nowrap; }
  .file b { color: var(--src-claude); font-weight: 500; }
</style>
