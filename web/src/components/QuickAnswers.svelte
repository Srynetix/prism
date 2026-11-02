<script lang="ts">
  import { agentsSettings, insertInAgent } from '../agents.svelte'
  import { used } from '../feature-use.svelte'
  import { sendPrompt } from '../agent-inbox.svelte'
  import { answersFor, onPhone, sendFromPhone, sendSuggestion, suggestionOf } from '../quick-answers'
  import { toast } from '../toast.svelte'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * Quick answers (quick-answers.ts) of an agent at its prompt: a click sends the text (through the
   * inbox: never twice), ⇧-click or an answer with `submit = false` types it into its input, to
   * finish. Nothing while it works or asks for a permission. `compact`: the first three, small
   * (a pane's header, a row, an alert); `bar`: the agent view's line under the terminal. First,
   * the prompt the agent suggests itself (Claude Code's dimmed text in its input), sent as it is.
   */
  let { agent, compact = false, bar = false }: { agent: Agent; compact?: boolean; bar?: boolean } = $props()

  const all = $derived(answersFor(agentsSettings().quick_answers, agentsSettings().prompt_language, agent))
  const suggested = $derived(suggestionOf(agent))
  const shown = $derived(compact ? all.slice(0, suggested ? 2 : 3) : all)
  /** Clicked: the buttons wait for the agent to leave its prompt (or 4 s). */
  let busy = $state(false)

  async function suggest(e: MouseEvent, text: string) {
    e.stopPropagation()
    if (busy) return
    busy = true
    setTimeout(() => (busy = false), 4000)
    try {
      await sendSuggestion(agent, text)
      toast(t('“{text}” sent to {agent}', { text, agent: agent.title }))
    } catch (err) {
      busy = false
      toast(t('Not sent to {agent}: {error}', { agent: agent.title, error: String(err) }), 6000)
    }
  }

  async function answer(e: MouseEvent, q: (typeof all)[number]) {
    e.stopPropagation()
    if (busy) return
    busy = true
    setTimeout(() => (busy = false), 4000)
    used('quick-answer')
    try {
      if (onPhone()) {
        await sendFromPhone(agent, q)
        toast(q.submit ? t('“{text}” sent to {agent}', { text: q.label, agent: agent.title }) : t('Typed into {agent}: finish it there', { agent: agent.title }))
      } else if (!q.submit || e.shiftKey) {
        await insertInAgent(agent.id, q.sent)
        toast(t('Typed into {agent}: finish it there', { agent: agent.title }))
      } else {
        const r = await sendPrompt(agent.id, q.sent)
        toast(r === 'sent' ? t('“{text}” sent to {agent}', { text: q.label, agent: agent.title }) : r === 'queued' ? t('“{text}” goes to {agent} once its menu is answered', { text: q.label, agent: agent.title }) : t('Already sent to {agent}', { agent: agent.title }))
      }
    } catch (err) {
      busy = false
      toast(t('Not sent to {agent}: {error}', { agent: agent.title, error: String(err) }), 6000)
    }
  }
</script>

{#if shown.length || suggested}
  <span class="qa" class:compact class:bar role="group" aria-label={t('Quick answers to {agent}', { agent: agent.title })}>
    {#if bar}<span class="lbl">{t('Answer')}</span>{/if}
    {#if suggested}<button class="sugg" disabled={busy} onclick={(e) => suggest(e, suggested)} title={t('Its own suggestion, sent as it is: {text}', { text: suggested })}>↳ <span>{suggested}</span></button>{/if}
    {#each shown as q, i (i)}
      <button class:first={i === 0} disabled={busy} onclick={(e) => answer(e, q)} title="{q.sent}{q.submit ? ` · ${t('⇧-click: type it, to finish')}` : ` ${t('(typed, to finish)')}`}">{q.label}</button>
    {/each}
  </span>
{/if}

<style>
  .qa { display: inline-flex; align-items: center; gap: 5px; flex-wrap: wrap; min-width: 0; }
  .qa.bar { display: flex; padding: 6px 10px; border-top: 1px solid var(--border); background: var(--surface); }
  .lbl { margin-right: 2px; color: var(--text-muted); font-size: var(--fs-tag); }
  button { padding: 2px 10px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); white-space: nowrap; cursor: pointer; }
  button.first { border-color: var(--accent); }
  button:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
  button:disabled { opacity: 0.45; cursor: default; }
  .sugg { display: inline-flex; gap: 4px; max-width: 280px; border-style: dashed; border-color: var(--accent); color: var(--accent); }
  .sugg span { overflow: hidden; text-overflow: ellipsis; }
  .compact .sugg { max-width: 150px; }
  .compact { flex-wrap: nowrap; gap: 3px; }
  .compact button { padding: 0 8px; border-radius: 10px; font-size: var(--fs-tag); line-height: 1.6; }
</style>
