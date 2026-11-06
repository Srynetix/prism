<script lang="ts">
  import { clickedOutside } from '../drawers'
  import { isPinned } from '../drawer-pins'
  import PinButton from './PinButton.svelte'
  import { t } from '../i18n.svelte'
  import { tick } from 'svelte'
  import { closePodLogs, loadOlder, podLogs, reload, selectPod, setContainer, setPrevious } from '../pod-logs.svelte'
  import Icon from './Icon.svelte'
  import { LogLines, type Line } from '../ansi'
  import { highlight } from '../log-highlight'
  import { ago } from '../time'
  import { pref, setPref } from '../layout.svelte'
  import { live } from '../live.svelte'
  import { isDesktop } from '../bridge'
  import { openPodShell } from '../shell-drawer.svelte'
  import PodUsage from './PodUsage.svelte'
  import { changesOn, openWhatChanged } from '../changes-view.svelte'

  /**
   * A workload's log drawer (Environments block): its pods on the left, on the right the log of the
   * chosen container, the current one or the one before the last restart ("Previous", for a
   * crash or an OOMKilled). A filter keeps only the lines that contain it. Without ANSI
   * codes, each line gets syntax highlighting (log-highlight.ts) and error lines
   * are emphasised. A cut log reads further back once scrolled to its top, up to the container's
   * start, where it was scrolled staying in place. ⤢ or "f" switches to full screen (remembered under `logs.full`). Escape closes.
   */
  let query = $state('')
  /**
   * Parsed as it slides (its last lines kept, polled): a line still there keeps its object and its
   * key, so a poll renders only the new ones (the 2000 lines again were 53 ms, measured).
   * Another pod, container or previous log, another reading.
   */
  const reading = $derived.by(() => (void `${podLogs.pod}/${podLogs.container}/${podLogs.previous}`, new LogLines()))
  const lines = $derived(reading.parse(podLogs.log, podLogs.grown))
  const q = $derived(query.trim().toLowerCase())
  const ERROR = /\b(error|fatal|exception|traceback|panic|critical)\b/i
  const WARN = /\bwarn(ing)?\b/i
  /** A line's text and what it says, worked out once: a log read back to its start is 200,000 lines, polled. */
  const facts = new WeakMap<Line, { text: string; level: string; error: boolean }>()
  function fact(l: Line) {
    let f = facts.get(l)
    if (!f) {
      const text = l.segs.map((s) => s.text).join('')
      const error = ERROR.test(text)
      f = { text, error, level: l.segs.some((s) => s.fg) ? '' : error ? 'lv-err' : WARN.test(text) ? 'lv-warn' : '' }
      facts.set(l, f)
    }
    return f
  }
  const shown = $derived(q ? lines.map((l, i) => ({ l, i })).filter(({ l }) => fact(l).text.toLowerCase().includes(q)) : lines.map((l, i) => ({ l, i })))
  const level = (l: Line) => fact(l).level
  const firstError = $derived(shown.find(({ l }) => fact(l).error)?.i ?? -1)
  const pod = $derived(podLogs.pods.find((p) => p.name === podLogs.pod) ?? null)
  /** A pod's name without its workload's prefix; a block item's name (its "pod" is its id). */
  const short = (name: string) =>
    podLogs.target?.read ? (podLogs.pods.find((p) => p.name === name)?.containers[0] ?? name) : podLogs.target?.deployment && name.startsWith(`${podLogs.target.deployment}-`) ? name.slice(podLogs.target.deployment.length + 1) : name

  let view: HTMLElement | undefined = $state()
  /** Stuck to the bottom: a growing log stays followed. Reset to true on each pod change. */
  let follow = true
  let shownKey = ''
  $effect(() => {
    void shown
    const key = `${podLogs.pod}/${podLogs.container}/${podLogs.previous}`
    if (key !== shownKey) {
      shownKey = key
      follow = true
    }
    if (follow) tick().then(() => view?.scrollTo({ top: view.scrollHeight }))
  })
  function onScroll() {
    if (!view) return
    follow = view.scrollHeight - view.scrollTop - view.clientHeight < 40
    // Not while filtering: the few lines kept would bring the whole log, step by step.
    if (view.scrollTop < 200 && !q && !follow) older()
  }
  /** Older lines above, what was on screen staying there. */
  async function older() {
    if (!view || !podLogs.truncated || podLogs.older || podLogs.target?.read) return
    const below = view.scrollHeight - view.scrollTop
    await loadOlder()
    await tick()
    if (view && !follow) view.scrollTop = view.scrollHeight - below
  }
  function toLine(i: number) {
    follow = i === lines.length - 1
    view?.querySelector(`[data-line="${i}"]`)?.scrollIntoView({ block: 'center' })
  }

  let full = $state(pref<boolean>('logs.full', false))
  function toggleFull() {
    full = !full
    setPref('logs.full', full)
  }
  const typing = (e: KeyboardEvent) => !!(e.target as HTMLElement | null)?.closest?.('input, textarea, select')

  function onKey(e: KeyboardEvent) {
    if (!podLogs.open) return
    if (e.key === 'f' && !typing(e) && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault()
      e.stopImmediatePropagation()
      return toggleFull()
    }
    if (e.key !== 'Escape') return
    e.stopImmediatePropagation()
    if (query) query = ''
    else closePodLogs()
  }
  /** A shell in the pod's container (`kubectl exec`), in the shells drawer that replaces this one. */
  const canShell = $derived(isDesktop() && !!live.snapshot?.pod_shell && !podLogs.target?.read && !!pod)
  function shell() {
    const target = podLogs.target
    if (!target || !pod) return
    openPodShell({ context: target.context, namespace: target.namespace, pod: pod.name, container: podLogs.container }, `${target.label} › ${short(pod.name)}`)
    closePodLogs()
  }

  /** The pod's CPU and memory over time above its log (`[clusters] usage_history`), remembered under `logs.usage`. */
  const canUsage = $derived(!!live.snapshot?.pod_history && !podLogs.target?.read && !!pod)
  /** A workload of an environment (not a block's items): its timeline, around its crash. */
  const changed = $derived.by(() => {
    const tg = podLogs.target
    if (!tg || tg.read || !changesOn()) return null
    const env = live.snapshot?.environments?.envs.find((e) => e.context === tg.context && e.namespace === tg.namespace)
    return env ? { env: env.env, app: tg.deployment } : null
  })
  let usage = $state(pref<boolean>('logs.usage', true))
  function toggleUsage() {
    usage = !usage
    setPref('logs.usage', usage)
  }

  let drawerEl: HTMLElement | undefined = $state()
  function onOutside(e: PointerEvent) {
    if (podLogs.open && clickedOutside(e, drawerEl)) {
      closePodLogs()
    }
  }
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if podLogs.open && podLogs.target}
  <aside class="drawer" class:full bind:this={drawerEl} data-drawer="51" data-pinned={isPinned('pod-logs')} aria-label={t('Workload logs')}>
    <header>
      <span class="what">{podLogs.target.label}</span>
      {#if pod}
        <span class="sep">›</span>
        <b class="mono" title={pod.name}>{short(pod.name)}</b>
        {#if pod.containers.length > 1 && !podLogs.target.read}
          <select value={podLogs.container} onchange={(e) => setContainer(e.currentTarget.value)} aria-label={t('Container')}>
            {#each pod.containers as c (c)}<option value={c}>{c}</option>{/each}
          </select>
        {/if}
        {#if pod.last_exit}
          <span class="seg" role="group" aria-label={t('Which log')}>
            <button class:on={!podLogs.previous} onclick={() => setPrevious(false)}>{t('Current')}</button>
            <button class:on={podLogs.previous} onclick={() => setPrevious(true)} title={t('The log before the last restart ({reason})', { reason: pod.last_exit ?? '' })}>{t('Previous · {reason}', { reason: pod.last_exit ?? '' })}</button>
          </span>
        {/if}
        {#if !podLogs.previous}<span class="st run"><Icon name="running" /> {t('live')}</span>{/if}
      {/if}
      <span class="grow"></span>
      <input class="filter" bind:value={query} placeholder={t('Filter lines…')} spellcheck="false" />
      {#if firstError !== -1}<button class="txt" onclick={() => toLine(firstError)} title={t('Scroll to the first error line')}>{t('First error')}</button>{/if}
      <button class="txt" onclick={() => toLine(lines.length - 1)} title={t('Scroll to the end and follow')}>{t('End')} <Icon name="down" /></button>
      {#if changed}<button class="txt" onclick={() => openWhatChanged(changed.env, changed.app)} title={t('The timeline of {env} around the crash, the change just before it highlighted', { env: changed.env ?? '' })}><Icon name="activity" /> {t('What changed')}</button>{/if}
      {#if canUsage}<button class="txt" class:on={usage} onclick={toggleUsage} aria-pressed={usage} title={t('CPU and memory of this pod over time')}><Icon name="chart" /> {t('Usage')}</button>{/if}
      {#if canShell}<button class="txt" onclick={shell} title={t('A shell in this container (kubectl exec): tmux keeps it after closing')}><Icon name="terminal" /> {t('Shell')}</button>{/if}
      <button class="icon" onclick={() => reload(true)} disabled={podLogs.loading || !podLogs.pod} title={t('Reload the log')} aria-label={t('Reload')}><Icon name="refresh" /></button>
      <button class="icon" onclick={toggleFull} title={full ? t('Exit full screen (f)') : t('Full screen (f)')} aria-label={t('Full screen')} aria-pressed={full}><Icon name={full ? 'shrink' : 'expand'} /></button>
      <PinButton kind="pod-logs" />
      <button class="icon" onclick={closePodLogs} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      <nav class="pods" aria-label={t('Pods')}>
        <p class="stage">{podLogs.target.namespace}</p>
        {#each podLogs.pods as p (p.name)}
          <button class="pod" class:on={p.name === podLogs.pod} onclick={() => selectPod(p.name)} title="{p.name} · {p.status}{p.restarts ? ` · ${t('{n} restarts', { n: p.restarts })}` : ''}">
            <span class="i" class:ok={p.ready} class:ko={!p.ready}><Icon name="running" /></span>
            <span class="n">{short(p.name)}</span>
            <span class="d">{#if p.restarts}<Icon name="refresh" />{p.restarts} {/if}{p.started_at ? ago(p.started_at) : ''}</span>
            {#if !p.ready}<span class="why">{p.status}</span>{/if}
          </button>
        {:else}
          <p class="muted pad">{podLogs.loading ? t('Loading…') : t('No pod.')}</p>
        {/each}
      </nav>
      <div class="main">
      {#if canUsage && usage && pod && podLogs.target}<PodUsage context={podLogs.target.context} namespace={podLogs.target.namespace} pod={pod.name} />{/if}
      <div class="log" bind:this={view} onscroll={onScroll}>
        {#if podLogs.error}
          <p class="error">{podLogs.error}</p>
        {:else if !podLogs.log && podLogs.loading}
          <p class="muted pad">{t('Loading the log…')}</p>
        {:else if !podLogs.log}
          <p class="muted pad">{t('Empty log.')}</p>
        {:else}
          {#if podLogs.older}
            <p class="muted pad">{t('Reading older lines…')}</p>
          {:else if podLogs.truncated && !podLogs.target.read}
            <p class="muted pad">{t('Only the last lines of this log are shown.')} <button class="txt" onclick={older}>{t('Older lines')} <Icon name="up" /></button></p>
          {:else if podLogs.truncated}
            <p class="muted pad">{t('Only the last lines of this log are shown.')}</p>
          {:else if podLogs.depth !== null}
            <p class="muted pad">{t('Start of the log: the container started here.')}</p>
          {/if}
          {#if q && !shown.length}<p class="muted pad">{t('No line contains “{query}”.', { query })}</p>{/if}
          <pre>{#each shown as { l, i } (reading.key(i))}<div class="line {level(l)}" data-line={i}><span class="no">{i + 1}</span><span class="txt-line">{#each l.segs as s, k (k)}{#if s.fg}<span class={s.fg} class:b={s.bold}>{s.text}</span>{:else}<span class:b={s.bold}>{#each highlight(s.text) as h, j (j)}{#if h.tok}<span class="t-{h.tok}">{h.text}</span>{:else}{h.text}{/if}{/each}</span>{/if}{/each}</span></div>{/each}</pre>
        {/if}
      </div>
      </div>
    </div>
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 51; width: min(1100px, 94vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid color-mix(in srgb, var(--src-k8s) 45%, var(--border));
    box-shadow: var(--shadow-drawer);
  }
  /* Full screen: the whole window, no shadow or border. */
  .drawer.full { left: 0; width: auto; border-left: none; box-shadow: none; }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; min-width: 0; white-space: nowrap;
    border-bottom: 1px solid var(--border);
    background: var(--surface); box-shadow: inset 0 2px var(--src-k8s);
  }
  .what { font-weight: 600; overflow: hidden; text-overflow: ellipsis; }
  .sep, .muted { color: var(--text-muted); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .grow { flex: 1; }
  .st { font-size: var(--fs-meta); }
  .ok { color: var(--ok); } .ko { color: var(--ko); } .run { color: var(--info); }
  select { font: inherit; font-size: var(--fs-meta); padding: 1px 4px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); }
  .seg { display: inline-flex; gap: 1px; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .seg button { padding: 0 8px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); font-weight: 500; }
  .filter { width: 160px; height: 24px; padding: 0 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); outline: none; }
  .filter:focus { border-color: var(--info); }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: 15px; line-height: 1; cursor: pointer; }
  .icon:hover, .txt:hover { background: var(--surface-muted); color: var(--text); }
  .icon:disabled { opacity: 0.4; cursor: progress; }
  .txt { border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); padding: 1px 8px; cursor: pointer; }
  .body { flex: 1; min-height: 0; display: grid; grid-template-columns: 240px 1fr; }
  .pods { overflow: auto; border-right: 1px solid var(--border); padding: 4px 0 12px; }
  .stage { margin: 8px 10px 2px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .pod { display: grid; grid-template-columns: 12px minmax(0, 1fr) auto; align-items: center; column-gap: 6px; width: 100%; padding: 3px 10px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .pod:hover { background: var(--surface-muted); }
  .pod.on { background: color-mix(in srgb, var(--src-k8s) 14%, transparent); }
  .pod .i { display: inline-flex; justify-content: center; font-size: 11px; }
  .pod .n { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-tag); }
  .pod .d { font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; }
  .pod .why { grid-column: 2 / -1; font-size: var(--fs-tag); color: var(--ko); }
  .main { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .txt.on { color: var(--text); border-color: color-mix(in srgb, var(--src-k8s) 55%, var(--border)); }
  .log { flex: 1; min-height: 0; overflow: auto; background: var(--bg); color: var(--text); }
  .pad { padding: 10px 14px; margin: 0; font-size: var(--fs-body); }
  .log .muted { color: var(--text-muted); }
  .error { margin: 0; padding: 10px 14px; color: var(--ko); font-size: var(--fs-body); white-space: pre-wrap; }
  pre { margin: 0; padding: 6px 0 16px; font: 11.5px/1.5 var(--mono); }
  /* Off screen, a line is neither laid out nor painted: a long log scrolls and slides lightly. */
  .line { display: flex; padding-right: 14px; content-visibility: auto; contain-intrinsic-size: auto 1.5em; }
  .line:hover { background: color-mix(in srgb, var(--text) 4%, transparent); }
  .line.lv-err { background: color-mix(in srgb, var(--ko) 8%, transparent); box-shadow: inset 2px 0 var(--ko); }
  .line.lv-warn { background: color-mix(in srgb, var(--warn) 6%, transparent); }
  /* Syntax highlighting (log-highlight.ts), in the theme's colours. */
  .t-time { color: var(--text-muted); }
  .t-err { color: var(--ko); font-weight: 700; }
  .t-warn { color: var(--warn); font-weight: 700; }
  .t-info { color: var(--info); font-weight: 600; }
  .t-debug { color: var(--text-muted); }
  .t-method { color: var(--violet, var(--accent)); font-weight: 600; }
  .t-url { color: color-mix(in srgb, var(--info) 72%, var(--text)); }
  .t-key { color: color-mix(in srgb, var(--src-calendar, var(--info)) 72%, var(--text)); }
  .t-str { color: color-mix(in srgb, var(--ok) 72%, var(--text)); }
  .t-id { color: color-mix(in srgb, var(--violet, var(--accent)) 72%, var(--text)); }
  .t-num, .t-lit { color: color-mix(in srgb, var(--warn) 72%, var(--text)); }
  .t-ok { color: var(--ok); font-weight: 600; }
  .t-redir { color: var(--info); font-weight: 600; }
  .t-cerr { color: var(--warn); font-weight: 600; }
  .t-serr { color: var(--ko); font-weight: 600; }
  .no { flex-shrink: 0; width: 52px; padding-right: 12px; text-align: right; color: color-mix(in srgb, var(--text-muted) 70%, transparent); user-select: none; }
  .txt-line { white-space: pre-wrap; overflow-wrap: anywhere; min-width: 0; }
  .b { font-weight: 700; }
  /* A phone, a narrow window: the header wraps, the pods a strip above the log. */
  @media (max-width: 640px) {
    .drawer { width: 100vw; }
    header { flex-wrap: wrap; row-gap: 6px; white-space: normal; }
    .filter { flex: 1; min-width: 120px; }
    .body { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); }
    .pods { display: flex; gap: 2px; overflow-x: auto; overflow-y: hidden; padding: 4px 6px; border-right: none; border-bottom: 1px solid var(--border); }
    .stage { display: none; }
    .pod { flex-shrink: 0; width: auto; grid-template-columns: 12px auto; border-radius: var(--r5); }
    .pod .d, .pod .why { display: none; }
    .no { width: 34px; padding-right: 8px; }
  }
</style>
