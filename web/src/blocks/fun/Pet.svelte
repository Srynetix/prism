<script lang="ts">
  import { locale } from '../../locale'
  import { t, tn } from '../../i18n.svelte'
  import { onMount, untrack } from 'svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import { celebrateFrom } from '../../fun.svelte'
  import { isCollapsed, isShort, pref } from '../../layout.svelte'
  import { petShown, petStore, savePet } from './pet-store.svelte'
  import { hasCore } from '../../bridge'
  import { alerts, waitsForYou } from '../../rules'
  import { anchored, dismissable } from '../../popup'
  import { cliOf } from '../../agent-cli'
  import { agentRepo } from '../../repositories'
  import { isArchived } from '../../agent-archive.svelte'
  import { openAgents } from '../../agents.svelte'
  import { openMr } from '../../mr-view.svelte'
  import type { BlockProps } from '../block'
  import type { Agent } from '../../types'
  import type { TrailState } from '../trail/trail'
  import Creature from './Creature.svelte'
  import { errands, type Errand } from './errands'
  import { daypart, season, specials, type Special } from './world'
  import { count, dueWeeks, FORMS, NEW_GROWTH, pickForm, scores, takeShots, trackGreen, trackSprint, TROPHIES, unlock, writePage, type Face, type Growth, type Shot, type Stage } from './growth'

  /**
   * A virtual companion. Three needs (food, energy, fun, 0 to 100) run down with time, Prism
   * closed included (caught up from the last visit); feeding, playing, petting and sleep fill them.
   * Experience comes from each care and each merged MR (once per MR): its level, and its look as it
   * grows. It reacts to the work: sick with a red protected pipeline, worried by red alerts, it
   * says when an agent waits or a meeting is close, and celebrates a merge. Asleep at night, at
   * the weekend, or with its light off. Its state stays in the layout's prefs (`pet`). Short: one
   * row, the creature small with its needs and its cares, still to feed, play with and pet.
   *
   * With the Day trail's journal it also grows into your work (`[pet]`, see `growth.ts`): grown up,
   * the form of what you did most these two weeks; trophies, each an object in its room; and a
   * diary page on your week, written on Friday afternoon; a photo album of its milestones.
   *
   * Its room holds its friends, the running agents (the one waiting for you waving); its window
   * follows the hour, the season and a few special days; Fetch brings back the next thing waiting
   * for you. `place: 'status'`: its status bar item, while no Pet block is shown — a small Pet and
   * a popover of its short row.
   */
  type Settings = { forms: boolean; trophies: boolean; diary: boolean; friends: boolean; fetch: boolean; seasons: boolean; status_bar: boolean; album: boolean }
  let { state: settings, snap, place = 'block' }: BlockProps<Settings> & { place?: 'block' | 'status' } = $props()
  const cfg = $derived({
    forms: settings?.forms ?? true,
    trophies: settings?.trophies ?? true,
    diary: settings?.diary ?? true,
    friends: settings?.friends ?? true,
    fetch: settings?.fetch ?? true,
    seasons: settings?.seasons ?? true,
    album: settings?.album ?? true,
  })
  onMount(() => {
    if (place !== 'block') return
    petShown.n++
    return () => petShown.n--
  })

  interface Pet {
    name: string
    born: number
    food: number
    energy: number
    fun: number
    xp: number
    /** Its light, turned off by hand: it sleeps. */
    dark: boolean
    /** The merged MRs it already got experience from. */
    seen: string[]
    /** When its needs were last brought up to date. */
    at: number
    /** What it became with your work. */
    growth: Growth
    /** What it last fetched, and when: done within the hour, a bonus. */
    fetched: { key: string; at: number } | null
  }
  const NEW: Pet = { name: 'Pixel', born: Date.now(), food: 80, energy: 80, fun: 80, xp: 0, dark: false, seen: [], at: Date.now(), growth: structuredClone(NEW_GROWTH), fetched: null }
  const kept = (hasCore() ? (petStore.value ?? {}) : pref<Partial<Pet>>('pet', {})) as Partial<Pet>
  const restore = (k: Partial<Pet>): Pet => ({ ...NEW, ...k, growth: { ...structuredClone(NEW_GROWTH), ...k.growth, shot: { ...NEW_GROWTH.shot, ...k.growth?.shot } } })
  let p = $state<Pet>(restore(kept))
  const save = () => savePet($state.snapshot(p) as unknown as Record<string, unknown>)
  // The same pet on the Mac and the phones: one written elsewhere comes in.
  let seenVersion = petStore.version
  $effect(() => {
    if (petStore.version === seenVersion || !petStore.value) return
    seenVersion = petStore.version
    p = restore(petStore.value as Partial<Pet>)
  })
  const clamp = (n: number) => Math.max(0, Math.min(100, n))

  let now = $state(Date.now())
  /** Asleep: its light off, the night, or the weekend. */
  const asleep = $derived.by(() => {
    const d = new Date(now)
    return p.dark || d.getDay() === 0 || d.getDay() === 6 || d.getHours() >= 22 || d.getHours() < 7
  })

  /**
   * Needs run down by the hour; asleep, energy comes back and hunger slows. Its light off, a nap:
   * energy back in about half an hour (+150 an hour); the night's sleep, longer, slower (+15).
   */
  function tick() {
    now = Date.now()
    const hours = Math.min(48, (now - p.at) / 3_600_000)
    if (hours <= 0) return
    const rest = p.dark ? 150 : asleep ? 15 : -5
    const was = p.energy
    p.food = clamp(p.food - hours * (asleep ? 3 : 7))
    p.fun = clamp(p.fun - hours * (asleep ? 1 : 6))
    p.energy = clamp(p.energy + hours * rest)
    p.at = now
    if (p.dark && was < 100 && p.energy === 100) say(t('Fully rested! Turn the light on?'), 'sleep', 8000)
    // Written once a minute at most: the needs move little in between.
    if (now - saved > 60_000) {
      saved = now
      save()
    }
  }
  let saved = 0

  /** A merged MR it has not counted yet: experience, and a celebration. */
  $effect(() => {
    const merged = snap.merge_requests.filter((m) => m.state === 'merged').map((m) => `${m.repo ?? ''}!${m.iid}`)
    const fresh = merged.filter((k) => !p.seen.includes(k))
    if (!fresh.length) return
    // The first time, those already there are only noted: no experience for the past.
    const first = !p.seen.length && p.xp === 0
    p.seen = [...p.seen, ...fresh].slice(-300)
    if (!first) {
      p.xp += 10 * fresh.length
      say(t('A merge! +{xp} XP', { xp: 10 * fresh.length }), 'party')
      if (body) celebrateFrom(body, { count: 24 })
    }
    save()
  })

  const level = $derived(1 + Math.floor(Math.sqrt(p.xp / 10)))
  const stage = $derived<Stage>(level < 3 ? 'baby' : level < 6 ? 'young' : level < 10 ? 'grown-up' : 'legend')

  /* ---------- What it becomes with your work ---------- */

  const trail = $derived(snap.blocks?.trail as unknown as TrailState | undefined)
  const grown = $derived(stage === 'grown-up' || stage === 'legend')
  const form = $derived(cfg.forms && grown ? p.growth.form : null)
  const WEEK = 7 * 86_400_000
  const sig = () => `— ${p.name}, ${t('Lv {n}', { n: level })}${form ? `, ${FORMS[form].label}` : ''}`
  /** Counts the new traces, then a form, trophies and diary pages when they are due. */
  function grow() {
    const g = p.growth
    const before = JSON.stringify(g)
    count(g, trail, snap)
    trackGreen(g, snap)
    trackSprint(g, snap)
    if (cfg.forms && grown && Date.now() - g.formAt > WEEK) {
      const f = pickForm(scores(trail, snap))
      if (f) {
        if (f !== g.form) say(t('I’m becoming {form}: {why}!', { form: FORMS[f].label, why: FORMS[f].why }), 'party', 7000)
        g.form = f
        g.formAt = Date.now()
      }
    }
    const won = cfg.trophies ? unlock(g, level, days) : []
    for (const tr of won) {
      say(t('New trophy: {label} {icon}', { label: tr.label, icon: tr.icon }), 'party', 7000)
      if (body) celebrateFrom(body, { count: 30 })
    }
    if (cfg.album && takeShots(g, { stage, form }, p.born, won).length && !won.length) say(t('Click! A new photo in my album 📷'), 'party', 5000)
    if (cfg.diary) {
      for (const week of dueWeeks(g)) {
        const page = writePage(week, trail, g, sig())
        if (!page) continue
        g.diary = [...g.diary, page].sort((a, b) => a.week.localeCompare(b.week)).slice(-8)
        say(t('I wrote in my diary 📔'), 'happy', 6000)
      }
    }
    if (JSON.stringify(g) !== before) save()
  }
  $effect(() => {
    void trail
    void snap.pipelines
    void snap.sprint
    void level
    untrack(grow)
  })

  let view = $state<'pet' | 'trophies' | 'diary' | 'album'>('pet')
  let page = $state(-1)
  const diary = $derived(p.growth.diary)
  const shownPage = $derived(diary[page < 0 ? diary.length - 1 : page])
  const won = $derived(TROPHIES.filter((tr) => p.growth.trophies[tr.id]))
  const has = (id: string) => cfg.trophies && !!p.growth.trophies[id]
  const album = $derived(cfg.album ? p.growth.album : [])
  const shortDate = (at: number) => new Date(at).toLocaleDateString(locale(), { day: 'numeric', month: 'short' })

  /* ---------- Its friends: the agents ---------- */

  const FRIEND_SLOTS = [44, 84, 236]
  const friends = $derived(cfg.friends ? (snap.agents ?? []).filter((a) => a.state === 'running' && !isArchived(a)).sort((a, b) => Number(waitsForYou(b.status)) - Number(waitsForYou(a.status)) || a.created_at.localeCompare(b.created_at)) : [])
  const friendFace = (a: Agent): Face => (waitsForYou(a.status) ? 'wow' : a.status === 'running' || a.status === 'background' ? 'happy' : 'tired')
  const friendTip = (a: Agent) => {
    const what = waitsForYou(a.status) ? t('waits for you') : a.status === 'running' || a.status === 'background' ? t('working') : t('at its prompt')
    return t('{cli} on {repo} · {title} · {what}', { cli: cliOf(a.provider).name, repo: agentRepo(a, snap) ?? '—', title: a.title, what })
  }

  /* ---------- Its window ---------- */

  const part = $derived(daypart(now))
  const seasonNow = $derived(season(now))
  const today = $derived(cfg.seasons ? specials(now, p.born, snap.sprint?.end) : [])
  const SKY = { dawn: '#f2b48a', day: '#9fcbeb', dusk: '#d98a57', night: '#141d33' }
  /** The day's object on its floor: a cake, a pumpkin, a tree… */
  const FEAST: Partial<Record<Special, string>> = { birthday: '🎂', halloween: '🎃', christmas: '🎄', 'new-year': '🥂' }
  const feast = $derived(today.find((d) => d in FEAST))
  /** Stars, the same every night. */
  const STARS = [[24, 22], [32, 30], [41, 19], [58, 26], [70, 21], [27, 52], [64, 56], [74, 47]]

  /* ---------- What it feels, and says ---------- */

  let said = $state<{ text: string; face: Face; until: number } | null>(null)
  function say(text: string, face: Face = 'happy', ms = 4000) {
    said = { text, face, until: Date.now() + ms }
  }
  const CHATTER = [t('Hi! What are we building today?'), t('I like it when the CI is green.'), t('Did you drink some water?'), t('Psst… your MRs miss you.'), t('I believe in you.'), t('Ship small, ship often.'), t('*stretches*'), t('Tell me when you merge something!')]
  const SPECIAL_SAYS: Partial<Record<Special, string>> = {
    birthday: t('It’s my birthday! A year together 🎂'),
    'sprint-end': t('Last day of the sprint! Hang in there.'),
    'no-deploy': t('Friday afternoon: no deploy, OK?'),
    halloween: t('Boo! 🎃'),
    christmas: t('Merry Christmas! 🎄'),
    'new-year': t('Happy new year! 🎆'),
  }
  const mood = $derived.by((): { face: Face; text: string } => {
    if (said && said.until > now) return { face: said.face, text: said.text }
    if (asleep) return { face: 'sleep', text: p.dark ? t('Zzz… (light off)') : t('Zzz… it is off duty.') }
    const red = snap.pipelines.find((x) => x.status === 'failed')
    if (red) return { face: 'sick', text: t('{target} is red… I feel unwell.', { target: red.target }) }
    if (p.food < 25) return { face: 'sad', text: t('I’m hungry…') }
    if (p.energy < 20) return { face: 'tired', text: t('So tired… turn the light off?') }
    if (p.fun < 25) return { face: 'sad', text: t('I’m bored. Play with me?') }
    if (alerts(snap).some((a) => a.severity === 'ko')) return { face: 'sad', text: t('Something is broken: look at Alerts.') }
    const waiting = (snap.agents ?? []).find((a) => a.state === 'running' && a.status === 'needs_you')
    if (waiting) return { face: 'wow', text: cfg.friends ? t('{cli} on {repo} is waiting for you!', { cli: cliOf(waiting.provider).name, repo: agentRepo(waiting, snap) ?? waiting.title }) : t('An agent is waiting for you!') }
    const soon = (snap.agenda?.events ?? []).find((e) => !e.all_day && !e.canceled && +new Date(e.start) > now && +new Date(e.start) - now < 10 * 60_000)
    if (soon) return { face: 'wow', text: t('“{title}” starts soon!', { title: soon.title }) }
    const special = today.map((d) => SPECIAL_SAYS[d]).find(Boolean)
    // Every other line of its chatter, not all day long.
    if (special && Math.floor(now / 120_000) % 2 === 0) return { face: 'party', text: special }
    return { face: 'happy', text: CHATTER[Math.floor(now / 120_000) % CHATTER.length] }
  })

  /* ---------- Cares ---------- */

  function care(xp: number) {
    p.xp += xp
    p.at = Date.now()
    save()
  }
  function feed() {
    tick()
    if (asleep && !p.dark) return say(t('Shh… it is sleeping.'), 'sleep')
    if (p.food > 90) return say(t('I’m full!'), 'happy')
    p.food = clamp(p.food + 30)
    care(1)
    say(t('Yum!'), 'party', 2500)
    celebrateFrom(body, { count: 12, colors: ['#e8a33d', '#c96442', '#d4a017'] })
  }
  let jumping = $state(false)
  function play() {
    tick()
    if (asleep) return say(p.dark ? t('Turn the light on first.') : t('Shh… it is sleeping.'), 'sleep')
    if (p.energy < 15) return say(t('Too tired to play…'), 'tired')
    p.fun = clamp(p.fun + 25)
    p.energy = clamp(p.energy - 10)
    p.food = clamp(p.food - 5)
    care(2)
    jumping = true
    setTimeout(() => (jumping = false), 900)
    say(t('Again! Again!'), 'party', 2500)
  }
  let hearts = $state<number[]>([])
  function stroke() {
    tick()
    if (asleep) return say(t('*purrs in its sleep*'), 'sleep', 2000)
    p.fun = clamp(p.fun + 4)
    care(1)
    const id = Date.now()
    hearts = [...hearts, id]
    setTimeout(() => (hearts = hearts.filter((h) => h !== id)), 1000)
    say('♥', 'happy', 1500)
  }
  /** What it holds in its mouth, back from Fetch: shown until done, or until the next Fetch. */
  let carried = $state<Errand | null>(null)
  /** The things waiting for you; it fetched `skip` of them already in a row. */
  const waitingList = $derived(cfg.fetch ? errands(snap, now) : [])
  let skip = 0
  function fetchIt() {
    tick()
    if (asleep) return say(p.dark ? t('Turn the light on first.') : t('Shh… it is sleeping.'), 'sleep')
    if (p.energy < 10) return say(t('Too tired to run…'), 'tired')
    p.energy = clamp(p.energy - 3)
    p.fun = clamp(p.fun + 5)
    jumping = true
    setTimeout(() => (jumping = false), 900)
    const list = waitingList
    if (!list.length) {
      carried = null
      skip = 0
      return say(t('Nothing waits for you. Well done! 🪵'), 'party', 4000)
    }
    // Again, while it still holds one: the next one, then back to the first.
    const i = carried && list.some((x) => x.key === carried!.key) ? skip : 0
    const e = list[i % list.length]
    skip = i + 1
    carried = e
    p.fetched = { key: e.key, at: Date.now() }
    save()
    say(t('Here! {text}', { text: e.text }), 'wow', 6000)
  }
  function openCarried() {
    if (!carried) return
    const o = carried.open
    if ('agent' in o) openAgents(o.agent)
    else openMr(o.mr)
  }
  /** What it fetched is done within the hour: a bonus. Gone later, or another one fetched, nothing. */
  $effect(() => {
    const f = p.fetched
    if (!f || waitingList.some((x) => x.key === f.key)) return
    untrack(() => {
      if (Date.now() - f.at < 3_600_000) {
        p.xp += 5
        say(t('Good job! +5 XP'), 'party', 4000)
        if (body) celebrateFrom(body, { count: 16 })
      }
      p.fetched = null
      if (carried?.key === f.key) carried = null
      save()
    })
  })

  function light() {
    tick()
    p.dark = !p.dark
    save()
    say(p.dark ? t('Good night…') : t('Good morning!'), p.dark ? 'sleep' : 'wow', 2000)
  }

  /* ---------- In the status bar ---------- */

  let chip: HTMLButtonElement | undefined = $state()
  let popOpen = $state(false)

  /* ---------- Its name ---------- */

  let naming = $state(false)
  function rename(v: string) {
    naming = false
    const name = v.trim().slice(0, 20)
    if (name && name !== p.name) {
      p.name = name
      save()
      say(t('{name}? I love it!', { name }), 'party')
    }
  }

  /* ---------- Eyes that follow the pointer, a blink now and then ---------- */

  let body: SVGSVGElement | undefined = $state()
  let look = $state({ x: 0, y: 0 })
  let blink = $state(false)
  onMount(() => {
    tick()
    // Every 10 s: a nap's energy fills the bar as you watch.
    const timer = setInterval(tick, 10_000)
    const move = (e: PointerEvent) => {
      const r = body?.getBoundingClientRect()
      if (!r) return
      const dx = e.clientX - (r.left + r.width / 2)
      const dy = e.clientY - (r.top + r.height / 2)
      const d = Math.hypot(dx, dy) || 1
      const k = Math.min(1, d / 200)
      look = { x: (dx / d) * 3 * k, y: (dy / d) * 2.5 * k }
    }
    let b: ReturnType<typeof setTimeout>
    const blinking = () => {
      blink = true
      setTimeout(() => (blink = false), 140)
      b = setTimeout(blinking, 2500 + Math.random() * 3500)
    }
    b = setTimeout(blinking, 2000)
    window.addEventListener('pointermove', move)
    return () => {
      clearInterval(timer)
      clearTimeout(b)
      window.removeEventListener('pointermove', move)
    }
  })

  const days = $derived(Math.max(0, Math.floor((now - p.born) / 86_400_000)))
  const BARS = [
    { key: 'food', label: t('Food'), icon: '🍪' },
    { key: 'energy', label: t('Energy'), icon: '⚡' },
    { key: 'fun', label: t('Fun'), icon: '🎈' },
  ] as const
  const tone = (v: number) => (v < 25 ? 'ko' : v < 50 ? 'warn' : 'ok')
  const closed = $derived(blink || mood.face === 'sleep')
