<script lang="ts">
  import { BLOCK_DEFS } from '../blocks'
  import { stateOf } from '../blocks/block'
  import { onMount } from 'svelte'
  import { boot, featureOn, reducedMotion } from '../fun.svelte'
  import { switchedHere } from '../remote/switched'
  import { live } from '../live.svelte'
  import { countedSessions } from '../rules'
  import type { Snapshot } from '../types'
  import { versionLabel } from '../version'
  import { profileColor } from '../profiles.svelte'
  import { forgeName, mrWord, onGithub } from '../forge'
  import { agentName, cliOf } from '../agent-cli'
  import { trackerProvider } from '../tracker'
  import { THEMES } from '../theme.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Prism startup: while the collectors respond, a black screen where a ray
   * crosses the prism and comes out as a rainbow, and one line per source that lights up when
   * it has responded (or turns "warn" on an error), with the time it took — and, while
   * it loads, the time passing — counted from when the screen appeared. It stays until all have
   * responded (30 s at most, then "later"), at least 1.2 s, then 1 s more to read once everything is loaded and shown, then rises like a curtain. A click
   * or a key skips it. While it waits, the screen lives: the prism breathes, light runs along the
   * ray with dust in it, each band of the rainbow lights up as the sources answer (the rainbow is
   * the progress), each line types itself with a cursor on the one waiting, and stars drift behind.
   * Reduce motion keeps it still: lines whole, rainbow lit, no stars.
   * It follows the theme, always dark: a dark theme's own background, accent and colors; a light
   * one's accent on a dark base tinted with it. Two themes have a scene of their own: Synthwave,
   * an Outrun drive in a pixel typeface (a car seen from behind on a road toward the mountains, palms
   * going by; the sun is the progress, rising from behind the mountains as the sources answer, its
   * stripes closing, the sky turning from night to dawn; each source a checkpoint with its time, a
   * thousand points each, the speed climbing; a flash, GOAL! and everything faster once all have
   * answered); Hacker, green phosphor with a rain of characters, an ASCII prism and
   * the lines as a system's boot log, "ACCESS GRANTED" while there is time to read them; Blueprint,
   * an architect's plan (the prism drawn line by line as the progress, a numbered mark on its
   * outline and a line in the legend per source, the title block; APPROVED stamped once all have
   * answered, the rays drawn out of the prism); Quest, an 8-bit RPG (a hero walking a field, a
   * companion joining per source answered, its line in the dialog box, the XP bar as the progress,
   * the gold as the score; LEVEL UP! and the party jumping once all have answered).
   * On every launch, with `[appearance] boot` (default). It only starts once the window is
   * visible: launched with the window hidden, the app waits for it to show. The console's `boot`
   * command replays it.
   */
  /** The phone (the remote): the same screen, at its width, tapped to skip. */
  const onPhone = document.documentElement.classList.contains('remote')
  const MIN_MS = 1200
  /** Once everything is loaded and all lines shown, time to read them before the screen rises. */
  const READ_MS = 1000
  /** Lines appear one after another, at this interval. */
  const LINE_MS = 150
  /** Beyond this, a silent source is marked "later" and the screen rises anyway. */
  const MAX_MS = 30_000
  /** Beyond this, the screen says what it is still waiting for. */
  const SLOW_MS = 3000

  interface Source {
    label: string
    /** Source name on the Rust side: its response (`answered`) and its error (`errors`). */
    source: string
    /**
     * Is the module read? Otherwise, no line: nothing to wait for. The snapshot says what is
     * enabled (`gitlab`, `cluster`, `gcp`); before it, a line waits like the others.
     */
    enabled?: (s: Snapshot | null) => boolean
    /** What it brought, once there. */
    detail: (s: Snapshot) => string
    /** Data already there without a new response (Trivy scan cached from one launch to the next). */
    cached?: (s: Snapshot) => boolean
  }
  /** The forge and the repository (`[forge] enabled`). */
  const code = (s: Snapshot | null) => s?.gitlab !== false
  /** The Kubernetes clusters (`[clusters] enabled`). */
  const cluster = (s: Snapshot | null) => s?.cluster !== false
  const SOURCES: Source[] = [
    { label: t('Mounting worktrees'), source: 'worktrees', enabled: code, detail: (s) => t('{n} found', { n: s.worktrees.length }) },
    // The snapshot is read before mount (main.ts): the forge is known here.
    { label: t('Handshaking with {forge}', { forge: forgeName() }), source: onGithub() ? 'github' : 'gitlab', enabled: code, detail: (s) => t('{n} {mr}s', { n: s.merge_requests.filter((m) => m.state === 'opened').length, mr: mrWord() }) },
    { label: t('Syncing the sprint'), get source() { return trackerProvider() }, enabled: (s) => s?.tracker !== null, detail: (s) => s.sprint?.name ?? t('no sprint') },
    { label: t('Probing kube contexts'), source: 'environments', enabled: cluster, detail: (s) => t('{n} clusters', { n: s.environments?.clusters?.length ?? s.environments?.envs.length ?? 0 }) },
    { label: t('Asking ArgoCD'), source: 'argocd', enabled: cluster, detail: (s) => tn(s.argocd?.instances.reduce((n, i) => n + i.apps.length, 0) ?? 0, '{n} app', '{n} apps') },
    // The blocks with a source of their own, while on: one line per source.
    ...BLOCK_DEFS.filter((b, i) => b.boot && BLOCK_DEFS.findIndex((o) => (o.source ?? o.id) === (b.source ?? b.id) && o.boot) === i).map((b) => ({
      label: t(b.boot!.label),
      source: b.source ?? b.id,
      enabled: (s: Snapshot | null) => stateOf(b, s) !== undefined,
      detail: (s: Snapshot) => b.boot!.detail(stateOf(b, s) as never),
    })),
    { label: t('Loading Trivy database'), source: 'vulnerabilities', enabled: (s) => code(s) && s?.features?.vulnerabilities !== false, detail: (s) => t('{n} targets', { n: s.vulnerabilities?.projects.length ?? 0 }), cached: (s) => !!s.vulnerabilities },
    { label: t('Waking {agent} sessions', { agent: agentName() }), source: 'sessions', detail: (s) => tn(countedSessions(s).length, '{n} session', '{n} sessions') },
    // OpenCode runs on the user's own providers: no plan usage to meter.
    { label: t('Metering {agent} usage', { agent: agentName() }), source: 'usage', enabled: (s) => cliOf(s?.agents_settings?.provider).plan, detail: (s) => (s.usage?.five_hour ? t('{pct}% of 5 h', { pct: Math.round(s.usage.five_hour.utilization) }) : t('read')), cached: (s) => !!s.usage },
  ]

  // The snapshot is read before mount (main.ts): the setting is known from the first frame. At the
  // first launch (no config.toml), FirstLaunch greets instead.
  // On the phone, a profile switch reloads the page: no boot screen for it (`phone-profiles`).
  let shown = $state(featureOn('boot') && !live.snapshot?.profile?.fresh && !(onPhone && switchedHere()))

  /** The theme's scene, read when the screen starts (applied before mounting, see main.ts). */
  type Scene = 'dark' | 'light' | 'synthwave' | 'hacker' | 'blueprint' | 'quest'
  function sceneOf(): Scene {
    const id = document.documentElement.dataset.theme
    if (id === 'synthwave' || id === 'hacker' || id === 'blueprint' || id === 'quest') return id
    return THEMES.find((th) => th.id === id)?.tone === 'light' ? 'light' : 'dark'
  }
  let scene = $state<Scene>(sceneOf())
  let lifting = $state(false)
  /** Synthwave: the car follows the pointer across the road (its x, in px; centered until it moves). */
  let steer = $state<number | undefined>()
  let elapsed = $state(0)

  /** State of each line: waiting, ready (detail), errored. */
  const lines = $derived(
    // A disabled module (GitLab, clusters, GCP…) is not read: no line, nothing to wait for.
    SOURCES.filter((src) => src.enabled?.(live.snapshot) ?? true).map((src) => {
      const s = live.snapshot
      const err = s?.errors[src.source]
      const ready = !!s && (s.answered?.includes(src.source) || !!src.cached?.(s))
      const state = err ? 'warn' : ready ? 'ok' : elapsed >= MAX_MS ? 'later' : 'wait'
      return { label: src.label, state, detail: err ? t('error') : ready ? src.detail(s!) : state === 'later' ? t('later') : '' }
    }),
  )
  const done = $derived(lines.every((l) => l.state !== 'wait'))

  /** Start of the count, and the moment each line left the waiting state (ms since start). */
  let t0 = 0
  let took = $state<Record<string, number>>({})
  $effect(() => {
    for (const l of lines) if (l.state !== 'wait' && took[l.label] === undefined) took[l.label] = Date.now() - t0
  })
  /** The dots after each label reach the same column, however long the language makes them. */
  const padTo = $derived(Math.max(26, ...lines.map((l) => l.label.length + 1)))
  const secs = (ms: number) => `${(ms / 1000).toFixed(1)}s`.padStart(5)
  const timeOf = (l: { label: string; state: string }) => secs(l.state === 'wait' ? elapsed : (took[l.label] ?? elapsed))
  const waiting = $derived(lines.filter((l) => l.state === 'wait'))

  const still = $derived(reducedMotion())
  /** Characters of a line's label shown: it types itself over LINE_MS, once the one above is written. */
  const typed = (i: number, label: string) => (still ? label.length : Math.max(0, Math.min(label.length, Math.floor(((elapsed - LINE_MS * (i + 1)) / LINE_MS) * label.length))))
  /** The line being typed, or else the first one waiting: the cursor's. */
  const cursor = $derived.by(() => {
    if (still) return -1
    const typing = lines.findIndex((l, i) => elapsed > LINE_MS * (i + 1) && typed(i, l.label) < l.label.length)
    return typing >= 0 ? typing : lines.findIndex((l) => l.state === 'wait')
  })
  /** Bands of the rainbow lit: the share of the sources answered. */
  const BANDS = ['#ff5f6d', '#ffc371', '#fff36e', '#7cf29a', '#5ec8ff', '#a78bfa']
  const lit = $derived(still ? BANDS.length : Math.ceil((lines.filter((l) => l.state !== 'wait').length / Math.max(1, lines.length)) * BANDS.length))
  /** Everything loaded and shown: the time left to read before the curtain (Hacker's ACCESS GRANTED). */
  const reading = $derived(done && elapsed >= Math.max(MIN_MS, LINE_MS * lines.length, ...Object.values(took)))
  /**
   * Synthwave's drive. A source reaches its checkpoint when it has answered and its line is typed (at
   * launch, sources have often answered before the screen appears): from then it counts in the
   * score (a thousand points if ok), the stage, the sun's height and the speed.
   */
  const reached = (i: number, l: { label: string; state: string }) => (l.state === 'wait' ? Infinity : Math.max(took[l.label] ?? 0, LINE_MS * (i + 2)))
  const arrived = $derived(lines.filter((l, i) => reached(i, l) <= elapsed))
  const score = $derived(String(arrived.filter((l) => l.state === 'ok').length * 1000).padStart(6, '0'))
  /** Every checkpoint reached and the time to read begun: GOAL!, a flash, everything faster. */
  const goal = $derived(reading && arrived.length === lines.length)
  /** The progress, 0 to 1: the sun rises with it, the sky lightens. */
  const progress = $derived(arrived.length / Math.max(1, lines.length))
  /** The clock counts down from the longest wait, frozen once everything has answered. */
  const clock = $derived(String(Math.max(0, Math.ceil((MAX_MS - (reading ? Math.max(0, ...Object.values(took)) : elapsed)) / 1000))).padStart(2, '0'))
  const kmh = $derived(goal ? 293 : Math.round(40 + progress * 220 + (still ? 0 : Math.sin(elapsed / 90) * 3)))
  /** The source that has just reached its checkpoint, for a moment, in the banner. */
  const checkpoint = $derived.by(() => {
    if (goal) return undefined
    let at = -1
    let line: (typeof lines)[number] | undefined
    lines.forEach((l, i) => {
      const r = reached(i, l)
      if (r <= elapsed && elapsed - r < 700 && r > at) [at, line] = [r, l]
    })
    return line
  })
  /** A checkpoint's time, as a racing game's (`1"50`). */
  const lap = (ms: number) => `${Math.floor(ms / 1000)}"${String(Math.floor((ms % 1000) / 10)).padStart(2, '0')}`
  const LAP: Record<string, string> = { warn: 'MISS', later: 'LATE', wait: '...' }
  /** Palms along the road, a pair at each stage of their way toward you. */
  const PALMS = [0, 0.45, 0.9, 1.35].map((d, i) => ({ right: i % 2 === 1, delay: -d }))
  /** The scenes drawn from the progress (`--p`): the sun, the plan's lines, the XP. */
  const paced = $derived(scene === 'synthwave' || scene === 'blueprint' || scene === 'quest')
  /** The last source to reach its checkpoint, kept until the next: the plan's and the dialog box's. */
  const latest = $derived.by(() => {
    let at = -1
    let line: (typeof lines)[number] | undefined
    lines.forEach((l, i) => {
      const r = reached(i, l)
      if (r <= elapsed && r > at) [at, line] = [r, l]
    })
    return line
  })
  /** Blueprint: a source's mark on the prism's outline (A top, B bottom right, C bottom left), at its share of the way round. */
  const TRI = [[30, 10], [52, 52], [8, 52]]
  function onOutline(f: number): [number, number] {
    const side = Math.min(2, Math.floor(f * 3))
    const k = f * 3 - side
    const [a, b] = [TRI[side], TRI[(side + 1) % 3]]
    return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
  }
  /** Blueprint: the rays out of the prism, once approved. */
  const RAYS = ['#ffb3a1', '#fff36e', '#b7f5c4', '#9fdcff']
  /** Quest: the hero and the companions' colors (tunic, hair), one per source, in turn. */
  const PARTY = [['#e33', '#7a3a0a'], ['#fc8', '#222'], ['#7b4dff', '#ffd84a'], ['#3ac', '#ddd'], ['#ffd84a', '#a33'], ['#4c4', '#3a2a1a'], ['#f6a', '#222'], ['#5cc8ff', '#7a3a0a'], ['#ff9a4a', '#ddd']]
  const party = (i: number) => PARTY[i % PARTY.length]
  /** Quest: a hundred gold per source answered, a level per source too. */
  const gold = $derived(arrived.filter((l) => l.state === 'ok').length * 100)
  /** Hacker's log: a line's time, as a kernel's (`[    0.142]`). */
  const stamp = (ms: number) => (ms / 1000).toFixed(3).padStart(9)
  const TAG: Record<string, string> = { ok: ' OK ', warn: 'FAIL', wait: ' .. ', later: 'LATE' }
  /** Hacker's rain: columns of characters falling at their own pace, fixed at random once. */
  const GLYPHS = 'ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789'
  const RAIN = Array.from({ length: 28 }, (_, i) => ({
    left: (i * 100) / 28 + Math.random() * 2,
    text: Array.from({ length: 14 + Math.floor(Math.random() * 12) }, () => GLYPHS[Math.floor(Math.random() * GLYPHS.length)]).join(''),
    duration: 2.4 + Math.random() * 3,
    delay: -Math.random() * 5,
  }))

  /** Dust in the ray: fixed at random once. */
  const DUST = Array.from({ length: 14 }, () => ({ left: -70 + Math.random() * 24, top: 41 + Math.random() * 12, dy: (Math.random() - 0.5) * 14, delay: Math.random() * 2.2 }))

  /** Stars drifting behind, slowly; stopped with the screen. */
  function stars(canvas: HTMLCanvasElement) {
    if (still) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const w = (canvas.width = canvas.clientWidth || innerWidth)
    const h = (canvas.height = canvas.clientHeight || innerHeight)
    // The theme's tint (`--b-star`), resolved by the page; brighter or fainter by depth.
    ctx.fillStyle = 'rgb(200, 200, 255)'
    ctx.fillStyle = getComputedStyle(canvas).color
    const pts = Array.from({ length: Math.round((w * h) / 9000) }, () => ({ x: Math.random() * w, y: Math.random() * h, z: Math.random() }))
    let raf = 0
    const draw = () => {
      ctx.clearRect(0, 0, w, h)
      for (const p of pts) {
        p.x -= 0.12 + p.z * 0.45
        if (p.x < 0) {
          p.x = w
          p.y = Math.random() * h
        }
        ctx.globalAlpha = 0.12 + p.z * 0.45
        ctx.fillRect(p.x, p.y, 1 + p.z, 1 + p.z)
      }
      raf = requestAnimationFrame(draw)
    }
    draw()
    return () => cancelAnimationFrame(raf)
  }

  function lift() {
    if (lifting) return
    lifting = true
    stop()
    setTimeout(() => (shown = false), 600)
  }

  /** Each frame: the time passing (the lines type themselves from it). */
  let frame = 0
  const skip = () => lift()
  function stop() {
    cancelAnimationFrame(frame)
    removeEventListener('keydown', skip)
  }
  /** Starts the count: the lines, then the curtain. */
  function start() {
    stop()
    scene = sceneOf()
    shown = true
    lifting = false
    steer = undefined
    elapsed = 0
    took = {}
    t0 = Date.now()
    const tick = () => {
      elapsed = Date.now() - t0
      frame = requestAnimationFrame(tick)
    }
    tick()
    addEventListener('keydown', skip, { once: true })
  }

  onMount(() => {
    if (!shown) return
    if (document.visibilityState === 'visible') return start(), stop
    // Window hidden at launch: the screen waits for it to show.
    const visible = () => {
      if (document.visibilityState !== 'visible') return
      document.removeEventListener('visibilitychange', visible)
      start()
    }
    document.addEventListener('visibilitychange', visible)
    return () => {
      document.removeEventListener('visibilitychange', visible)
      stop()
    }
  })
  // `boot` command: replay, whatever the setting.
  let replays = boot.replay
  $effect(() => {
    if (boot.replay === replays) return
    replays = boot.replay
    start()
  })
  $effect(() => {
    // Read from the last event: the last response, or the last line shown (at
    // launch, sources have often responded before the screen appears).
    if (shown && done && elapsed >= Math.max(MIN_MS, LINE_MS * lines.length, ...Object.values(took)) + READ_MS) lift()
  })
