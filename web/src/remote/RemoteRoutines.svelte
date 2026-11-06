<script lang="ts">
  import { live } from '../live.svelte'
  import { t } from '../i18n.svelte'
  import { builtinLabel, enableRoutine, nextTime, routinesOf, STATE_LABEL, whenWords } from '../routines.svelte'

  /**
   * More › Settings › Routines on a phone: read only — a routine launches an agent on the computer
   * without anyone looking, as a custom block runs code. Only its switch changes from here, each
   * time with the passkey (`routine_enable` is critical); its trust is given on the computer.
   */
  const rs = $derived(routinesOf(live.snapshot))
  const list = $derived(live.snapshot?.agents_settings?.routines ?? [])
  let busy = $state('')
  /** The routines whose prompt is shown whole. */
  let open = $state<string[]>([])
  const flip = (name: string) => (open = open.includes(name) ? open.filter((n) => n !== name) : [...open, name])
  async function toggle(name: string, on: boolean) {
    busy = name
    try {
      await enableRoutine(name, on)
    } finally {
      busy = ''
    }
  }
</script>

{#if !rs.keep_running}<p class="note warn">{t('Prism closed = no routine: Keep running is off on the computer.')}</p>{/if}
<div class="box">
  {#each list as r (r.name)}
    {@const v = rs.routines.find((x) => x.name === r.name)}
    {@const last = rs.runs.find((x) => x.routine === r.name && x.state !== 'skipped')}
    <div class="row">
      <div class="txt">
        <b>{r.name}</b>
        {#if r.prompt?.trim()}
          <button class="prompt" class:open={open.includes(r.name)} onclick={() => flip(r.name)} aria-expanded={open.includes(r.name)} title={t('The routine’s own prompt: tap to read it whole')}>{r.prompt.trim()}</button>
        {/if}
        <small>{r.prompt?.trim() ? t('Prompt') : r.action.startsWith('builtin:') ? builtinLabel(r.action) : r.action}{r.prompt?.trim() && r.mode ? ` · ${r.mode}` : ''}{v?.repos.length ? ` · ${v.repos.join(', ')}` : ''} · {r.deliver === 'digest' ? t('Digest') : t('To review')}</small>
        <small>{whenWords(r)}{v?.next[0] ? ` · ${t('next: {time}', { time: nextTime(v.next[0]) })}` : ''}</small>
        {#if v?.error}<small class="ko">{v.error}</small>{:else if v?.pending}<small class="warn">{t('waits for your trust, given on the computer')}</small>{:else if last}<small>{t('last: {state}', { state: STATE_LABEL[last.state] })}</small>{/if}
      </div>
      <button class="sw" class:on={r.enabled} disabled={busy === r.name} role="switch" aria-checked={r.enabled} aria-label={t('On')} onclick={() => toggle(r.name, !r.enabled)}></button>
    </div>
  {:else}
    <p class="note">{t('No routine yet: they are written in Settings on the computer.')}</p>
  {/each}
</div>
<p class="note">{t('Read only on the phone; its switch asks for your passkey.')}</p>

<style>
  .box { display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .row { display: flex; align-items: center; gap: 10px; padding: 10px 12px; border-top: 1px solid var(--border); }
  .row:first-child { border-top: none; }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .txt small { color: var(--text-muted); font-size: 12px; }
  .prompt { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin: 2px 0; padding: 4px 8px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface-muted); color: var(--text); font: 12px/1.4 var(--mono, ui-monospace, monospace); text-align: left; white-space: pre-wrap; overflow-wrap: anywhere; }
  .prompt.open { display: block; -webkit-line-clamp: unset; line-clamp: unset; }
  .ko { color: var(--ko) !important; }
  .warn { color: var(--warn) !important; }
  .note { margin: 12px 4px; color: var(--text-muted); font-size: 13px; text-align: center; }
  .sw { flex: none; width: 42px; height: 24px; padding: 0; border-radius: 12px; background: var(--surface-muted); border: 1px solid var(--border); position: relative; }
  .sw::after { content: ''; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: var(--text-muted); transition: left 0.12s; }
  .sw.on { background: var(--ok); border-color: var(--ok); }
  .sw.on::after { left: 20px; background: var(--surface); }
  .sw:disabled { opacity: 0.45; }
</style>
