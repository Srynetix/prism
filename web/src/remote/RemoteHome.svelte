<script lang="ts">
  import { clock } from '../clock.svelte'
  import { t, tn } from '../i18n.svelte'
  import Icon from '../components/Icon.svelte'
  import type { IconName } from '../icons'
  import { openAgents } from '../agents.svelte'
  import { isArchived } from '../agent-archive.svelte'
  import { cliOf } from '../agent-cli'
  import { meOn } from '../forge'
  import { isDone, isWorking, mrNeedsMe, openMrs, type Alert, type Severity } from '../rules'
  import { conventions } from '../conventions'
  import { ago } from '../time'
  import { locale } from '../locale'
  import type { Agent, CalendarEvent, Snapshot } from '../types'
  import QuickAnswers from '../components/QuickAnswers.svelte'
  import RoutineReview from '../components/RoutineReview.svelte'
  import { initial, profileColor } from '../profiles.svelte'
  import { phoneProfiles, switchProfile, type PhoneProfile } from './phone-profiles.svelte'
  import { socketState } from './socket.svelte'

  /**
   * The remote's home: what waits for you, at a glance. First the agents waiting for an answer (a
   * tap opens their terminal), then three tiles (the MRs that need you, your tickets in progress,
   * the next meeting), then the alerts by severity, the most serious few in full. Each part opens
   * its own view (`go`), where everything is.
   *
   * All (`all`, several profiles, `phone-profiles`): the agents waiting in every profile, those
   * waiting longest first, each with its profile's badge and its quick answers (a tap opens it in
   * its profile), then a card per profile; the tiles and alerts are the hub's own.
   */
  let { snap, alerts, go, all = false }: { snap: Snapshot; alerts: Alert[]; go: (view: string) => void; all?: boolean } = $props()

  // Archived: out of the counts, until they wait for you (then they are no longer).
  const running = $derived((snap.agents ?? []).filter((a) => a.state === 'running' && !isArchived(a)))
  const waiting = $derived(running.filter((a) => a.status === 'needs_you' || a.status === 'waiting_tool' || a.status === 'review_ready'))
  const working = $derived(running.filter((a) => isWorking(a.status)).length)
  const ASKS: Partial<Record<Agent['status'], string>> = { needs_you: t('asks you'), waiting_tool: t('asks permission'), review_ready: t('plan ready') }

  const waitingIn = (s: Snapshot | null) => (s?.agents ?? []).filter((a) => a.state === 'running' && !isArchived(a) && (a.status === 'needs_you' || a.status === 'waiting_tool' || a.status === 'review_ready'))
  const hubProfile = $derived(phoneProfiles.list.find((p) => p.hub) ?? null)
  type Waiting = { a: Agent; p: PhoneProfile | null; acts: boolean }
  /** What waits for you: here, or in every profile in All, longest waiting first. */
  const waits = $derived.by<Waiting[]>(() => {
    const own = waiting.map((a) => ({ a, p: all ? hubProfile : null, acts: socketState.hello?.classes.includes('act') ?? false }))
    if (!all) return own
    const others = phoneProfiles.list.flatMap((p) => {
      const side = phoneProfiles.sides[p.id]
      return side ? waitingIn(side.snap).map((a) => ({ a, p, acts: side.state.hello?.classes.includes('act') ?? false })) : []
    })
    return [...own, ...others].sort((x, y) => (x.a.last_activity ?? x.a.created_at).localeCompare(y.a.last_activity ?? y.a.created_at))
  })
  const openWaiting = (w: Waiting) => (w.p && !w.p.hub ? switchProfile(w.p.id, { view: 'agents', agent: w.a.id }) : openAgents(w.a.id))
  /** A profile's card in All: what it is doing, its MRs that need you when its snapshot is here. */
  const sub = (p: PhoneProfile) => {
    if (!p.reachable) return p.open ? t('Open on the computer; its remote is not relayed here') : t('Closed on the computer')
    const s = p.hub ? snap : (phoneProfiles.sides[p.id]?.snap ?? null)
    const agents = s ? s.agents.filter((a) => a.state === 'running').length : p.agents
    const n = s ? waitingIn(s).length : p.waiting
    const mrs = s ? openMrs(s).filter((m) => mrNeedsMe(m, meOn(s, m), s.merge_queue_label)).length : 0
    return [tn(agents, '{n} agent', '{n} agents'), n && tn(n, '{n} waiting', '{n} waiting'), mrs && tn(mrs, '{n} MR to review', '{n} MRs to review')].filter(Boolean).join(' · ')
  }

  const mrs = $derived(openMrs(snap).filter((m) => mrNeedsMe(m, meOn(snap, m), snap.merge_queue_label)).length)
  const me = $derived(snap.tracker_me)
  const tickets = $derived(
    (snap.sprint?.tickets ?? []).filter((t) => !isDone(t) && !!me && t.assignees.some((a) => a.id === me.id) && conventions().in_progress_statuses.includes(t.status)).length,
  )

  const now = $derived(clock(30_000).now)
  const ms = (iso: string) => new Date(iso).getTime()
  const busy = (e: CalendarEvent) => !e.canceled && !e.free && e.my_status !== 'declined'
  const timed = $derived((snap.agenda?.events ?? []).filter((e) => !e.all_day && busy(e)))
  /** The meeting under way, else the next one today. */
  const meeting = $derived(timed.find((e) => ms(e.start) <= now && now < ms(e.end)) ?? timed.find((e) => ms(e.start) > now) ?? null)
  const hhmm = (iso: string) => new Date(iso).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' })

  const count = (s: Severity) => alerts.filter((a) => a.severity === s).length
  const RANK: Record<Severity, number> = { ko: 0, warn: 1, info: 2 }
  const top = $derived([...alerts].sort((a, b) => RANK[a.severity] - RANK[b.severity]).slice(0, 3))

  const hour = new Date().getHours()
  const hello = hour < 5 ? t('Good night') : hour < 12 ? t('Good morning') : hour < 18 ? t('Good afternoon') : t('Good evening')
  const calm = $derived(!waits.length && !mrs && !count('ko'))

  type Tile = { view: string; icon: IconName; value: string; label: string; hot: boolean }
  const tiles = $derived<Tile[]>([
    { view: 'mrs', icon: 'mr', value: String(mrs), label: tn(mrs, 'MR needs you', 'MRs need you'), hot: mrs > 0 },
    { view: 'tickets', icon: 'kanban', value: String(tickets), label: t('in progress'), hot: false },
    meeting
      ? { view: 'today', icon: 'calendar', value: ms(meeting.start) <= now ? t('now') : hhmm(meeting.start), label: meeting.title, hot: ms(meeting.start) - now < 15 * 60_000 }
      : { view: 'today', icon: 'calendar', value: '—', label: t('no meeting left'), hot: false },
  ])
