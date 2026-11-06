<script lang="ts">
  import Icon from '../Icon.svelte'
  import { cancelDictation, dictation, raiseAndDictate, restoreGain, startDictation, stopDictation } from '../../dictation.svelte'
  import { t } from '../../i18n.svelte'

  /**
   * How a dictation into `key`'s field goes, under it: recording (the time, the microphone's level,
   * its name), transcribing, the microphone that would hear nothing (Raise, or dictate anyway), a
   * failure. Nothing otherwise.
   */
  let { key }: { key: string } = $props()

  const mine = $derived(dictation.key === key)
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`
  const warn = $derived(dictation.warn?.key === key ? dictation.warn.why : null)
  const error = $derived(dictation.error?.key === key && dictation.phase === 'idle' ? dictation.error.text : null)
</script>

{#if mine && (dictation.phase === 'recording' || dictation.phase === 'starting')}
  <div class="bar rec" role="status">
    <span class="dot"></span><b class="time">{clock(dictation.seconds)}</b>
    <span class="meter" aria-hidden="true">{#each Array.from({ length: 24 }, (_, i) => dictation.levels[dictation.levels.length - 24 + i] ?? 0) as l, i (i)}<i style:height="{Math.max(2, Math.min(18, 2 + l * 40))}px"></i>{/each}</span>
    {#if dictation.device}<span class="dev">{dictation.device}</span>{/if}
    <span class="grow"></span>
    <span class="hint">{dictation.mode === 'hold' ? t('release to put it in') : t('click 🎙 to put it in')} · <kbd>Esc</kbd> {t('cancels')}</span>
    {#if dictation.mode === 'toggle'}<button type="button" class="btn" onclick={() => void stopDictation()}>{t('Put it in')}</button>{/if}
  </div>
{:else if mine && dictation.phase === 'transcribing'}
  <div class="bar busy" role="status">
    <Icon name="refresh" spin /> {t('Transcribing {time}…', { time: clock(dictation.seconds) })}
    <span class="grow"></span><span class="hint">{t('on this computer')}</span>
    <button type="button" class="btn" onclick={cancelDictation}>{t('Cancel')}</button>
  </div>
{:else if warn}
  <div class="bar warn" role="alert">
    <Icon name="warning" />
    <span>{warn.kind === 'muted' ? t('Microphone muted: nothing would be heard.') : warn.kind === 'off' ? t('Microphone at {n} % (turned off?): nothing would be heard.', { n: warn.volume }) : t('Microphone at {n} %: little would be heard.', { n: warn.volume })}</span>
    <span class="grow"></span>
    <button type="button" class="btn" onclick={() => void raiseAndDictate(key)}>{warn.kind === 'muted' ? t('Unmute and dictate') : t('Raise to {n} % and dictate', { n: restoreGain() })}</button>
    <button type="button" class="btn" onclick={() => void startDictation(key, 'toggle', true)}>{t('Dictate anyway')}</button>
    <button type="button" class="x" onclick={cancelDictation} aria-label={t('Close')}><Icon name="close" /></button>
  </div>
{:else if error}
  <div class="bar ko" role="alert">
    <Icon name="warning" /><span class="msg">{error}</span><span class="grow"></span>
    <button type="button" class="x" onclick={() => (dictation.error = null)} aria-label={t('Close')}><Icon name="close" /></button>
  </div>
{:else if dictation.done?.key === key}
  <div class="bar done"><Icon name="check" /> {t('Dictated: read it over, then send')}<span class="grow"></span><span class="hint">{dictation.done.text}</span></div>
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 4px 8px; border-radius: 8px; font-size: var(--fs-meta); }
  .rec { background: var(--ko-bg, var(--surface-muted)); color: var(--ko); }
  .busy { background: var(--info-bg, var(--surface-muted)); color: var(--info, var(--text)); }
  .warn { background: var(--warn-bg, var(--surface-muted)); color: var(--warn); flex-wrap: wrap; }
  .ko { background: var(--ko-bg, var(--surface-muted)); color: var(--ko); }
  .done { color: var(--ok); padding-block: 0; }
  .dot { flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--ko); animation: blink 1s steps(1) infinite; }
  .time { font-variant-numeric: tabular-nums; }
  .meter { display: flex; align-items: center; gap: 2px; height: 18px; flex: none; }
  .meter i { width: 3px; border-radius: 2px; background: currentColor; opacity: 0.8; transition: height 80ms linear; }
  .dev { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; opacity: 0.85; }
  .grow { flex: 1; }
  .hint { flex: none; color: inherit; opacity: 0.85; white-space: nowrap; }
  .msg { min-width: 0; overflow-wrap: anywhere; }
  kbd { font: 10px ui-monospace, 'SF Mono', Menlo, monospace; padding: 0 4px; border: 1px solid currentColor; border-radius: 4px; }
  .btn { flex: none; padding: 1px 8px; border: 1px solid color-mix(in srgb, currentColor 45%, transparent); border-radius: 6px; background: var(--surface); color: var(--text); font: inherit; cursor: pointer; white-space: nowrap; }
  .btn:hover { border-color: var(--accent); }
  .x { flex: none; display: grid; place-items: center; padding: 2px; border: none; background: none; color: inherit; cursor: pointer; }
  @keyframes blink { 50% { opacity: 0; } }
  @media (prefers-reduced-motion: reduce) { .dot { animation: none; } .meter i { transition: none; } }
</style>