</script>

{#snippet creature()}
  <Creature bind:el={body} {stage} {form} face={mood.face} {look} {closed} {jumping} {hearts} label={t('{name}: pet it', { name: p.name })} onpoke={stroke} />
{/snippet}

{#snippet needs()}
  {#each BARS as b (b.key)}
    <span class="bar" title={t('{label}: {n}%', { label: b.label, n: Math.round(p[b.key]) })}>
      <span class="ico" aria-hidden="true">{b.icon}</span>
      <span class="track"><i class={tone(p[b.key])} style:width="{p[b.key]}%"></i></span>
    </span>
  {/each}
{/snippet}

<!-- The cares; `compact`: their icons alone (the short mode), their names in the tooltip. -->
{#snippet cares(compact: boolean)}
  <button onclick={feed} title={t('Feed it (+food)')} aria-label={t('Feed')}>🍪{#if !compact}<span>{t('Feed')}</span>{/if}</button>
  <button onclick={play} title={t('Throw it the ball (+fun, −energy)')} aria-label={t('Play')}>🎾{#if !compact}<span>{t('Play')}</span>{/if}</button>
  {#if cfg.fetch}<button onclick={fetchIt} title={waitingList.length ? tn(waitingList.length, 'Fetch: it brings back what waits for you ({n} thing)', 'Fetch: it brings back what waits for you ({n} things)') : t('Fetch: it brings back what waits for you (nothing now)')} aria-label={t('Fetch')}>🦴{#if !compact}<span>{t('Fetch')}</span>{/if}</button>{/if}
  <button onclick={light} class:on={p.dark} title={p.dark ? t('Turn the light on: it wakes up') : t('Turn the light off: it sleeps and gets its energy back')} aria-label={p.dark ? t('Wake') : t('Sleep')}>{p.dark ? '💡' : '🌙'}{#if !compact}<span>{p.dark ? t('Wake') : t('Sleep')}</span>{/if}</button>
{/snippet}

<!-- What it brought back: a paper in its mouth, in its room; a chip elsewhere. -->
{#snippet carriedPaper(inRoom: boolean)}
  {#if carried}<button class="carried" class:in-room={inRoom} onclick={openCarried} title={t('{text} · open it', { text: carried.text })}>{#if !inRoom}🦴{' '}{/if}{carried.tag}</button>{/if}
{/snippet}

<!-- Short: one row, the creature small, what it says, its needs and its cares. -->
{#snippet shortRow(mini: boolean)}
  <div class="short" class:dark={p.dark}>
    <div class="mini">{#if mini}{@render creature()}{:else}<Creature {stage} {form} face={mood.face} />{/if}</div>
    <div class="mid">
      <p class="line" title={mood.text}>{mood.text}</p>
      <div class="bars">{@render needs()}</div>
    </div>
    <div class="acts icons">{@render cares(true)}</div>
  </div>
{/snippet}

{#snippet photo(shot: Shot)}
  <figure class="polaroid">
    <div class="ph"><Creature stage={shot.stage} form={shot.form} face={shot.icon ? 'party' : 'happy'} />{#if shot.icon}<span class="badge">{shot.icon}</span>{/if}</div>
    <figcaption>{shot.caption}<small>{shortDate(shot.at)}</small></figcaption>
  </figure>
{/snippet}

{#if place === 'status'}
  <!-- In the status bar: small, hopping when it has something to say; a click, its short row. -->
  <button bind:this={chip} class="sbi petchip" class:need={mood.face !== 'happy' && mood.face !== 'sleep'} onclick={(e) => { e.stopPropagation(); popOpen = !popOpen }} title={t('{name}: {text}', { name: p.name, text: mood.text })} aria-expanded={popOpen}>
    <span class="tiny">{@render creature()}</span>{p.name}
  </button>
  {#if popOpen}
    <div class="petpop" {@attach anchored(() => chip, { align: 'end', gap: 6 })} {@attach dismissable(() => (popOpen = false), { trigger: () => chip })}>
      <p class="pophead"><b>{p.name}</b> {t('Lv {n}', { n: level })} · {form ? FORMS[form].label : t(stage)}<span class="grow"></span>{@render carriedPaper(false)}</p>
      {@render shortRow(false)}
    </div>
  {/if}
{:else}
<section class="panel" style="--src: var(--src-claude)" class:collapsed={isCollapsed('pet')}>
  <h2>
    <span class="title">
      {#if naming}
        <!-- svelte-ignore a11y_autofocus -->
        <input class="name" value={p.name} autofocus maxlength="20" onblur={(e) => rename(e.currentTarget.value)} onkeydown={(e) => { if (e.key === 'Enter') rename(e.currentTarget.value); if (e.key === 'Escape') naming = false }} aria-label={t('Its name')} />
      {:else}
        <button class="named" onclick={() => (naming = true)} title={t('Rename it')}>{p.name}</button>
      {/if}
    </span>
    <small title="{p.xp} XP · {tn(days, '{n} day old', '{n} days old')}{form ? ` · ${FORMS[form].label}: ${FORMS[form].why}` : ''}">{t('Lv {n}', { n: level })} · {form ? FORMS[form].label : t(stage)}</small>
    {#if !isShort('pet')}
      {#if cfg.trophies}<button class="tab" class:on={view === 'trophies'} onclick={() => (view = view === 'trophies' ? 'pet' : 'trophies')} title={t('Its trophies')}>🏆 {won.length}/{TROPHIES.length}</button>{/if}
      {#if cfg.diary && diary.length}<button class="tab" class:on={view === 'diary'} onclick={() => ((view = view === 'diary' ? 'pet' : 'diary'), (page = -1))} title={t('Its diary: a page a week')}>📔</button>{/if}
      {#if album.length}<button class="tab" class:on={view === 'album'} onclick={() => (view = view === 'album' ? 'pet' : 'album')} title={t('Its album: a photo at each milestone')}>📷</button>{/if}
    {/if}
    <BlockControls id="pet" />
  </h2>
  {#if isShort('pet')}
    {@render shortRow(true)}
  {:else if view === 'trophies'}
    <div class="list">
      {#each TROPHIES as tr (tr.id)}
        {@const [n, of] = tr.progress(p.growth, level, days)}
        {@const at = p.growth.trophies[tr.id]}
        <div class="tr" class:lock={!at}>
          <span class="ic" aria-hidden="true">{tr.icon}</span>
          <b>{tr.label}</b>
          {#if at}<span class="when">{shortDate(at)}</span>{:else}<span class="when">{Math.min(n, of)} / {of}</span>{/if}
          <small>{tr.what}</small>
          {#if !at}<span class="prog"><i style:width="{Math.min(100, (n / of) * 100)}%"></i></span>{/if}
        </div>
      {/each}
    </div>
  {:else if view === 'diary' && shownPage}
    <div class="diary">
      <div class="page">
        <p class="dear">{t('Dear diary,')}</p>
        {#each shownPage.lines as line, i (i)}<p>{line}</p>{/each}
        <p class="sig">{shownPage.sig}</p>
      </div>
      <div class="dfoot">
        <span>{t('Week of {date}', { date: new Date(`${shownPage.week}T12:00`).toLocaleDateString(locale(), { day: 'numeric', month: 'short' }) })} · {shownPage.stats}</span>
        <span class="grow"></span>
        <button disabled={diary.indexOf(shownPage) <= 0} onclick={() => (page = diary.indexOf(shownPage) - 1)} aria-label={t('Previous week')}>‹</button>
        <button disabled={diary.indexOf(shownPage) >= diary.length - 1} onclick={() => (page = diary.indexOf(shownPage) + 1)} aria-label={t('Next week')}>›</button>
      </div>
    </div>
  {:else if view === 'album' && album.length}
    <div class="album">
      {#each [...album].reverse() as shot, i (i)}{@render photo(shot)}{/each}
    </div>
  {:else}
    <div class="body" class:dark={p.dark}>
      <p class="bubble">{mood.text}</p>
      {#if cfg.trophies || cfg.friends || cfg.seasons}
        <!-- Its room: each trophy won puts an object in it (the others are faint outlines), its friends sit in it, its window shows the world. -->
        {@const night = cfg.seasons && part === 'night'}
        <div class="room">
          <svg class="scene" viewBox="0 0 300 170" preserveAspectRatio="none" aria-hidden="true">
            <rect class="wall" x="0" y="0" width="300" height="122" /><rect class="floor" x="0" y="122" width="300" height="48" /><rect class="board" x="0" y="119" width="300" height="5" />
            {#if cfg.seasons}
              <rect x="18" y="16" width="62" height="50" style:fill={SKY[part]} />
              {#if night}
                {#each STARS as [x, y] (`${x},${y}`)}<circle class="star" cx={x} cy={y} r="1" />{/each}
                <path class="moonlight" d="M66 22 a 7 7 0 1 0 7 9 a 5.5 5.5 0 1 1 -7 -9 Z" />
              {:else if has('green')}<circle class="sun" cx={part === 'day' ? 58 : 36} cy={part === 'day' ? 30 : 56} r="9" />
              {:else}<path class="cloud" d="M30 36 a 6 6 0 0 1 11 -3 a 7 7 0 0 1 13 2 a 5 5 0 0 1 0 10 h -24 a 5 5 0 0 1 0 -9 Z" />{/if}
              {#if seasonNow === 'winter'}<rect class="snow" x="18" y="60" width="62" height="6" />{#each [[28, 24], [46, 34], [62, 22], [72, 44], [36, 48]] as [x, y] (`${x},${y}`)}<circle class="flake" cx={x} cy={y} r="1.4" />{/each}{/if}
            {:else}
              <rect class="win" x="18" y="16" width="62" height="50" />
              {#if has('green')}<circle class="sun" cx="58" cy="33" r="10" />{/if}
            {/if}
            <rect class="frame" x="18" y="16" width="62" height="50" /><path class="frame" d="M49 16 L49 66 M18 41 L80 41" />
            <rect class="shelf" x="196" y="48" width="92" height="5" rx="2" />
            <ellipse class="rug" cx="150" cy="152" rx="70" ry="12" />
            {#if today.includes('birthday')}<path class="garland" d="M90 4 Q 110 18 130 4 Q 150 18 170 4 Q 190 18 210 4" />{/if}
          </svg>
          {#if cfg.seasons && seasonNow === 'autumn'}<span class="obj deco" style:left="2%" style:top="3%">🍂</span>{/if}
          {#if cfg.seasons && seasonNow === 'spring'}<span class="obj deco" style:left="14%" style:top="32%">🌸</span>{/if}
          {#if cfg.trophies}
            {#each [['sprint', 202, 26], ['bookworm', 226, 26], ['tiny', 250, 26], ['owl', 266, 70], ['bird', 26, 54], ['merges', 8, 104], ['month', 262, 132], ['legend', 120, 14]] as const as [id, x, y] (id)}
              {@const tr = TROPHIES.find((x) => x.id === id)!}
              <span class="obj" class:faint={!has(id)} style:left="{(x / 300) * 100}%" style:top="{(y / 170) * 100}%" title={has(id) ? t('{label}: {what}', { label: tr.label, what: tr.what }) : t('Not yet: {what}', { what: tr.what })}>{id === 'legend' ? '⭐' : tr.icon}</span>
            {/each}
          {/if}
          {#if today.includes('no-deploy')}<span class="sign" title={t('Friday afternoon: no deploy')}>{t('NO DEPLOY')}</span>{/if}
          {#if today.includes('sprint-end')}<span class="obj" style:left="71%" style:top="44%" title={t('Last day of the sprint')}>🏁</span>{/if}
          {#if feast}<span class="obj" style:left="65%" style:top="81%">{FEAST[feast]}</span>{/if}
          {#if album.length}
            <!-- Its last photo, on the wall. -->
            {@const last = album[album.length - 1]}
            <button class="wallphoto" onclick={() => (view = 'album')} title={t('{caption}, {date} · its album', { caption: last.caption, date: shortDate(last.at) })}><Creature stage={last.stage} form={last.form} face="happy" /></button>
          {/if}
          {#each friends.slice(0, FRIEND_SLOTS.length) as a, i (a.id)}
            <button class="friend" class:waits={waitsForYou(a.status)} style:left="{((FRIEND_SLOTS[i] - 17) / 300) * 100}%" onclick={() => openAgents(a.id)} title={friendTip(a)} aria-label={friendTip(a)}>
              <Creature stage="young" face={friendFace(a)} color={cliOf(a.provider).color} />
              {#if waitsForYou(a.status)}<span class="flag" aria-hidden="true">!</span>{/if}
            </button>
          {/each}
          {#if friends.length > FRIEND_SLOTS.length}<button class="more" onclick={() => openAgents()} title={t('Its other friends: the Agents view')}>+{friends.length - FRIEND_SLOTS.length}</button>{/if}
          <div class="stage in-room">{@render creature()}</div>
          {@render carriedPaper(true)}
        </div>
      {:else}
        <div class="stage">{@render creature()}</div>
        {@render carriedPaper(false)}
      {/if}
      <div class="bars">{@render needs()}</div>
      <div class="acts">{@render cares(false)}</div>
    </div>
  {/if}
</section>
{/if}

<style>
  .named { padding: 0; border: none; background: none; color: inherit; font: inherit; cursor: text; }
  .named:hover { text-decoration: underline dotted; }
  .name { width: 120px; padding: 0 4px; border: 1px solid var(--src-claude); border-radius: var(--r4); background: var(--bg); color: var(--text); font: inherit; outline: none; }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 8px 12px 12px; overflow: auto; transition: background 0.4s; }
  .body.dark { background: color-mix(in srgb, #000 35%, var(--surface)); }
  .bubble { position: relative; margin: 0; max-width: 100%; padding: 5px 10px; border-radius: var(--r12); background: var(--surface-muted); font-size: var(--fs-body); text-align: center; }
  .bubble::after { content: ''; position: absolute; left: 50%; bottom: -5px; margin-left: -5px; border: 5px solid transparent; border-top-color: var(--surface-muted); border-bottom: 0; }
  .stage { width: 160px; height: 160px; flex-shrink: 0; }
  .short { display: flex; align-items: center; gap: 10px; padding: 6px 10px; transition: background 0.4s; }
  .short.dark { background: color-mix(in srgb, #000 35%, var(--surface)); }
  .mini { width: 46px; height: 46px; flex-shrink: 0; }
  .mid { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .line { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-body); }
  .short .bars { max-width: none; gap: 8px; }
  .acts.icons { flex-shrink: 0; gap: 4px; }
  .acts.icons button { padding: 2px 6px; }

  .bars { display: flex; gap: 10px; width: 100%; max-width: 280px; }
  .bar { flex: 1; display: flex; align-items: center; gap: 4px; font-size: 11px; }
  .track { flex: 1; height: 6px; border-radius: var(--r3); background: var(--surface-muted); overflow: hidden; }
  .track i { display: block; height: 100%; border-radius: var(--r3); transition: width 0.4s; }
  .track i.ok { background: var(--ok); }
  .track i.warn { background: var(--warn); }
  .track i.ko { background: var(--ko); }
  .acts { display: flex; gap: 6px; }
  .acts button { display: inline-flex; align-items: center; gap: 5px; padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .acts button:hover { border-color: var(--src-claude); }
  .acts button.on { border-color: var(--src-alert); }
  .tab { padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); font-weight: 400; cursor: pointer; }
  .tab.on { border-color: var(--src-claude); color: var(--text); }

  /* Its room. */
  .room { position: relative; width: 100%; max-width: 300px; height: 170px; flex-shrink: 0; border-radius: var(--r10); overflow: hidden; }
  .scene { position: absolute; inset: 0; width: 100%; height: 100%; }
  .wall { fill: color-mix(in srgb, var(--src-claude) 10%, var(--surface-muted)); }
  .floor { fill: color-mix(in srgb, #7a5a3a 30%, var(--surface-muted)); }
  .board { fill: color-mix(in srgb, #7a5a3a 50%, var(--surface-muted)); }
  .win { fill: color-mix(in srgb, var(--src-calendar) 30%, var(--surface)); }
  .sun { fill: var(--src-alert); }
  .frame { fill: none; stroke: color-mix(in srgb, #7a5a3a 60%, var(--surface-muted)); stroke-width: 3; }
  .shelf { fill: color-mix(in srgb, #7a5a3a 60%, var(--surface-muted)); }
  .rug { fill: color-mix(in srgb, var(--src-claude) 25%, var(--surface-muted)); }
  .obj { position: absolute; font-size: 19px; line-height: 1; }
  .obj.faint { opacity: 0.16; filter: grayscale(1); }
  .star, .flake { fill: #fff; }
  .moonlight { fill: #f3ecd9; }
  .cloud { fill: #fff; opacity: 0.85; }
  .snow { fill: #eef3f8; }
  .garland { fill: none; stroke: #ff7aa2; stroke-width: 2.5; stroke-dasharray: 4 3; }
  .obj.deco { font-size: 14px; }
  .sign { position: absolute; left: 47%; top: 13%; padding: 1px 4px; border: 1.5px solid #3b3326; border-radius: var(--r3); background: #f3ecd9; color: #b4412f; font: 700 8px/1.2 var(--sans); letter-spacing: 0.3px; transform: rotate(-4deg); }
  .wallphoto { position: absolute; left: 29%; top: 30%; width: 20px; height: 24px; padding: 2px 2px 5px; border: none; border-radius: 1px; background: #f3ecd9; box-shadow: 0 1px 2px rgba(0, 0, 0, 0.4); transform: rotate(-4deg); cursor: pointer; }
  .wallphoto :global(svg) { display: block; background: #3a3129; }
  .friend { position: absolute; bottom: 4px; width: 34px; height: 34px; padding: 0; border: none; background: none; cursor: pointer; }
  .friend.waits { animation: hop 0.7s ease-in-out infinite; }
  .friend .flag { position: absolute; top: -8px; right: 0; color: var(--src-alert); font: 800 13px var(--sans); }
  .more { position: absolute; left: 90%; bottom: 8px; padding: 0 5px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .carried { padding: 1px 6px; border: none; border-radius: var(--r3); background: #f3ecd9; color: #3b3326; font: 600 10px var(--mono); box-shadow: 0 1px 0 rgba(0, 0, 0, 0.3); cursor: pointer; }
  .carried.in-room { position: absolute; left: calc(50% + 8px); bottom: 46px; transform: rotate(-10deg); }
  .stage.in-room { position: absolute; left: 50%; bottom: 4px; width: 130px; height: 130px; margin-left: -65px; }

  /* Its album. */
  .album { flex: 1; min-height: 0; overflow: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(88px, 1fr)); align-content: start; align-items: start; gap: 12px; padding: 12px; }
  .polaroid { margin: 0; padding: 5px 5px 6px; background: #f3ecd9; border-radius: 2px; box-shadow: 0 2px 5px rgba(0, 0, 0, 0.35); color: #3b3326; text-align: center; }
  .polaroid:nth-child(odd) { transform: rotate(-2deg); }
  .polaroid:nth-child(even) { transform: rotate(1.5deg); }
  .ph { position: relative; height: 70px; padding: 4px; background: #3a3129; }
  .badge { position: absolute; right: 3px; bottom: 2px; font-size: 14px; }
  .polaroid figcaption { margin-top: 4px; font: 12px/1.2 'Bradley Hand', 'Segoe Print', 'Comic Sans MS', cursive; }
  .polaroid small { display: block; opacity: 0.6; font-size: 10px; }

  /* In the status bar. */
  .petchip { color: var(--text); }
  .tiny { display: inline-flex; width: 18px; height: 18px; }
  .petchip.need .tiny { animation: hop 0.7s ease-in-out infinite; }
  .petpop { position: fixed; z-index: 45; width: 360px; overflow: hidden; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); font-size: var(--fs-body); }
  .pophead { display: flex; align-items: center; gap: 6px; margin: 0; padding: 6px 10px 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .pophead b { color: var(--text); }
  .petpop .line { white-space: normal; }
  .pophead .grow { flex: 1; }
  @keyframes hop { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-4px); } }
  :global(html[data-reduce-motion]) .friend.waits, :global(html[data-reduce-motion]) .petchip.need .tiny { animation: none; }

  /* Its trophies. */
  .list { flex: 1; min-height: 0; overflow: auto; }
  .tr { display: grid; grid-template-columns: 24px 1fr auto; gap: 1px 8px; align-items: center; padding: 6px 12px; border-bottom: 1px solid var(--border); }
  .tr .ic { grid-row: 1 / 3; font-size: 17px; text-align: center; }
  .tr b { font-size: var(--fs-body); font-weight: 600; }
  .tr small { grid-column: 2 / 4; color: var(--text-muted); font-size: var(--fs-meta); }
  .tr .when { font-size: var(--fs-tag); color: var(--ok); }
  .tr.lock .ic { filter: grayscale(1); opacity: 0.4; }
  .tr.lock b, .tr.lock .when { color: var(--text-muted); }
  .prog { grid-column: 2 / 4; height: 4px; border-radius: var(--r2); background: var(--surface-muted); overflow: hidden; }
  .prog i { display: block; height: 100%; background: var(--src-claude); }

  /* Its diary. */
  .diary { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .page { flex: 1; overflow: auto; padding: 12px 14px; background: #f3ecd9; color: #3b3326; font: 13px/1.55 'Bradley Hand', 'Segoe Print', 'Comic Sans MS', cursive; }
  .page p { margin: 0 0 6px; }
  .page .dear { font-weight: 700; }
  .page .sig { margin-top: 8px; text-align: right; }
  .dfoot { display: flex; align-items: center; gap: 6px; padding: 5px 10px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-meta); }
  .dfoot .grow { flex: 1; }
  .dfoot button { padding: 0 7px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; cursor: pointer; }
  .dfoot button:disabled { opacity: 0.4; cursor: default; }
</style>
