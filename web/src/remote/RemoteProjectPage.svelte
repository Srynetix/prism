<script lang="ts">
  import { t, tn } from '../i18n.svelte'
  import { openAgents, modelName } from '../agents.svelte'
  import { cliOf } from '../agent-cli'
  import { sessionLabel } from '../rules'
  import { mrKey } from '../mr-ref'
  import { ago } from '../time'
  import { tildePath } from '../platform'
  import { isHidden } from '../worktree-archive.svelte'
  import { nextOf } from '../project-actions'
  import { byActivity, projectSummaries, ticketsFor, waitingAgent, type Stage, type Workstream } from '../workstreams'
  import NewWorkstream from '../components/NewWorkstream.svelte'
  import Icon from '../components/Icon.svelte'
  import type { Snapshot } from '../types'

  /**
   * A project on the phone: its workstreams as cards, by stage (in progress, in review, done · to
   * clean up, to pick up), each with its agents (a tap opens one), its ticket and its MR; « New
   * workstream » at the bottom opens the launcher's sheet, prefilled. Reached from « By project »
   * and from an agent's card, one tap away.
   */
  let { snap, repo, acts, onback }: { snap: Snapshot; repo: string; acts: boolean; onback: () => void } = $props()

  const p = $derived(projectSummaries(snap, { hidden: isHidden }).find((x) => x.name === repo))
  const STAGES: [Stage, string][] = [['active', t('In progress')], ['review', t('In review')], ['cleanup', t('Done · to clean up')], ['resume', t('To pick up')]]
  const tone = (w: Workstream) => (w.waiting ? 'asks' : w.stage === 'active' ? 'run' : '')
</script>

<div class="page">
  <header class="top">
    <button class="back" onclick={onback} aria-label={t('Back')}><Icon name="back" size="18px" /></button>
    <b>{repo}</b>
    {#if p?.repo?.behind}<span class="behind">{p.repo.base} ↓{p.repo.behind}</span>{/if}
    {#if p?.mainFailed}<span class="ko"><Icon name="fail" size="12px" /> {p.repo?.base ?? 'main'}</span>{/if}
  </header>
  {#if p}
    {#each STAGES as [stage, label] (stage)}
      {@const items = p.workstreams.filter((w) => w.stage === stage).sort(byActivity)}
      {#if items.length}
        <p class="grp">{label} · {items.length}</p>
        {#each items as w (w.key)}
          <section class="ws">
            <div class="h"><span class="dot {tone(w)}"></span><b class="ttl">{w.title}</b>{#if w.ticketId}<code class="tk">{w.ticketId}</code>{/if}</div>
            <p class="where">{w.clone ? t('on {branch}', { branch: w.branch ?? '?' }) : tildePath(w.path)}{#if w.ahead}{' '}↑{w.ahead}{/if}{#if w.changes}{' '}· {tn(w.changes, '{n} file changed', '{n} files changed')}{:else if w.dirty}{' '}· {t('local changes')}{/if}</p>
            {#each w.agents as a (a.id)}
              <button class="agent" onclick={() => openAgents(a.id)}>
                <span class="who">{cliOf(a.provider).name}{a.model ? ` · ${modelName(a.model)}` : ''}</span>
                <span class="what" class:asks={waitingAgent(a)}>{a.state === 'running' ? (waitingAgent(a) ? (a.needs_action ?? t('Your turn')) : sessionLabel(a)) : t('agent stopped')}</span>
                <span class="tm">{ago(a.last_activity ?? a.created_at)}</span>
              </button>
            {/each}
            {#if w.mr}<p class="mr"><code>{mrKey(w.mr)}</code>{#if w.mr.state === 'merged'}{' '}· {t('merged')}{:else}{#if w.mr.pipeline}{' '}· {t('pipeline')} {w.mr.pipeline}{/if}{#if w.mr.open_threads}{' '}· {tn(w.mr.open_threads, '{n} open thread', '{n} open threads')}{/if}{/if}</p>{/if}
          </section>
        {/each}
      {/if}
    {/each}
    {#if !p.workstreams.length}<p class="empty">{t('Nothing in progress.')}</p>{/if}
    {#if acts && p.repo}
      <div class="new"><NewWorkstream repo={p.name} tickets={ticketsFor(snap, p.name, p.workstreams, nextOf(p.name))} direct={false} /></div>
    {/if}
  {/if}
</div>

<style>
  .page { width: calc(100% + 20px); margin: 0 -10px; padding-bottom: 8px; }
  .top { display: flex; align-items: center; gap: 10px; padding: 4px 14px 8px; }
  .top b { font-size: 17px; }
  .back { display: inline-flex; padding: 6px; border: 1px solid var(--border); border-radius: 9px; background: none; color: var(--text); }
  .behind { color: var(--warn); font: 600 12px var(--mono); }
  .ko { display: inline-flex; align-items: center; gap: 3px; color: var(--ko); font-size: 12px; }
  .grp { margin: 0; padding: 14px 14px 6px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .ws { margin: 0 10px 8px; padding: 10px 12px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .h { display: flex; align-items: center; gap: 8px; }
  .ttl { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 15px; font-weight: 550; }
  .tk { color: var(--info); font-family: var(--mono); font-size: 12px; }
  .dot { flex: none; width: 9px; height: 9px; border-radius: 50%; background: var(--text-muted); }
  .dot.asks { background: var(--warn); } .dot.run { background: var(--info); }
  .where, .mr { margin: 4px 0 0; color: var(--text-muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mr code { font-family: var(--mono); color: var(--text); }
  .agent { width: 100%; display: flex; align-items: center; gap: 8px; margin-top: 6px; padding: 7px 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg); color: var(--text); font: inherit; font-size: 13px; text-align: left; }
  .who { font-weight: 600; white-space: nowrap; }
  .what { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
  .what.asks { color: var(--warn); font-weight: 600; }
  .tm { color: var(--text-muted); font-size: 12px; }
  .empty { margin: 24px 0; color: var(--text-muted); font-size: 13px; text-align: center; }
  .new { margin: 12px 10px 0; }
</style>
