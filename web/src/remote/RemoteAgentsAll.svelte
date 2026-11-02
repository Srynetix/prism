<script lang="ts">
  import QuickAnswers from '../components/QuickAnswers.svelte'
  import { t, tn, tc } from '../i18n.svelte'
  import { openAgents } from '../agents.svelte'
  import { cliOf } from '../agent-cli'
  import { isArchived } from '../agent-archive.svelte'
  import { waitsForYou } from '../rules'
  import { ago } from '../time'
  import { initial, profileColor } from '../profiles.svelte'
  import { phoneProfiles, switchProfile, type PhoneProfile } from './phone-profiles.svelte'
  import { socketState } from './socket.svelte'
  import type { Agent, Snapshot } from '../types'

  /**
   * Agents in All (`phone-profiles`): every profile's, grouped by profile, what waits for you first
   * in each, with its quick answers. A tap opens its terminal, in its profile (the page goes there
   * for another's). No new agent here: one starts in a profile, chosen in the header.
   */
  let { snap }: { snap: Snapshot } = $props()

  type Group = { p: PhoneProfile; agents: Agent[]; stopped: number; acts: boolean }
  const RANK: Partial<Record<Agent['status'], number>> = { waiting_tool: 0, needs_you: 0, review_ready: 0, running: 1, background: 1 }
  const order = (a: Agent, b: Agent) => (RANK[a.status] ?? 2) - (RANK[b.status] ?? 2) || (b.last_activity ?? b.created_at).localeCompare(a.last_activity ?? a.created_at)
  const group = (p: PhoneProfile, s: Snapshot | null, acts: boolean): Group => {
    const agents = (s?.agents ?? []).filter((a) => !isArchived(a) || waitsForYou(a.status))
    return { p, agents: agents.filter((a) => a.state === 'running').sort(order), stopped: agents.filter((a) => a.state !== 'running').length, acts }
  }
  const groups = $derived.by(() => {
    const out: Group[] = []
    for (const p of phoneProfiles.list) {
      if (p.hub) out.push(group(p, snap, socketState.hello?.classes.includes('act') ?? false))
      else if (phoneProfiles.sides[p.id]) {
        const side = phoneProfiles.sides[p.id]
        out.push(group(p, side.snap, side.state.hello?.classes.includes('act') ?? false))
      }
    }
    return out
  })

  const STATE: Partial<Record<Agent['status'], [string, string]>> = { needs_you: [t('Asks'), 'asks'], waiting_tool: [t('Permission'), 'tool'], review_ready: [t('Plan ready'), 'asks'], running: [t('Working'), 'run'], background: [t('In the background'), 'run'] }
  const label = (a: Agent) => STATE[a.status] ?? [tc('infra', 'Done'), '']
  const open = (g: Group, a: Agent) => (g.p.hub ? openAgents(a.id) : switchProfile(g.p.id, { view: 'agents', agent: a.id }))
</script>

<div class="all">
  {#each groups as g (g.p.id)}
    <p class="grp"><span class="pf" style:--c={profileColor(g.p.color)}>{initial(g.p.name)}</span>{g.p.name} · {g.agents.length}</p>
    {#each g.agents as a (a.id)}
      {@const [state, tone] = label(a)}
      <div class="ag">
        <button class="row" onclick={() => open(g, a)}>
          <span class="dot {tone}"></span>
          <span class="txt"><b>{a.title}</b><small><span class="st {tone}">{state}</span> · {cliOf(a.provider).name.toLowerCase()} · {ago(a.last_activity ?? a.created_at)}</small></span>
        </button>
        {#if a.needs_action && waitsForYou(a.status)}<p class="q" class:tool={a.status === 'waiting_tool'}>{a.needs_action}</p>{/if}
        {#if g.acts && (a.status === 'needs_you' || a.status === 'review_ready')}<div class="chips"><QuickAnswers agent={a} /></div>{/if}
      </div>
    {:else}
      <p class="none">{t('No agent running.')}</p>
    {/each}
    {#if g.stopped}<button class="more" onclick={() => switchProfile(g.p.id, { view: 'agents' })}>{tn(g.stopped, '{n} stopped agent', '{n} stopped agents')}<span>›</span></button>{/if}
  {/each}
  <p class="note">{t('A new agent starts in a profile: choose it in the header.')}</p>
</div>

<style>
  .all { width: calc(100% + 20px); max-width: none; margin: 0 -10px; padding-bottom: 8px; }
  .grp { display: flex; align-items: center; gap: 7px; margin: 0; padding: 14px 14px 6px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .pf { display: inline-flex; align-items: center; justify-content: center; width: 18px; height: 18px; border-radius: 5px; border: 1px solid color-mix(in srgb, var(--c) 55%, transparent); background: color-mix(in srgb, var(--c) 20%, transparent); color: var(--c); font-size: 10px; font-weight: 700; letter-spacing: 0; }
  .ag { padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .row { width: 100%; display: flex; align-items: center; gap: 10px; padding: 0; border: none; background: none; color: var(--text); font: inherit; text-align: left; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .txt b { font-size: 15px; font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt small { color: var(--text-muted); font-size: 12px; }
  .dot { flex: none; width: 9px; height: 9px; border-radius: 50%; background: var(--text-muted); }
  .dot.asks { background: var(--warn); } .dot.tool { background: var(--ko); } .dot.run { background: var(--info); }
  .st { font-weight: 600; }
  .st.asks { color: var(--warn); } .st.tool { color: var(--ko); } .st.run { color: var(--info); }
  .q { margin: 8px 0 0; padding: 8px 10px; border-left: 3px solid var(--warn); border-radius: 4px; background: var(--bg); font-size: 13px; line-height: 1.4; white-space: pre-wrap; overflow-wrap: anywhere; }
  .q.tool { border-color: var(--ko); font-family: var(--mono); font-size: 12px; }
  .chips { display: flex; gap: 6px; margin-top: 8px; overflow-x: auto; scrollbar-width: none; }
  .chips :global(.qa) { flex-wrap: nowrap; }
  .chips :global(.qa button) { flex: none; padding: 6px 12px; border: 1px solid var(--border); border-radius: 16px; background: var(--surface); color: var(--text); font: inherit; font-size: 13px; }
  .none { margin: 0; padding: 10px 14px; color: var(--text-muted); font-size: 13px; }
  .more { display: flex; justify-content: space-between; width: calc(100% - 28px); margin: 8px 14px 0; padding: 9px 12px; border: 0; border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: 13px; }
  .note { margin: 18px 14px 0; color: var(--text-muted); font-size: 12px; text-align: center; }
</style>
