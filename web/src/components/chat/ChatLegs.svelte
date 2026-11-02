<script lang="ts">
  import Icon from '../Icon.svelte'
  import Markdown from '../Markdown.svelte'
  import { CLIS } from '../../agent-cli'
  import { legConversation } from '../../agent-switch.svelte'
  import { dateFormat } from '../../locale'
  import { t, tn } from '../../i18n.svelte'
  import type { AgentConversation, ConversationTurn } from '../../agents.svelte'
  import type { Agent } from '../../types'

  /**
   * Above an agent's Chat, the CLIs it ran before its current one (Continue with…), oldest first:
   * each stretch folded, its turns read on a click (your prompts and its last words, faded), then
   * the relay that handed it over, its message a click away.
   */
  let { agent }: { agent: Agent } = $props()

  const legs = $derived(agent.legs ?? [])
  let open = $state<Record<number, AgentConversation | null | 'loading'>>({})
  let shown = $state<Record<number, boolean>>({})

  async function toggle(i: number) {
    if (open[i] !== undefined) {
      delete open[i]
      return
    }
    open[i] = 'loading'
    try {
      open[i] = await legConversation(agent.id, i, 200)
    } catch {
      open[i] = null
    }
  }
  const time = (iso: string) => dateFormat({ hour: '2-digit', minute: '2-digit' }).format(new Date(iso))
  const last = (turn: ConversationTurn) => [...turn.entries].reverse().find((e) => e.type === 'text')
  const next = (i: number) => legs[i + 1]?.provider ?? agent.provider
  const reasonOf = (r: string) => (r === 'limit' ? t('its limit reached') : t('chosen'))
</script>

{#each legs as leg, i (i)}
  {@const c = open[i]}
  <button class="fold" onclick={() => toggle(i)} aria-expanded={c !== undefined}>
    <Icon name={c !== undefined ? 'down' : 'next'} size={11} />
    <Icon name={CLIS[leg.provider].icon} size={12} />
    <span>{leg.usage?.replies ? tn(leg.usage.replies, 'With {cli}: {n} reply', 'With {cli}: {n} replies', { cli: CLIS[leg.provider].name }) : t('With {cli}', { cli: CLIS[leg.provider].name })}</span>
    <span class="when">{time(leg.started)} → {time(leg.ended)}</span>
  </button>
  {#if c === 'loading'}
    <p class="muted">…</p>
  {:else if c === null}
    <p class="muted">{t('Its conversation is no longer on disk, or {cli} keeps none Prism reads.', { cli: CLIS[leg.provider].name })}</p>
  {:else if c}
    <div class="old">
      {#each c.turns as turn (turn.n)}
        {@const words = last(turn)}
        {#if turn.prompt}<div class="me">{turn.prompt}</div>{/if}
        {#if words && words.type === 'text'}<div class="ai"><span class="who"><Icon name={CLIS[leg.provider].icon} size={11} />{CLIS[leg.provider].name}</span><Markdown text={words.text} reading /></div>{/if}
      {/each}
    </div>
  {/if}
  <div class="divider">
    <span class="card">
      <Icon name={CLIS[leg.provider].icon} size={12} />→<Icon name={CLIS[next(i)].icon} size={12} />
      {t('Relay {time} · {reason}', { time: time(leg.ended), reason: reasonOf(leg.reason) })}
      <button class="link" onclick={() => (shown[i] = !shown[i])}>{shown[i] ? t('hide the message') : t('see the message')}</button>
    </span>
  </div>
  {#if shown[i]}<pre class="msg">{leg.message}</pre>{/if}
{/each}

<style>
  .fold { display: flex; align-items: center; gap: 6px; width: 100%; padding: 6px 10px; border: 1px dashed var(--border); border-radius: 10px; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .fold span:first-of-type { flex: 1; }
  .when { font-size: var(--fs-tag, 11px); }
  .old { display: flex; flex-direction: column; gap: 8px; opacity: .6; }
  .me { align-self: flex-end; max-width: 85%; background: var(--surface-muted); padding: 6px 11px; border-radius: 14px 14px 4px 14px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .ai { display: flex; flex-direction: column; gap: 2px; }
  .who { display: inline-flex; align-items: center; gap: 4px; color: var(--text-muted); font-size: var(--fs-tag, 11px); }
  .divider { display: flex; align-items: center; gap: 10px; margin: 4px 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .divider::before, .divider::after { content: ''; flex: 1; height: 1px; background: var(--border); }
  .card { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); color: var(--text); }
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; }
  .msg { margin: 0; max-height: 260px; overflow: auto; padding: 8px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); font-family: var(--mono); font-size: var(--fs-tag, 11px); line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
  .muted { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
