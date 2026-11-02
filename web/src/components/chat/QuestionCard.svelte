<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { answerAgent, type AgentQuestion } from '../../agents.svelte'

  /**
   * A question the agent asks (its `AskUserQuestion`), in its Chat: each question and its
   * choices, picked here and sent as its menu's keys, without the terminal; a choice with your own
   * words goes as words (question.rs). On a phone, your own
   * words for a question are typing (`[remote] typing`, the passkey). Once answered, the answers it got.
   */
  let { id, ask, status, acts, typing, passkey = false }: {
    id: string
    ask: AgentQuestion
    /** Its tool call's: running while it waits for the answer. */
    status: 'running' | 'ok' | 'failed'
    /** The phone may act (`[remote] actions`), and type (`[remote] typing`). */
    acts: boolean
    typing: boolean
    /** Your words ask the phone's passkey. */
    passkey?: boolean
  } = $props()

  const asked = $derived(status === 'running')
  let picks = $state<number[][]>([])
  let words = $state<string[]>([])
  let busy = $state(false)
  let error = $state<string | null>(null)
  let sent = $state(false)

  function toggle(q: number, n: number, multi: boolean) {
    const now = picks[q] ?? []
    picks[q] = multi ? (now.includes(n) ? now.filter((p) => p !== n) : [...now, n]) : now[0] === n ? [] : [n]
  }
  /** Every question answered. */
  const ready = $derived(ask.questions.every((_, q) => (picks[q]?.length ?? 0) > 0 || !!words[q]?.trim()))

  async function send() {
    busy = true
    error = null
    try {
      await answerAgent(id, ask.call, ask.questions.map((_, q) => ({ picks: picks[q] ?? [], ...(words[q]?.trim() ? { text: words[q].trim() } : {}) })))
      sent = true
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="ask" class:asked>
  <div class="head"><Icon name="help" /> {asked ? (sent ? t('Answer sent') : t('It asks you')) : status === 'failed' ? t('Not answered') : t('Answered')}</div>
  {#each ask.questions as q, i (i)}
    <div class="q">
      {#if q.header}<span class="tag">{q.header}</span>{/if}
      <p>{q.question}</p>
      {#if asked && !sent}
        {#if q.multi}<small>{t('Several choices')}</small>{/if}
        <div class="opts">
          {#each q.options as o, n (n)}
            {@const on = picks[i]?.includes(n + 1)}
            <button class="opt" class:on disabled={!acts || busy} onclick={() => toggle(i, n + 1, q.multi)} aria-pressed={on}>
              <span class="mark" class:box={q.multi}>{#if on}<Icon name="check" />{/if}</span>
              <span class="txt"><b>{o.label}</b>{#if o.description}<small>{o.description}</small>{/if}</span>
            </button>
          {/each}
        </div>
        {#if typing}
          <input type="text" bind:value={words[i]} disabled={!acts || busy} placeholder={`${picks[i]?.length ? t('And your own words') : t('Or your own words')}${passkey ? ` (${t('passkey')})` : ''}`} enterkeyhint="done" />
        {/if}
      {:else if ask.answers[i]}
        <p class="ans">→ {ask.answers[i]}</p>
      {/if}
    </div>
  {/each}
  {#if asked && !sent}
    {#if acts}
      <button class="send" onclick={send} disabled={!ready || busy}>{busy ? '…' : ask.questions.length > 1 ? t('Send the answers') : t('Send the answer')}</button>
    {:else}
      <p class="muted">{t('Answering from a phone is off (Settings › Remote › actions): the terminal shows it.')}</p>
    {/if}
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .ask { border: 1px solid var(--border); border-radius: 12px; background: var(--surface); padding: 10px 12px; display: flex; flex-direction: column; gap: 10px; font-size: 14px; }
  .ask.asked { border-color: var(--info); box-shadow: 0 0 0 1px color-mix(in srgb, var(--info) 30%, transparent); }
  .head { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .asked .head { color: var(--info); }
  .q p { margin: 2px 0 6px; font-weight: 600; line-height: 1.35; }
  .tag { font-size: 11px; padding: 1px 7px; border-radius: 7px; background: var(--surface-muted); border: 1px solid var(--border); color: var(--text-muted); }
  small { display: block; font-size: 12px; color: var(--text-muted); }
  .q > small { margin: -4px 0 6px; }
  .opts { display: flex; flex-direction: column; gap: 6px; }
  .opt { display: flex; align-items: flex-start; gap: 10px; width: 100%; text-align: left; font: inherit; color: var(--text); background: var(--surface-muted); border: 1px solid var(--border); border-radius: 10px; padding: 9px 10px; }
  .opt.on { border-color: var(--info); background: var(--info-bg); }
  .mark { flex: none; width: 18px; height: 18px; margin-top: 1px; border-radius: 50%; border: 1.5px solid var(--text-muted); display: grid; place-items: center; color: var(--info); font-size: 12px; }
  .mark.box { border-radius: 4px; }
  .on .mark { border-color: var(--info); }
  .txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .txt b { font-weight: 600; }
  input { margin-top: 6px; width: 100%; box-sizing: border-box; border: 1px solid var(--border); background: var(--surface-muted); color: var(--text); border-radius: 10px; padding: 8px 10px; font: inherit; font-size: 15px; }
  .send { align-self: stretch; border: none; border-radius: 10px; padding: 10px; background: var(--accent); color: var(--on-accent); font: inherit; font-weight: 600; }
  .send:disabled, .opt:disabled { opacity: 0.5; }
  .ans { font-weight: 400 !important; color: var(--ok); margin: 0 !important; }
  .muted, .error { margin: 0; font-size: 12.5px; color: var(--text-muted); }
  .error { color: var(--ko); }
</style>
