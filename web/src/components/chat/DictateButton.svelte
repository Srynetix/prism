<script lang="ts">
  import Icon from '../Icon.svelte'
  import { clickDictation, dictation, dictationOn, dictationSettings, micWarning, registerTarget, releaseDictation, startDictation, stopDictation, type Target } from '../../dictation.svelte'
  import { isMac, keys } from '../../platform'
  import { t } from '../../i18n.svelte'

  /**
   * The microphone beside a long text field (`dictation.svelte.ts`): held, it records while held;
   * clicked (`[dictation] toggle_on_click`), it starts, and the next click stops. The field's
   * shortcut (Fn on macOS) does the same while it has the focus. Its bar (DictationBar) says how it goes.
   */
  let { key, field, text, write, hints, disabled = false, size = 15 }: { key: string; field: () => HTMLTextAreaElement | HTMLInputElement | undefined; text: () => string; write: (v: string) => void; hints?: () => string[]; disabled?: boolean; size?: number } = $props()

  $effect(() => registerTarget(key, { field: () => field(), text: () => text(), write: (v) => write(v), hints: () => hints?.() ?? [] } satisfies Target))

  const mine = $derived(dictation.key === key)
  const recording = $derived(mine && (dictation.phase === 'recording' || dictation.phase === 'starting'))
  const busy = $derived(mine && dictation.phase === 'transcribing')
  const warned = $derived(dictation.phase === 'idle' && !!micWarning())
  const shortcut = $derived(dictationSettings().shortcut.trim())
  const title = $derived(
    recording ? (dictation.mode === 'hold' ? t('Release to put the text in (Esc cancels)') : t('Click to put the text in (Esc cancels)'))
    : busy ? t('Transcribing, on this computer…')
    : [t('Dictate: hold to speak{click}, the text goes in at the cursor, never sent on its own', { click: dictationSettings().toggle_on_click ? t(', or click to start and stop') : '' }), shortcut ? t('Shortcut: hold {key}', { key: shortcut.toLowerCase() === 'fn' ? (isMac ? 'Fn 🌐' : 'Fn') : keys(shortcut) }) : ''].filter(Boolean).join('. '),
  )

  let downAt = 0
  /** This press stopped a toggled recording: its release does nothing more. */
  let stopping = false

  function down(e: PointerEvent) {
    if (e.button !== 0 || disabled) return
    // The field keeps the focus (and its cursor, where the text goes).
    e.preventDefault()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    stopping = false
    if (mine && dictation.phase === 'recording' && dictation.mode === 'toggle') {
      stopping = true
      void stopDictation()
      return
    }
    if (dictation.phase !== 'idle') return
    downAt = Date.now()
    void startDictation(key, 'hold')
  }

  function up() {
    if (stopping) {
      stopping = false
      return
    }
    if (!mine || dictation.mode !== 'hold') return
    // A click, not a hold: it keeps recording until the next one.
    if (Date.now() - downAt < 350 && dictationSettings().toggle_on_click) dictation.mode = 'toggle'
    else releaseDictation()
  }
</script>

{#if dictationOn()}
  <button
    type="button"
    class="mic"
    class:rec={recording}
    class:busy
    class:warn={warned && !recording && !busy}
    disabled={disabled || (dictation.phase !== 'idle' && !mine)}
    onpointerdown={down}
    onpointerup={up}
    onclick={(e) => e.detail === 0 && clickDictation(key)}
    aria-label={recording ? t('Stop dictating') : t('Dictate')}
    aria-pressed={recording}
    {title}
  ><Icon name={busy ? 'refresh' : 'mic'} {size} spin={busy} /></button>
{/if}

<style>
  .mic { position: relative; flex: none; display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: none; border-radius: 50%; background: none; color: var(--text-muted); cursor: pointer; touch-action: none; }
  .mic:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  .mic:disabled { opacity: 0.4; cursor: default; }
  .mic.rec { background: var(--ko); color: var(--on-accent, #fff); animation: pulse 1.4s infinite; }
  .mic.rec:hover { background: var(--ko); color: var(--on-accent, #fff); }
  .mic.busy { color: var(--info, var(--accent)); }
  /* The microphone would hear nothing: said on the button before a click. */
  .mic.warn::after { content: ''; position: absolute; top: 1px; right: 1px; width: 7px; height: 7px; border-radius: 50%; background: var(--warn); border: 1.5px solid var(--chat-field, var(--surface)); }
  @keyframes pulse {
    0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--ko) 55%, transparent); }
    70% { box-shadow: 0 0 0 8px transparent; }
    100% { box-shadow: 0 0 0 0 transparent; }
  }
  @media (prefers-reduced-motion: reduce) { .mic.rec { animation: none; } }
</style>