</script>

{#if shown}
  {@const profile = live.snapshot?.profile}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="boot {scene}" class:lifting class:still class:ready={goal} style:--p={paced ? progress : null} style:--steer={scene === 'synthwave' && steer !== undefined ? `${steer}px` : null} onclick={lift} onpointermove={(e) => (steer = e.clientX)} aria-hidden="true">
    {#if scene === 'dark' || scene === 'light' || scene === 'synthwave'}<canvas {@attach stars}></canvas>{/if}
    {#if scene === 'synthwave'}
      <!-- An Outrun drive toward a rising sun: the sun behind the mountains, the road, palms going by, the car. -->
      <div class="sunset">
        <span class="sun"></span><span class="ridge"></span><span class="road"></span>
        {#if !still}{#each PALMS as p, i (i)}<span class="palm" class:right={p.right} style:animation-delay="{p.delay}s"><svg viewBox="0 0 20 40"><path d="M10 40C10 30 11 20 10 12" stroke="#120321" stroke-width="2" fill="none" /><path d="M10 12C4 8 0 11 0 14 3 11 7 11 10 12 6 6 3 4 1 4 5 3 8 7 10 12 10 6 12 2 15 1 12 5 11 8 10 12 14 8 18 8 20 11 16 10 13 11 10 12 15 12 19 15 19 19 16 15 13 13 10 12Z" fill="#120321" /></svg></span>{/each}{/if}
        <svg class="car" viewBox="0 0 64 30" shape-rendering="crispEdges"><rect x="10" y="2" width="44" height="10" fill="#2a0f4a" /><rect x="13" y="4" width="38" height="7" fill="#36f9f6" opacity=".55" /><rect x="2" y="11" width="60" height="12" fill="#ff2a9a" /><rect x="2" y="11" width="60" height="2" fill="#ff8ad0" /><rect x="5" y="15" width="14" height="4" fill="#ff1a3a" /><rect x="45" y="15" width="14" height="4" fill="#ff1a3a" /><rect x="22" y="15" width="20" height="4" fill="#1a0630" /><rect x="26" y="16" width="12" height="2" fill="#ffd75e" /><rect x="4" y="23" width="12" height="6" fill="#0b0414" /><rect x="48" y="23" width="12" height="6" fill="#0b0414" /><rect x="16" y="23" width="32" height="3" fill="#3a1060" /></svg>
      </div>
      <div class="hud"><span><b>STAGE</b> {Math.min(lines.length, arrived.length + 1)}/{lines.length}</span><span class="clock"><b>TIME</b><span>{clock}</span></span><span><b>SCORE</b> {score}</span></div>
      <span class="marquee">PRISM</span>
      <span class="version">{profile?.name ? `${profile.name.toUpperCase()} · ` : ''}{onPhone ? versionLabel().split(' · ')[0] : versionLabel()}</span>
      <!-- The source that just reached its checkpoint, with what it brought; GOAL! once all have. -->
      {#if goal}<span class="banner goal">GOAL!</span>
      {:else if checkpoint}<span class="banner"><b class={checkpoint.state}>{checkpoint.state === 'ok' ? 'CHECKPOINT' : checkpoint.state === 'warn' ? 'CRASH!' : 'LATE'}</b>{checkpoint.label.toUpperCase()}{checkpoint.detail ? ` · ${checkpoint.detail.toUpperCase()}` : ''}</span>{/if}
      <span class="flash"></span>
    {:else if scene === 'blueprint'}
      <!-- An architect's plan: the prism drawn with the progress, a numbered mark per source on its outline. -->
      <svg class="plan" viewBox="-12 -6 84 72">
        <circle class="arc" cx="30" cy="10" r="6" /><line class="axis" x1="30" y1="-4" x2="30" y2="62" /><line class="axis" x1="-8" y1="52" x2="68" y2="52" />
        <path class="tri" pathLength="1" d="M30 10 52 52H8Z" />
        <line class="ray" pathLength="1" x1="-10" y1="34" x2="19" y2="31" />
        {#each RAYS as c, i (c)}<line class="ray" pathLength="1" x1="41" y1="31" x2="72" y2={37 + i * 3.5} style:stroke={c} />{/each}
        <line class="dim" x1="8" y1="57" x2="52" y2="57" /><line class="dim" x1="8" y1="55" x2="8" y2="59" /><line class="dim" x1="52" y1="55" x2="52" y2="59" />
        <text x="26.5" y="61.5">44,0</text><text x="33.5" y="7.5">60°</text>
        {#each lines as l, i (l.label)}{#if reached(i, l) <= elapsed}{@const [x, y] = onOutline((i + 0.5) / lines.length)}<g class="mark {l.state}"><circle cx={x} cy={y} r="2.3" /><text {x} y={y + 0.85}>{i + 1}</text></g>{/if}{/each}
      </svg>
    {:else if scene === 'quest'}
      <!-- An 8-bit RPG: a hero walking a field, a companion joining per source answered. -->
      <div class="field"><span class="cloud"></span><span class="cloud b"></span><span class="hills"></span><span class="ground"></span></div>
      <div class="hud"><span><b>LV</b> {arrived.filter((l) => l.state === 'ok').length + 1}</span><span class="xp"><b>XP</b>{#each lines as l, i (l.label)}<i class:on={reached(i, l) <= elapsed}></i>{/each}</span><span>{gold} G</span></div>
      <span class="marquee">PRISM QUEST</span>
      <span class="version">{profile?.name ? `${profile.name.toUpperCase()} · ` : ''}{onPhone ? versionLabel().split(' · ')[0] : versionLabel()}</span>
      {#if goal}<span class="lvl">LEVEL UP!</span>{/if}
      <div class="party">
        {#each [-1, ...lines.map((_, i) => i)] as i (i)}{@const l = lines[i]}{@const [body, hair] = party(i + 1)}
          <svg class="hero" class:join={i >= 0} class:on={i < 0 || (reached(i, l) <= elapsed && l.state === 'ok')} viewBox="0 0 8 12" shape-rendering="crispEdges"><rect x="2" y="0" width="4" height="2" fill={hair} /><rect x="2" y="2" width="4" height="3" fill="#ffcc99" /><rect x="3" y="3" width="1" height="1" fill="#000" /><rect x="1" y="5" width="6" height="4" fill={body} /><rect x="2" y="9" width="1" height="3" fill="#333" /><rect x="5" y="9" width="1" height="3" fill="#333" /></svg>
        {/each}
      </div>
    {:else if scene === 'hacker'}
      {#if !still}<div class="rain">{#each RAIN as r, i (i)}<span style:left="{r.left}%" style:animation-duration="{r.duration}s" style:animation-delay="{r.delay}s">{r.text}</span>{/each}</div>{/if}
      <pre class="ascii">{`      /\\
     /  \\    PRISM ${versionLabel()}
    /____\\   (c) ${new Date().getFullYear()} · tty1`}</pre>
    {:else}
      <div class="prism">
        <span class="beam"></span>
        {#if !still}{#each DUST as d, i (i)}<span class="dust" style:left="{d.left}px" style:top="{d.top}px" style:--dy="{d.dy}px" style:animation-delay="{d.delay}s"></span>{/each}{/if}
        <svg viewBox="0 0 32 32"><path d="M16 4 28 26H4Z" /></svg>
        <span class="rainbow">{#each BANDS as c, i (c)}<i class:on={i < lit} style:top="{(i * 100) / BANDS.length}%" style:--c={c}></i>{/each}</span>
      </div>
    {/if}
    {#if scene === 'hacker'}
      <pre class="log"><span class="dim">[{stamp(0)}] </span>prism: booting{#if profile?.name}{' '}profile <span class="amber">{profile.name}</span>{/if}
{#each lines as l, i (l.label)}{#if elapsed > LINE_MS * (i + 1)}{@const n = typed(i, l.label)}<span><span class="dim">[{stamp(took[l.label] ?? LINE_MS * (i + 1))}] </span>{#if n < l.label.length}{l.label.slice(0, n)}{:else}[<i class={l.state}>{TAG[l.state]}</i>] {l.label} <small>{l.detail}</small>{/if}{#if i === cursor}<b class="cur"></b>{/if}</span>
{/if}{/each}</pre>
      {#if reading}<span class="grant">ACCESS GRANTED</span>{/if}
    {:else if scene === 'blueprint'}
      <!-- The legend: a numbered line per source, what it brought once there. -->
      <ol class="legend">{#each lines as l, i (l.label)}{#if elapsed > LINE_MS * (i + 1)}{@const n = typed(i, l.label)}{@const there = reached(i, l) <= elapsed}<li class={there ? l.state : 'wait'}><b>{i + 1}</b><span>{n < l.label.length ? l.label.slice(0, n) : l.label}{#if i === cursor}<i class="cur"></i>{/if}<small>{there ? l.detail : '…'}</small></span></li>{/if}{/each}</ol>
      <div class="cartouche"><span class="big">PRISM</span><span><small>{t('Revision')}</small>{versionLabel().split(' · ')[0]}</span><span><small>{t('Sources')}</small>{arrived.length}/{lines.length}</span><span><small>{t('Drawn by')}</small>{profile?.name || 'Prism'}</span></div>
      {#if goal}<span class="stamp">{t('APPROVED')}</span>{/if}
    {:else if scene === 'quest'}
      <!-- The dialog box: the last source there, or the whole party. -->
      <div class="dialog">
        {#if goal}{t('The whole party is here!')}<br /><b>{profile?.name || 'Prism'}</b> {t('is ready for adventure.')}
        {:else if latest}<b class={latest.state}>{latest.label}</b><br />{latest.state === 'ok' ? t('Success! {detail}', { detail: latest.detail }) : latest.state === 'warn' ? t('The quest failed…') : t('Too slow… later!')}
        {:else}{t('A new day begins…')}{/if}
        <span class="next">▼</span>
      </div>
    {:else if scene === 'synthwave'}
      <pre class="laps">{#each lines as l, i (l.label)}{#if elapsed > LINE_MS * (i + 1)}{@const n = typed(i, l.label)}<span>{#if n < l.label.length}{l.label.slice(0, n).toUpperCase()}{:else}{l.label.toUpperCase().padEnd(padTo, ' ')}<i class={l.state}>{reached(i, l) <= elapsed && l.state === 'ok' ? lap(took[l.label] ?? 0) : (LAP[l.state] ?? '...')}</i>{/if}{#if i === cursor}<b class="cur"></b>{/if}</span>
{/if}{/each}</pre>
      <span class="speed"><span>{kmh}</span>KM/H<span class="tacho">{#each { length: 12 } as _, i (i)}<i class:on={i < Math.round((kmh / 293) * 12)} class:hot={i >= 9}></i>{/each}</span></span>
    {:else}
      <pre>
<b>Prism</b> <small class="ver">{versionLabel()}</small>{#if profile}  <span class="prof" style:--c={profileColor(profile.color)} title={t('Profile')}><i></i>{profile.name}</span>{/if}
{#each lines as l, i (l.label)}{#if elapsed > LINE_MS * (i + 1)}{@const n = typed(i, l.label)}<span>{#if n < l.label.length}{l.label.slice(0, n)}{:else}{l.label.padEnd(padTo, '.')} <i class={l.state}>{l.state === 'wait' ? '…   ' : l.state === 'ok' ? 'ok  ' : 'warn'}</i> <em class:run={l.state === 'wait'}>{timeOf(l)}</em> <small>{l.detail}</small>{/if}{#if i === cursor}<b class="cur"></b>{/if}</span>
{/if}{/each}</pre>
    {/if}
    <span class="skip">{elapsed >= SLOW_MS && waiting.length ? tn(waiting.length, 'waiting for {n} source · {s} s · ', 'waiting for {n} sources · {s} s · ', { s: Math.round(elapsed / 1000) }) : ''}{onPhone ? t('tap to skip') : t('click or press a key to skip')}</span>
  </div>
{/if}

<style>
  /*
   * The scene's colors, from the theme: a dark theme's own (its background darkened, its surface
   * as the glow); a light theme's accent on a dark base tinted with it, the text light.
   */
  .boot {
    --b-bg: color-mix(in srgb, var(--bg) 55%, #000);
    --b-glow: color-mix(in srgb, var(--accent) 8%, color-mix(in srgb, var(--surface) 60%, #000));
    --b-accent: var(--accent);
    --b-title: var(--text);
    --b-line: var(--text-muted);
    --b-faint: color-mix(in srgb, var(--text-muted) 55%, var(--b-bg));
    --b-ok: var(--ok);
    --b-warn: var(--warn);
    --b-star: color-mix(in srgb, var(--accent) 35%, var(--text));
  }
  .boot.light {
    --b-bg: color-mix(in srgb, var(--accent) 7%, #0b0b0d);
    --b-glow: color-mix(in srgb, var(--accent) 20%, #0b0b0d);
    --b-accent: color-mix(in srgb, var(--accent) 70%, #fff);
    --b-title: #fff;
    --b-line: #9c9ba8;
    --b-faint: #6b6b78;
    --b-ok: #7ee0a8;
    --b-warn: #ffc978;
    --b-star: #c8c8ff;
  }
  .boot { position: fixed; inset: 0; z-index: 100; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 22px; background: var(--b-bg); color: var(--b-line); box-shadow: var(--shadow-modal); transition: transform 0.6s cubic-bezier(0.7, 0, 0.2, 1); }
  /* Exit: the screen rises like a curtain and reveals the app. */
  .boot.lifting { transform: translateY(-102%); }
  .boot:not(.still) { background: radial-gradient(ellipse at 50% 35%, var(--b-glow), var(--b-bg) 70%); }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; color: var(--b-star); }
  .prism { position: relative; width: 200px; height: 90px; }
  .prism svg { position: absolute; left: 50%; top: 0; width: 90px; height: 90px; transform: translateX(-50%); fill: color-mix(in srgb, var(--b-accent) 30%, transparent); stroke: var(--b-accent); stroke-width: 1.4; stroke-linejoin: round; }
  .beam { position: absolute; left: -70px; top: 47px; width: 150px; height: 2px; background: #fff; box-shadow: 0 0 8px #fff; transform-origin: right center; transform: scaleX(0); animation: beam 0.5s 0.2s forwards; }
  .rainbow { position: absolute; left: 122px; top: 40px; width: 160px; height: 20px; transform-origin: left center; clip-path: polygon(0 45%, 100% 0, 100% 100%, 0 55%); opacity: 0; animation: fan 0.5s 0.7s forwards; }
  /* A band per sixth of the sources answered: faint until then, shimmering once lit. */
  .rainbow i { position: absolute; left: 0; right: 0; height: calc(100% / 6); background: linear-gradient(90deg, var(--c), color-mix(in srgb, var(--c) 45%, #fff) 50%, var(--c)); background-size: 200% 100%; opacity: 0.18; transition: opacity 0.4s; animation: shimmer 1.6s linear infinite; }
  .rainbow i.on { opacity: 1; }
  .still .rainbow i { background: var(--c); animation: none; }
  /* The prism breathes, light runs along the ray, dust drifts in it. */
  .boot:not(.still) .prism svg { animation: breathe 2.4s ease-in-out infinite; }
  .boot:not(.still) .beam::after { content: ''; position: absolute; inset: -1px 0; background: linear-gradient(90deg, transparent, #fff 50%, transparent) no-repeat; background-size: 40px 100%; filter: blur(1px); animation: pulse 1.1s linear infinite; }
  .dust { position: absolute; width: 2px; height: 2px; border-radius: 50%; background: #fff; opacity: 0; animation: dust 2.2s linear infinite; }
  @keyframes breathe { 50% { filter: drop-shadow(0 0 12px color-mix(in srgb, var(--b-accent) 75%, transparent)); } }
  @keyframes pulse { from { background-position: -40px 0; } to { background-position: 150px 0; } }
  @keyframes shimmer { from { background-position: 100% 0; } to { background-position: -100% 0; } }
  @keyframes dust { 0% { opacity: 0; transform: translate(0, 0); } 20% { opacity: 0.8; } 100% { opacity: 0; transform: translate(140px, var(--dy)); } }
  /* The cursor: on the line typing, or the first one waiting. */
  pre .cur { display: inline-block; width: 6px; height: 12px; margin-left: 2px; background: var(--b-accent); vertical-align: -2px; animation: blink 1s steps(1) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  @keyframes beam { to { transform: scaleX(1); } }
  @keyframes fan { from { opacity: 0; transform: scaleX(0.2); } to { opacity: 0.95; transform: scaleX(1); } }
  pre { position: relative; margin: 0; width: 420px; min-height: 140px; font: 11.5px/1.65 var(--mono); color: var(--b-line); }
  pre b { color: var(--b-title); font-weight: 600; }
  pre span { display: inline; animation: line 0.15s ease-out; }
  pre i { font-style: normal; }
  pre .ok { color: var(--b-ok); }
  pre .warn, pre .later { color: var(--b-warn); }
  pre .wait { color: var(--b-faint); }
  pre small { font-size: inherit; color: var(--b-faint); }
  /* The active profile, on its color. */
  pre .prof { color: color-mix(in srgb, var(--c) 70%, #fff); animation: none; }
  pre .prof i { display: inline-block; width: 8px; height: 8px; margin-right: 6px; border-radius: var(--r2); background: var(--c); vertical-align: 0; }
  /* Time of each source: frozen once there, ticking while it loads. */
  pre em { font-style: normal; color: var(--b-faint); white-space: pre; }
  pre em.run { color: var(--b-line); }
  @keyframes line { from { opacity: 0; } }
  .skip { position: absolute; bottom: 16px; right: 20px; font-size: 11px; color: var(--b-faint); }
  /* The phone: the lines and the scenes at its width, the hint above its home bar. */
  :global(html.remote) pre, :global(html.remote) .ascii, :global(html.remote) .log { width: calc(100vw - 32px); font-size: 10.5px; }
  :global(html.remote) .ascii { font-size: 8px; }
  :global(html.remote) .skip { bottom: calc(16px + env(safe-area-inset-bottom)); }

  /* Synthwave: an Outrun drive in a pixel typeface (Press Start 2P, app.css), toward a sun rising with the progress (--p, 0 to 1). */
  @property --p { syntax: '<number>'; inherits: true; initial-value: 0; }
  .boot:is(.synthwave, .blueprint, .quest):not(.still) { transition: transform 0.6s cubic-bezier(0.7, 0, 0.2, 1), --p 0.5s ease-out; }
  .boot.synthwave { --b-accent: #36f9f6; --b-title: #fff; --b-line: #36f9f6; --b-faint: #7a66b3; --b-ok: #ffd75e; --b-warn: #ff2a6d; --b-star: #fff;
    justify-content: flex-start; gap: 0; padding-top: 13vh; font-family: 'Press Start 2P', var(--mono); image-rendering: pixelated; transition: transform 0.6s cubic-bezier(0.7, 0, 0.2, 1), --p 0.5s ease-out;
    /* The sky, from night to dawn as the sun rises; the horizon at 52 %. */
    background: linear-gradient(hsl(calc(262 - var(--p) * 12) 80% calc(4% + var(--p) * 8%)) 0%, hsl(calc(282 - var(--p) * 22) 70% calc(9% + var(--p) * 16%)) 26%, hsl(calc(318 - var(--p) * 14) 75% calc(16% + var(--p) * 30%)) 44%, hsl(calc(345 - var(--p) * 12) 95% calc(28% + var(--p) * 36%)) 52%, #05000f 52%); }
  .synthwave.still { transition: transform 0.6s cubic-bezier(0.7, 0, 0.2, 1); }
  .synthwave canvas { opacity: calc(0.9 - var(--p) * 0.6); }
  .sunset { position: absolute; inset: 0; pointer-events: none; }
  /* The sun behind the mountains and the road: it rises with the progress, its stripes closing as it climbs. */
  .sun { position: absolute; left: 50%; top: calc(52% + 16vh - var(--p) * 22vh); width: 34vh; height: 34vh; transform: translate(-50%, -50%); border-radius: 50%;
    background: linear-gradient(#ffe46b 0%, #ffb347 18%, #ff5f7e 32%, #ff2a9a 50%, #c01a8a 70%);
    box-shadow: 0 0 calc(4vh + var(--p) * 8vh) calc(var(--p) * 2vh) rgba(255, 110, 150, calc(0.25 + var(--p) * 0.35));
    mask: linear-gradient(#000 0 calc(30% + var(--p) * 30%), transparent 0 calc(32% + var(--p) * 30%), #000 0 calc(40% + var(--p) * 26%), transparent 0 calc(43% + var(--p) * 25%), #000 0 calc(50% + var(--p) * 22%), transparent 0 calc(54% + var(--p) * 21%), #000 0 calc(60% + var(--p) * 18%), transparent 0 calc(65% + var(--p) * 17%), #000 0); }
  .ridge { position: absolute; left: 0; right: 0; bottom: 48%; height: 8vh; background: linear-gradient(#2a0a4a, #0d0420);
    clip-path: polygon(0 100%, 0 62%, 7% 30%, 14% 58%, 22% 18%, 30% 52%, 36% 40%, 41% 70%, 59% 70%, 64% 34%, 71% 56%, 79% 12%, 87% 50%, 93% 28%, 100% 48%, 100% 100%); }
  /* The road toward the horizon, its center line and its bands rolling toward you. */
  .road { position: absolute; left: 0; right: 0; bottom: 0; height: 48%; overflow: hidden; background: linear-gradient(#1b0b33, #0a0418); }
  .road::before, .road::after { content: ''; position: absolute; left: 50%; bottom: 0; width: 160vw; height: 100%; transform: translateX(-50%); clip-path: polygon(48.5% 0, 51.5% 0, 100% 100%, 0 100%); }
  .road::before { background: repeating-linear-gradient(rgba(255, 42, 170, 0.15) 0 7%, transparent 7% 14%) #251a3a; }
  .road::after { background: repeating-linear-gradient(#ffd75e 0 6%, transparent 6% 14%) 50% 0 / 0.9% 100% no-repeat; }
  .synthwave:not(.still) .road::before, .synthwave:not(.still) .road::after { animation: lanes 0.35s linear infinite; }
  /* Everything has answered: the road speeds up, the palms with it. */
  .synthwave.ready .road::before, .synthwave.ready .road::after { animation-duration: 0.12s; }
  @keyframes lanes { from { background-position: 50% 0; } to { background-position: 50% 14%; } }
  .palm { position: absolute; bottom: 48%; left: 48%; width: 6vh; height: 12vh; transform-origin: 50% 100%; opacity: 0; --dx: -60vw; animation: palm 1.8s cubic-bezier(0.55, 0, 1, 0.45) infinite; }
  .palm.right { left: 50%; --dx: 58vw; }
  .palm svg { display: block; width: 100%; height: 100%; }
  .synthwave.ready .palm { animation-duration: 0.7s; }
  @keyframes palm { 0% { transform: translate(0, 0) scale(0.15); opacity: 0; } 10% { opacity: 1; } 100% { transform: translate(var(--dx), 52vh) scale(4); opacity: 1; } }
  /* The car follows the pointer, kept whole on screen. */
  .car { position: absolute; left: clamp(13vh, var(--steer, 50%), calc(100% - 13vh)); bottom: calc(4vh + 24px); width: 26vh; transform: translateX(-50%); }
  .synthwave:not(.still) .car { transition: left 0.35s ease-out; }
  .synthwave:not(.still) .car { animation: sway 2.4s ease-in-out infinite; }
  @keyframes sway { 50% { transform: translateX(calc(-50% + 1.2vh)); } }
  .hud { position: absolute; left: 28px; right: 28px; top: 22px; display: grid; grid-template-columns: 1fr auto 1fr; align-items: start; gap: 16px; font: 11px/1.4 'Press Start 2P', var(--mono); color: #fff; letter-spacing: 1px; }
  .hud > :last-child { text-align: right; }
  .hud b { color: #ff2a6d; font-weight: 400; margin-right: 6px; }
  .hud .clock { text-align: center; }
  .hud .clock b { display: block; margin: 0 0 4px; }
  .hud .clock span { font-size: 30px; color: #ffd75e; text-shadow: 3px 3px 0 #ff2a6d; }
  .marquee { position: relative; font: 34px/1 'Press Start 2P', var(--mono); letter-spacing: 6px; color: #fff; text-shadow: 0 0 16px #ff2a9a, 3px 3px 0 #ff2a6d; }
  .version { position: relative; margin-top: 12px; font: 9px/1 'Press Start 2P', var(--mono); letter-spacing: 2px; color: #d9c8ff; }
  .banner { position: relative; display: flex; flex-direction: column; align-items: center; gap: 8px; margin-top: 5vh; font: 10px/1.4 'Press Start 2P', var(--mono); color: #fff; text-align: center; padding: 0 16px; animation: line 0.15s ease-out; }
  .banner b { font: 400 18px/1 'Press Start 2P', var(--mono); letter-spacing: 3px; color: #36f9f6; text-shadow: 3px 3px 0 #4a0c63; }
  .banner b.warn, .banner b.later { color: #ff2a6d; text-shadow: 3px 3px 0 #1a0630; }
  .banner.goal { font-size: 34px; letter-spacing: 5px; color: #ffd75e; text-shadow: 4px 4px 0 #ff2a6d; }
  /* The sun whole: a flash. */
  .flash { position: absolute; inset: 0; background: #ffe9a8; opacity: 0; pointer-events: none; }
  .synthwave.ready:not(.still) .flash { animation: flash 0.6s ease-out 1; }
  @keyframes flash { from { opacity: 0.7; } to { opacity: 0; } }
  /* Every source and its checkpoint's time, bottom left, on a veil readable over the road. */
  .laps { position: absolute; left: 24px; bottom: 44px; width: auto; min-height: 0; padding: 8px 12px; background: rgba(5, 0, 15, 0.55); font: 9px/1.9 'Press Start 2P', var(--mono); color: #36f9f6; }
  .laps .ok { color: #fff; } .laps .warn, .laps .later { color: #ff2a6d; } .laps .wait { color: #7a66b3; }
  .synthwave .cur { width: 8px; height: 9px; background: #fff; }
  .speed { position: absolute; right: 24px; bottom: 44px; display: flex; flex-direction: column; align-items: flex-end; font: 9px/1 'Press Start 2P', var(--mono); color: #fff; }
  .speed > span:first-child { font-size: 26px; color: #36f9f6; margin-bottom: 6px; }
  .tacho { display: flex; gap: 3px; margin-top: 8px; }
  .tacho i { width: 7px; height: 16px; background: #2a1d48; }
  .tacho i.on { background: #36f9f6; } .tacho i.on.hot { background: #ff2a6d; }
  /* The phone: the pixel typeface is as wide as tall; the sources above the car, the speed in the HUD's place. */
  :global(html.remote) .hud { left: 10px; right: 10px; top: calc(14px + env(safe-area-inset-top)); gap: 8px; font-size: 6.5px; }
  :global(html.remote) .hud span { white-space: nowrap; }
  :global(html.remote) .hud .clock span { font-size: 20px; }
  :global(html.remote) .marquee { font-size: 26px; letter-spacing: 4px; }
  :global(html.remote) .version { font-size: 7px; }
  :global(html.remote) .banner { font-size: 7px; }
  :global(html.remote) .banner b { font-size: 12px; }
  :global(html.remote) .banner.goal { font-size: 24px; }
  :global(html.remote) .car { width: 34vw; left: clamp(17vw, var(--steer, 50%), calc(100% - 17vw)); bottom: calc(40px + env(safe-area-inset-bottom)); }
  :global(html.remote) .laps { left: 8px; right: 8px; bottom: calc(52px + 16vw + env(safe-area-inset-bottom)); font-size: 7px; overflow: hidden; }
  :global(html.remote) .speed { display: none; }
  .synthwave::after, .hacker::after { content: ''; position: absolute; inset: 0; pointer-events: none; background: repeating-linear-gradient(transparent 0 2px, rgba(0, 0, 0, 0.18) 2px 3px); }

  /* Hacker: a green phosphor terminal booting, a rain of characters behind. */
  .boot.hacker { --b-accent: #39ff14; justify-content: center; align-items: center; background: radial-gradient(ellipse at 50% 50%, #071a07, #020402 75%); color: #39ff14; }
  .hacker::after { background: repeating-linear-gradient(transparent 0 2px, rgba(0, 0, 0, 0.35) 2px 3px), radial-gradient(ellipse at center, transparent 60%, rgba(0, 0, 0, 0.65)); }
  .rain { position: absolute; inset: 0; overflow: hidden; opacity: 0.2; pointer-events: none; font: 13px/15px var(--mono); color: #39ff14; }
  .rain span { position: absolute; top: -60%; writing-mode: vertical-rl; text-orientation: upright; animation: fall linear infinite; }
  @keyframes fall { to { top: 110%; } }
  .ascii { min-height: 0; width: 560px; font: 12px/1.15 var(--mono); color: #39ff14; text-shadow: 0 0 8px #39ff14; }
  .hacker pre { color: #39ff14; text-shadow: 0 0 6px rgba(57, 255, 20, 0.55); }
  .log { width: 560px; min-height: 180px; }
  .hacker pre .dim, .hacker pre small { color: #1f7a12; text-shadow: none; }
  .hacker pre .amber { color: #ffb000; text-shadow: 0 0 6px rgba(255, 176, 0, 0.6); }
  .hacker pre i.ok { padding: 0 1px; background: #39ff14; color: #020402; text-shadow: none; }
  .hacker pre i.warn, .hacker pre i.later { color: #ff3b3b; text-shadow: 0 0 6px rgba(255, 59, 59, 0.6); }
  .hacker pre i.wait { color: #ffb000; }
  .hacker .cur { background: #39ff14; box-shadow: 0 0 6px #39ff14; }
  .hacker .skip { color: #1f7a12; }
  .grant { position: absolute; left: 50%; top: 50%; z-index: 1; transform: translate(-50%, -50%); padding: 10px 26px; border: 1px solid #39ff14; background: #020402; color: #39ff14; font: 700 22px var(--mono); letter-spacing: 6px; text-shadow: 0 0 10px #39ff14; box-shadow: 0 0 24px rgba(57, 255, 20, 0.35); }
  :global(html.remote) .grant { padding: 8px 14px; font-size: 16px; letter-spacing: 3px; white-space: nowrap; }
  .boot:not(.still) .grant { animation: grant 0.5s steps(1) infinite; }
  @keyframes grant { 60% { opacity: 0.15; } }

  /* Blueprint: an architect's plan on blue paper; the prism's outline drawn with the progress (--p). */
  .boot.blueprint { --b-accent: #fff; --b-line: #eef5ff; --b-faint: #9fc0e6; --b-ok: #b7f5c4; --b-warn: #ffb3a1;
    display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); align-items: center; gap: 4vw; padding: 6vh 6vw 18vh; color: #eef5ff; font-family: 'Jost', var(--sans);
    background: repeating-linear-gradient(0deg, rgba(255, 255, 255, 0.12) 0 1px, transparent 1px 100px), repeating-linear-gradient(90deg, rgba(255, 255, 255, 0.12) 0 1px, transparent 1px 100px),
      repeating-linear-gradient(0deg, rgba(255, 255, 255, 0.05) 0 1px, transparent 1px 20px), repeating-linear-gradient(90deg, rgba(255, 255, 255, 0.05) 0 1px, transparent 1px 20px), radial-gradient(ellipse at 40% 40%, #1f5c9e, #12396a); }
  .blueprint::before { content: ''; position: absolute; inset: 16px; border: 2px solid rgba(234, 243, 255, 0.8); pointer-events: none; }
  .plan { width: 100%; max-height: 62vh; overflow: visible; }
  .plan .tri { fill: none; stroke: #fff; stroke-width: 0.5; stroke-linejoin: round; stroke-dasharray: 1; stroke-dashoffset: calc(1 - var(--p)); }
  .plan .axis { stroke: rgba(255, 255, 255, 0.4); stroke-width: 0.25; stroke-dasharray: 1.4 1; }
  .plan .arc, .plan .dim { fill: none; stroke: rgba(255, 255, 255, 0.55); stroke-width: 0.25; }
  .plan .ray { stroke: #fff; stroke-width: 0.35; stroke-dasharray: 1; stroke-dashoffset: 1; }
  .blueprint.ready .plan .ray { stroke-dashoffset: 0; }
  .blueprint.ready:not(.still) .plan .ray { transition: stroke-dashoffset 0.6s ease-out; }
  .plan text { fill: #eef5ff; font: 2.6px 'Jost', var(--sans); }
  .plan .mark circle { fill: #174780; stroke: #fff; stroke-width: 0.3; }
  .plan .mark text { font-size: 2.2px; font-weight: 600; text-anchor: middle; }
  .plan .mark:is(.warn, .later) circle { stroke: #ffb3a1; }
  .plan .mark:is(.warn, .later) text { fill: #ffb3a1; }
  .boot:not(.still) .plan .mark { animation: line 0.3s ease-out; }
  .legend { margin: 0; padding: 0; list-style: none; display: grid; gap: 8px; font: 500 13px/1.25 'Jost', var(--sans); letter-spacing: 0.5px; text-transform: uppercase; }
  .legend li { display: flex; gap: 10px; align-items: flex-start; animation: line 0.15s ease-out; }
  .legend b { flex: none; width: 20px; height: 20px; display: grid; place-items: center; border: 1.5px solid #fff; border-radius: 50%; font-size: 10.5px; font-weight: 600; }
  .legend small { display: block; font-size: 11px; text-transform: none; letter-spacing: 0; color: #9fc0e6; }
  .legend .wait { color: #9fc0e6; }
  .legend .wait b { border-style: dashed; border-color: #9fc0e6; }
  .legend :is(.warn, .later) b { border-color: #ffb3a1; color: #ffb3a1; }
  .legend :is(.warn, .later) small { color: #ffb3a1; }
  .legend .cur { display: inline-block; width: 7px; height: 13px; margin-left: 2px; background: #fff; vertical-align: -2px; }
  .boot:not(.still) .legend .cur { animation: blink 1s steps(1) infinite; }
  /* The title block, bottom right, in the frame's corner. */
  .cartouche { position: absolute; right: 16px; bottom: 16px; display: grid; grid-template-columns: 2fr 1fr 1fr 1fr; border-top: 2px solid rgba(234, 243, 255, 0.8); border-left: 2px solid rgba(234, 243, 255, 0.8); font: 500 12px/1.2 'Jost', var(--sans); text-transform: uppercase; letter-spacing: 0.5px; }
  .cartouche span { padding: 8px 14px; border-right: 1px solid rgba(234, 243, 255, 0.5); }
  .cartouche span:last-child { border-right: 0; }
  .cartouche small { display: block; font-size: 9px; color: #9fc0e6; margin-bottom: 3px; }
  .cartouche .big { font-size: 22px; font-weight: 600; letter-spacing: 5px; align-self: center; }
  .stamp { position: absolute; left: 50%; top: 76%; padding: 8px 22px; border: 5px double #ff6a5a; color: #ff6a5a; font: 700 40px/1 'Jost', var(--sans); letter-spacing: 6px; transform: translate(-50%, -50%) rotate(-12deg); mix-blend-mode: screen; }
  .boot:not(.still) .stamp { animation: stamp 0.25s cubic-bezier(0.5, 0, 0.3, 1.4) both; }
  @keyframes stamp { from { opacity: 0; transform: translate(-50%, -50%) rotate(-12deg) scale(2.4); } }
  .blueprint .skip { color: #9fc0e6; right: auto; left: 32px; bottom: 28px; }
  :global(html.remote) .boot.blueprint { grid-template-columns: 1fr; align-content: center; gap: 18px; padding: calc(24px + env(safe-area-inset-top)) 28px 120px; }
  :global(html.remote) .plan { max-height: 30vh; }
  :global(html.remote) .legend { font-size: 11px; gap: 5px; }
  :global(html.remote) .cartouche { left: 16px; bottom: calc(16px + env(safe-area-inset-bottom)); grid-template-columns: 1.6fr 1fr 1fr; }
  :global(html.remote) .cartouche span:last-child { display: none; }
  :global(html.remote) .cartouche span:nth-child(3) { border-right: 0; }
  :global(html.remote) .stamp { top: 22%; font-size: 28px; }
  :global(html.remote) .blueprint .skip { left: 0; right: 0; text-align: center; bottom: calc(84px + env(safe-area-inset-bottom)); }

  /* Quest: an 8-bit RPG in the pixel typeface; the party walks, the XP bar is the progress. */
  .boot.quest { --b-accent: #ffd84a; justify-content: flex-start; gap: 0; padding-top: 16vh; font-family: 'Press Start 2P', var(--mono); color: #fff; image-rendering: pixelated; background: linear-gradient(#5c94fc 0 58%, #3a8a2a 58% 62%, #2f6e22 62%); }
  .field { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .cloud { position: absolute; top: 9vh; left: 70%; width: 12vh; height: 3.4vh; background: #fff; box-shadow: 1.8vh -1.8vh 0 #fff, 5vh -1.8vh 0 #fff, 3.4vh -3.2vh 0 #fff; }
  .cloud.b { top: 22vh; left: 12%; transform: scale(0.7); }
  .boot:not(.still) .cloud { animation: cloud 26s linear infinite; }
  .boot:not(.still) .cloud.b { animation-duration: 36s; animation-delay: -14s; }
  @keyframes cloud { from { left: 110%; } to { left: -25%; } }
  .hills { position: absolute; left: 0; right: 0; top: 46%; height: 12%; background: radial-gradient(ellipse at 15% 100%, #2f9a3a 0 40%, transparent 41%), radial-gradient(ellipse at 55% 100%, #278a32 0 34%, transparent 35%), radial-gradient(ellipse at 88% 100%, #2f9a3a 0 44%, transparent 45%); }
  .ground { position: absolute; left: 0; right: 0; top: 62%; bottom: 0; background: repeating-linear-gradient(90deg, transparent 0 4vh, rgba(0, 0, 0, 0.12) 4vh 8vh); }
  .boot.quest:not(.still):not(.ready) .ground { animation: walk 0.6s steps(4) infinite; }
  @keyframes walk { to { background-position: -8vh 0; } }
  .quest .hud { grid-template-columns: auto 1fr auto; text-shadow: 2px 2px 0 #000; }
  .quest .hud b { color: #ffd84a; }
  .quest .xp { justify-self: center; display: flex; align-items: center; gap: 3px; }
  .quest .xp b { margin-right: 8px; }
  .quest .xp i { width: 10px; height: 12px; background: #133; box-shadow: 0 0 0 2px #000; }
  .quest .xp i.on { background: #6cff4a; }
  .quest .marquee { color: #ffd84a; text-shadow: 4px 4px 0 #b03a0a, 7px 7px 0 #000; }
  .quest .version { color: #fff; text-shadow: 2px 2px 0 #000; }
  .lvl { position: relative; margin-top: 4vh; font: 28px/1 'Press Start 2P', var(--mono); color: #fff; text-shadow: 4px 4px 0 #000; }
  .boot:not(.still) .lvl { animation: grant 0.4s steps(1) 3; }
  .party { position: absolute; left: 0; right: 0; top: calc(60% - 7.5vh); display: flex; flex-direction: row-reverse; justify-content: center; gap: 1vh; }
  .hero { width: 5vh; height: 7.5vh; }
  .hero.join:not(.on) { opacity: 0; transform: translateX(-6vh); }
  .boot:not(.still) .hero { transition: opacity 0.3s, transform 0.4s; }
  .boot:not(.still):not(.ready) .hero.on { animation: step 0.3s steps(2) infinite; }
  .boot.ready:not(.still) .hero.on { animation: jump 0.5s steps(3) infinite; }
  @keyframes step { 50% { transform: translateY(-0.5vh); } }
  @keyframes jump { 50% { transform: translateY(-2.6vh); } }
  .dialog { position: absolute; left: 50%; bottom: 7vh; width: min(720px, calc(100vw - 48px)); min-height: 108px; transform: translateX(-50%); padding: 18px 22px; background: #102a8a; border: 4px solid #fff; border-radius: 6px; box-shadow: 0 0 0 4px #000, inset 0 0 0 3px #000; font: 12px/2 'Press Start 2P', var(--mono); color: #fff; }
  .dialog b { font-weight: 400; color: #ffd84a; }
  .dialog b:is(.warn, .later) { color: #ff7a7a; }
  .dialog .next { position: absolute; right: 16px; bottom: 10px; font-size: 10px; }
  .boot:not(.still) .dialog .next { animation: blink 0.6s steps(1) infinite; }
  .quest .skip { color: #fff; text-shadow: 2px 2px 0 #000; bottom: calc(7vh + 120px); right: 24px; font-family: var(--sans); }
  :global(html.remote) .boot.quest { padding-top: calc(16vh + env(safe-area-inset-top)); }
  :global(html.remote) .quest .marquee { font-size: 22px; }
  :global(html.remote) .quest .xp i { width: 6px; height: 9px; }
  :global(html.remote) .lvl { font-size: 20px; }
  :global(html.remote) .party { top: calc(60% - 10.5vw); }
  :global(html.remote) .hero { width: 7vw; height: 10.5vw; }
  :global(html.remote) .dialog { bottom: calc(40px + env(safe-area-inset-bottom)); font-size: 9px; padding: 14px 16px; min-height: 90px; }
  :global(html.remote) .quest .skip { bottom: calc(150px + env(safe-area-inset-bottom)); left: 0; right: 0; text-align: center; }
</style>
