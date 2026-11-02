<script lang="ts">
  import { tick } from 'svelte'
  import { t } from '../i18n.svelte'
  import { complete, consoleKey, historyAt, historyLength, runLine, term, toggleConsole } from '../console.svelte'
  import { matches, worksWhileTyping } from '../shortcut'
  import { live } from '../live.svelte'

  /**
   * Prism console (console.svelte.ts): dropped down from the top of the window by its shortcut
   * (`[console] key`), closed by the same or Escape. ↑ / ↓ browse the history, Tab completes,
   * Ctrl+L clears the screen.
   */
  let input: HTMLInputElement | undefined = $state()
  let out: HTMLDivElement | undefined = $state()
  let value = $state('')
  let at = $state(0)
  const prompt = $derived(`${live.snapshot?.me ?? 'you'}@prism ~ %`)

  $effect(() => {
    if (!term.open) return
    at = historyLength()
    tick().then(() => input?.focus())
  })
  // Always at the bottom of the output.
  $effect(() => {
    void term.lines.length
    tick().then(() => out && (out.scrollTop = out.scrollHeight))
  })

  async function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      const line = value
      value = ''
      await runLine(line)
      at = historyLength()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      at = Math.max(0, at - 1)
      value = historyAt(at)
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      at = Math.min(historyLength(), at + 1)
      value = historyAt(at)
    } else if (e.key === 'Tab') {
      e.preventDefault()
      const c = complete(value)
      value = c.line
      if (c.options.length > 1) term.lines.push({ text: c.options.join('  '), tone: 'muted' })
    } else if (e.ctrlKey && !e.metaKey && !e.altKey && e.key.toLowerCase() === 'l') {
      // Ctrl+L clears the screen, as in a terminal: what is being typed stays.
      e.preventDefault()
      term.lines = []
    } else if (e.key === 'Escape' || (matches(e, consoleKey()) && (worksWhileTyping(consoleKey()) || !/^[a-z0-9]$/i.test(consoleKey().key)))) {
      // The shortcut also closes, except a lone letter or digit: that would be a keystroke.
      e.preventDefault()
      toggleConsole(false)
    }
    // The grid shortcuts do not see what is typed here.
    e.stopPropagation()
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="console" class:open={term.open} aria-hidden={!term.open} onclick={() => input?.focus()}>
  <div class="out" bind:this={out} role="log">
    {#each term.lines as l, i (i)}<div class={l.tone}>{l.text || ' '}</div>{/each}
  </div>
  <label class="in"><span>{prompt}</span><input bind:this={input} bind:value {onkeydown} autocomplete="off" spellcheck="false" aria-label={t('Command')} tabindex={term.open ? 0 : -1} /></label>
</div>

<style>
  .console { position: fixed; left: 0; right: 0; top: 0; z-index: 70; height: min(46vh, 420px); display: flex; flex-direction: column; background: rgba(10, 10, 14, 0.95); backdrop-filter: blur(8px); border-bottom: 1px solid color-mix(in srgb, var(--accent) 45%, transparent); box-shadow: var(--shadow-pop); color: #d8d8e2; font: 12px/1.55 var(--mono); transform: translateY(-100%); visibility: hidden; transition: transform 0.22s cubic-bezier(0.2, 0.8, 0.3, 1), visibility 0s 0.22s; }
  .console.open { transform: none; visibility: visible; transition: transform 0.22s cubic-bezier(0.2, 0.8, 0.3, 1); }
  .out { flex: 1; overflow: auto; padding: 10px 14px; white-space: pre-wrap; }
  .in { display: flex; gap: 8px; padding: 7px 14px; border-top: 1px solid #ffffff14; }
  .in span { color: #a78bfa; white-space: nowrap; }
  input { flex: 1; min-width: 0; border: none; outline: none; background: none; color: #fff; font: inherit; }
  .ok { color: #7ee0a8; }
  .muted { color: #8e8e9c; }
  .err { color: #ff8fa3; }
  .warn { color: #ffc978; }
  .cmd { color: #fff; }
  .cmd::first-letter { color: #a78bfa; }
  :global(html[data-reduce-motion]) .console, :global(html[data-reduce-motion]) .console.open { transition: none; }
</style>
