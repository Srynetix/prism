<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { openThread, resolveThread, type ThreadRef } from '../thread.svelte'
  import { agentName } from '../agent-cli'
  import { mrWord } from '../forge'

  /**
   * A row's thread folded into small squares: worktree, agent, MR, pipeline, then the
   * environments updated since the merge. Green: there; dashed orange: should be there by now;
   * red: broken (a red pipeline). A click opens the thread drawer. Nothing for a ticket no work
   * touched yet.
   */
  let { at }: { at: ThreadRef } = $props()

  type Tone = 'on' | 'miss' | 'bad' | 'off'
  const th = $derived(live.snapshot ? resolveThread(at, live.snapshot, Date.now(), false) : null)
  const squares = $derived.by((): { k: string; c: string; tone: Tone; title: string }[] => {
    if (!th) return []
    const m = th.mr
    const p = th.pipeline
    const a = th.agents.find((x) => x.state === 'running') ?? th.agents[0]
    const out: { k: string; c: string; tone: Tone; title: string }[] = [
      { k: 'wt', c: '⑂', tone: th.worktree ? 'on' : th.missing.has('worktree') ? 'miss' : 'off', title: th.worktree ? t('Worktree {name}', { name: th.worktree.path.split('/').pop() ?? '' }) : th.missing.has('worktree') ? t('In progress, but no worktree, branch or agent') : t('No worktree') },
      { k: 'agent', c: '✦', tone: a ? (a.state === 'running' ? 'on' : 'off') : 'off', title: a ? `${agentName()}: ${a.title}${a.state === 'running' ? '' : ` ${t('(stopped)')}`}` : t('No {name}', { name: agentName() }) },
      { k: 'mr', c: '⇄', tone: m ? 'on' : th.missing.has('mr') ? 'miss' : 'off', title: m ? `${mrWord()} ${m.title}${m.state === 'merged' ? ` ${t('(merged)')}` : ''}` : th.missing.has('mr') ? t('Commits on the branch, but no MR') : t('No {name}', { name: mrWord() }) },
      { k: 'ci', c: p?.status === 'failed' ? '✕' : p?.status === 'canceled' ? '⊘' : '✓', tone: p ? (p.status === 'failed' ? 'bad' : p.status === 'success' ? 'on' : 'off') : m?.state === 'merged' ? 'on' : 'off', title: p ? t('Pipeline {status}', { status: p.status }) : m?.state === 'merged' ? tc('tickets', 'Merged') : t('No pipeline') },
    ]
    for (const e of th.envs) {
      const miss = th.missing.has(`env:${e.env}`)
      out.push({ k: `env:${e.env}`, c: e.env.slice(0, 1).toUpperCase(), tone: e.since ? 'on' : miss ? 'miss' : 'off', title: !m?.merged_at ? t('{env}: not merged yet', { env: e.env }) : e.since ? t('{env}: updated since the merge', { env: e.env }) : miss ? t('{env}: not updated since the merge, days ago', { env: e.env }) : t('{env}: not updated since the merge', { env: e.env }) })
    }
    return out
  })
  const touched = $derived(squares.some((q) => q.tone !== 'off'))
</script>

{#if touched}
  <button class="links" onclick={(e) => { e.stopPropagation(); openThread(at) }} title="{t('Thread (t)')}: {squares.map((q) => q.title).join(' · ')}">
    {#each squares as q (q.k)}<i class={q.tone}>{q.c}</i>{/each}
  </button>
{/if}

<style>
  .links { display: inline-flex; gap: 2px; padding: 0; border: none; background: none; cursor: pointer; vertical-align: middle; }
  i { display: grid; place-items: center; width: 14px; height: 14px; border: 1px solid var(--border); border-radius: var(--r3); background: var(--surface-muted); color: var(--text-muted); font: 9px / 1 var(--mono); font-style: normal; opacity: 0.6; }
  i.on { color: var(--ok); border-color: color-mix(in srgb, var(--ok) 45%, var(--border)); opacity: 1; }
  i.bad { color: var(--ko); border-color: color-mix(in srgb, var(--ko) 55%, var(--border)); opacity: 1; }
  i.miss { color: var(--warn); border: 1px dashed var(--warn); opacity: 1; }
  .links:hover i { opacity: 1; }
</style>