</script>

<section class="hello">
  <h2>{hello}{me?.name ? `, ${me.name.split(' ')[0]}` : ''}</h2>
  <p>{calm ? t('Nothing waits for you.') : [waits.length && tn(waits.length, '{n} agent waiting', '{n} agents waiting'), mrs && tn(mrs, '{n} MR', '{n} MRs'), count('ko') && tn(count('ko'), '{n} broken', '{n} broken')].filter(Boolean).join(' · ')}{working ? ` · ${tn(working, '{n} agent working', '{n} agents working')}` : ''}</p>
</section>

{#if waits.length}
  <p class="sect">{t('Agents waiting')} · {waits.length}</p>
  <ul class="card">
    {#each waits as w (w.a.id)}
      {@const a = w.a}
      <li>
        <button onclick={() => openWaiting(w)}>
          {#if w.p}<span class="pf" style:--c={profileColor(w.p.color)} title={w.p.name}>{initial(w.p.name)}</span>{:else}<span class="ico asks"><Icon name={cliOf(a.provider).icon} /></span>{/if}
          <span class="txt"><b>{a.title}</b><small>{ASKS[a.status]}{a.last_activity ? ` · ${ago(a.last_activity)}` : ''}</small></span>
          <Icon name="next" />
        </button>
        {#if all && w.acts && (a.status === 'needs_you' || a.status === 'review_ready')}<div class="chips"><QuickAnswers agent={a} /></div>{/if}
      </li>
    {/each}
  </ul>
{/if}

<!-- The routines' runs waiting for you ("To review"): the same pile as the computer's, Rerun with the passkey. -->
<RoutineReview next={false} />

{#if all}
  <p class="sect">{t('Profiles')}</p>
  <ul class="card">
    {#each phoneProfiles.list as p (p.id)}
      <li>
        <button disabled={!p.reachable} onclick={() => switchProfile(p.id)}>
          <span class="pf big" style:--c={profileColor(p.color)}>{initial(p.name)}</span>
          <span class="txt"><b>{p.name}</b><small>{sub(p)}</small></span>
          {#if p.reachable}<Icon name="next" />{/if}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<div class="tiles">
  {#each tiles as t (t.view)}
    <button class="tile" class:hot={t.hot} onclick={() => go(t.view)}>
      <Icon name={t.icon} />
      <b>{t.value}</b>
      <small>{t.label}</small>
    </button>
  {/each}
</div>

<p class="sect">{t('Alerts')}{#if alerts.length}{' '}· <span class="ko">{tn(count('ko'), '{n} broken', '{n} broken')}</span> · <span class="warn">{t('{n} to watch', { n: count('warn') })}</span>{/if}</p>
<ul class="card">
  {#each top as a, i (i)}
    <li>
      <button onclick={() => go('alerts')}>
        <span class="bar {a.severity}"></span>
        <span class="txt"><small>{a.kind}</small><span class="clip">{a.text}</span></span>
      </button>
    </li>
  {:else}
    <li><p class="none"><Icon name="check" /> {t('All clear')}</p></li>
  {/each}
  {#if alerts.length > top.length}
    <li><button class="all" onclick={() => go('alerts')}><span class="txt">{t('See all {n}', { n: alerts.length })}</span><Icon name="next" /></button></li>
  {/if}
</ul>

<style>
  .hello { padding: 8px 4px 2px; }
  .hello h2 { margin: 0; font-size: 22px; font-weight: 650; }
  .hello p { margin: 4px 0 0; color: var(--text-muted); font-size: 14px; }
  .sect { margin: 6px 4px -4px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .sect .ko { color: var(--ko); }
  .sect .warn { color: var(--warn); }
  ul.card { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; background: var(--surface); }
  ul.card li + li { border-top: 1px solid var(--border); }
  ul.card button { width: 100%; display: flex; align-items: center; gap: 12px; padding: 13px 14px; background: none; border: none; color: var(--text); font-size: 15px; text-align: left; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .txt b { font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt small { color: var(--text-muted); font-size: 12px; }
  .clip { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; font-size: 14px; }
  .ico { flex: none; display: grid; place-items: center; width: 32px; height: 32px; border-radius: 10px; background: var(--surface-muted); }
  .ico.asks { background: color-mix(in srgb, var(--accent) 18%, transparent); color: var(--accent); }
  /* A profile's badge (All): its initial on its color. */
  .pf { flex: none; display: grid; place-items: center; width: 24px; height: 24px; border-radius: 7px; border: 1px solid color-mix(in srgb, var(--c) 55%, transparent); background: color-mix(in srgb, var(--c) 20%, transparent); color: var(--c); font-size: 12px; font-weight: 700; }
  .pf.big { width: 32px; height: 32px; border-radius: 10px; font-size: 15px; }
  ul.card button:disabled { opacity: 0.65; }
  .chips { display: flex; gap: 6px; padding: 0 14px 12px 50px; overflow-x: auto; scrollbar-width: none; }
  .chips :global(.qa) { flex-wrap: nowrap; }
  .chips :global(.qa button) { flex: none; padding: 6px 12px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
  .bar { flex: none; align-self: stretch; width: 3px; border-radius: 2px; background: var(--text-muted); }
  .bar.ko { background: var(--ko); }
  .bar.warn { background: var(--warn); }
  .all { color: var(--accent) !important; }
  .none { margin: 0; padding: 14px; display: flex; align-items: center; gap: 8px; color: var(--ok); font-size: 14px; }
  .tiles { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; }
  .tile { display: flex; flex-direction: column; align-items: flex-start; gap: 4px; padding: 12px; min-width: 0; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); color: var(--text-muted); text-align: left; }
  .tile b { font-size: 22px; font-weight: 650; color: var(--text); font-variant-numeric: tabular-nums; }
  .tile small { width: 100%; font-size: 12px; line-height: 1.3; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tile.hot { border-color: color-mix(in srgb, var(--accent) 55%, var(--border)); color: var(--accent); }
</style>
