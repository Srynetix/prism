<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t, tn } from '../../i18n.svelte'
  import CommentableText from './CommentableText.svelte'
  import { hasCore, invoke, isDesktop } from '../../bridge'
  import { sendPrompt } from '../../agent-inbox.svelte'
  import { compose, passages } from '../../comments'
  import { answerTarget, draftsOf, forget } from '../../comments.svelte'
  import { armer } from '../../armed.svelte'

  /**
   * A long answer of the agent's in its Chat (`[agents] comment_answers`), its passages commented
   * on as its plans are; the comments go as one prompt, each passage quoted: at once, or once its
   * menu is answered (agent-inbox), from a phone typed as its answer box does (`agent_type`, the
   * passkey). Sent, they are forgotten: your prompt in the conversation holds them.
   */
  let { id, text, acts, typing }: { id: string; text: string; acts: boolean; typing: boolean } = $props()

  const target = $derived(answerTarget(text))
  const drafts = $derived(draftsOf(id, target))
  let preview = $state(false)
  let busy = $state(false)
  let note = $state<string | null>(null)
  const armed = armer()
  const message = $derived(compose('answer', drafts, '', passages(text)))

  async function send() {
    busy = true
    note = null
    try {
      if (hasCore() && !isDesktop()) await invoke('agent_type', { id, text: message })
      else if ((await sendPrompt(id, message)) === 'queued') note = t('Sent once its menu is answered')
      forget(id, target)
    } catch (e) {
      note = t('Not sent: {error}', { error: String(e) })
    } finally {
      busy = false
    }
  }
</script>

<div class="ans">
  <CommentableText {id} {target} {text} editable={acts && typing} reading />
  {#if drafts.length}
    <div class="tray">
      <Icon name="comment" size={12} /><b>{tn(drafts.length, '{n} comment', '{n} comments')}</b>
      <span class="grow"></span>
      <button class="lnk" onclick={() => (preview = !preview)}>{preview ? t('Hide') : t('Preview')}</button>
      <button class="lnk" class:armed={armed.is('clear')} onclick={() => armed.confirm('clear') && forget(id, target)}>{armed.is('clear') ? t('Clear them?') : t('Clear')}</button>
      <button class="b" disabled={busy} onclick={send} title={t('Your comments to it, as one prompt, each passage quoted')}>{t('Send comments')}</button>
    </div>
    {#if preview}<pre class="pv">{message}</pre>{/if}
  {/if}
  {#if note}<p class="note">{note}</p>{/if}
</div>

<style>
  .ans { overflow-wrap: anywhere; }
  .tray { display: flex; align-items: center; gap: 6px; margin-top: 4px; padding: 4px 8px; border-radius: 7px; background: color-mix(in srgb, var(--warn) 14%, transparent); font-size: 12px; }
  .grow { flex: 1; }
  .lnk { border: none; background: none; padding: 0 2px; color: var(--info); font: inherit; font-size: 12px; cursor: pointer; }
  .lnk.armed { color: var(--ko); font-weight: 600; }
  .b { height: 24px; padding: 0 10px; border: 1px solid var(--accent); border-radius: 6px; background: var(--accent); color: var(--on-accent); font: inherit; font-size: 12px; cursor: pointer; }
  .b:disabled { opacity: 0.5; }
  .pv { margin: 4px 0 0; padding: 7px 9px; border: 1px solid var(--border); border-radius: 7px; background: var(--surface); font: 11.5px/1.45 ui-monospace, 'SF Mono', Menlo, monospace; white-space: pre-wrap; }
  .note { margin: 4px 0 0; font-size: 12px; color: var(--text-muted); }
</style>
