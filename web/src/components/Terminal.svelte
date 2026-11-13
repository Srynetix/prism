<script lang="ts">
  import { onMount } from 'svelte'
  import { Terminal } from '@xterm/xterm'
  import { FitAddon } from '@xterm/addon-fit'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { ClipboardAddon, type IClipboardProvider } from '@xterm/addon-clipboard'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import { mrScopeOf, refsIn } from '../linkify'
  import { live } from '../live.svelte'
  import { openRef, openUrlOrRef } from '../ref-links'
  import { invoke } from '../bridge'
  import { toast } from '../toast.svelte'
  import { isMac } from '../platform'
  import '@xterm/xterm/css/xterm.css'
  import { terminalTheme } from '../terminal-theme'
  import { agentsSettings, agentsView, attachAgent, terminalKeeps, detachAgent, inCopyMode, layoutKey, leaveCopyMode, paneArrow, resizeAgent, terminalFontSize, writeAgent } from '../agents.svelte'
  import { t } from '../i18n.svelte'

  /**
   * An agent's terminal: xterm.js attached to its tmux session. Leaving it only detaches this
   * terminal; the agent keeps going, and coming back redraws its screen. Dark background whatever
   * Prism's theme, like Claude Code's default theme.
   */
  /** `active`: the displayed terminal; a hidden terminal stays attached but does not take the keyboard. */
  let { id, onend, active = true }: { id: string; onend?: () => void; active?: boolean } = $props()

  /**
   * Terminal clipboard (OSC 52): a mouse selection in tmux arrives here and goes
   * to the system clipboard (written by the backend); a program's own OSC 52 stops at tmux
   * (`set-clipboard external`). Reading is refused: a program in the terminal does not read
   * the clipboard.
   */
  const clipboard: IClipboardProvider = {
    readText: () => '',
    writeText: async (_selection, text) => {
      if (!text) return
      // In the scrolled history, tmux keeps the selection in sight, in copy mode: the next key
      // takes it out first.
      mayBeInCopyMode = true
      watchCopyMode()
      try {
        await invoke('copy_text', { text })
        toast(t('Copied'))
      } catch (e) {
        toast(t('Could not copy: {error}', { error: String(e) }))
      }
    },
  }
  /**
   * Extension 0.1.0 takes the base64 encoder then the provider, whatever its typings say
   * (a single parameter): the provider goes second, the default encoder first.
   */
  const clipboardAddon = () => new (ClipboardAddon as unknown as new (base64: undefined, provider: IClipboardProvider) => ClipboardAddon)(undefined, clipboard)
  /**
   * The pane may be in tmux's copy mode (a selection kept in sight, the history scrolled with the
   * wheel): the next key first takes it out (`leaveCopyMode`), otherwise copy mode would swallow
   * it. Set by the mouse and by a copy; a key clears it.
   */
  let mayBeInCopyMode = false
  /**
   * The pane is in copy mode, read from tmux shortly after the mouse or a copy, then every second
   * while it lasts: its view stays still meanwhile, which the terminal says. `copyRead` drops a
   * read that a key or a click overtook.
   */
  let copyMode = $state(false)
  let copyTimer: ReturnType<typeof setTimeout> | undefined
  let copyRead = 0
  function watchCopyMode(delay = 250) {
    clearTimeout(copyTimer)
    const read = ++copyRead
    copyTimer = setTimeout(async () => {
      const on = await inCopyMode(id).catch(() => false)
      if (read !== copyRead) return
      copyMode = on
      if (on) watchCopyMode(1000)
    }, delay)
  }
  function forgetCopyMode() {
    clearTimeout(copyTimer)
    copyRead++
    copyMode = false
  }
  /** The badge's click: out of copy mode, the keyboard back to the terminal. */
  function leave() {
    mayBeInCopyMode = false
    forgetCopyMode()
    void leaveCopyMode(id).catch(() => {})
    term?.focus()
  }
  /**
   * Links: plain URLs and OSC 8 ones (Claude Code's) open with ⌘-click (Ctrl-click elsewhere); a
   * plain click stays tmux's (select, place the cursor). A ticket or an MR, named (`ABC-12`, `!1957`)
   * or linked, opens in its panel over the terminal; ⇧ added, in the browser.
   */
  const modifier = (e: MouseEvent) => (isMac ? e.metaKey : e.ctrlKey)
  const openOnModifier = (e: MouseEvent, uri: string) => {
    if (modifier(e)) openUrlOrRef(uri, e.shiftKey)
  }
  let el: HTMLDivElement
  /**
   * Right-click menu: Prism's own, not tmux's (which would offer to split or kill the pane). The
   * right button never reaches tmux; `selection`: xterm's (⌥-drag), a tmux one being copied at once.
   */
  let menu = $state<{ x: number; y: number; selection: string } | null>(null)
  async function copySelection(text: string) {
    menu = null
    try {
      await invoke('copy_text', { text })
      toast(t('Copied'))
    } catch (e) {
      toast(t('Could not copy: {error}', { error: String(e) }))
    }
  }
  async function paste() {
    menu = null
    try {
      const text = await invoke<string>('paste_text')
      if (text) term?.paste(text)
      term?.focus()
    } catch (e) {
      toast(t('Could not paste: {error}', { error: String(e) }))
    }
  }
  const terminalFont = () => getComputedStyle(document.documentElement).getPropertyValue('--mono-term').trim()
  let term: Terminal | undefined
  let fitter: FitAddon | undefined

  /** Text size follows the setting (`[agents] font_size`, ⌘+ / ⌘-) without reopening the terminal. */
  $effect(() => {
    const size = terminalFontSize()
    if (!term || term.options.fontSize === size) return
    term.options.fontSize = size
    fitter?.fit()
  })

  /** Palette close to the mockups'; Claude Code's ANSI colours stay readable with it. */
  onMount(() => {
    const mono = terminalFont() || 'ui-monospace, Menlo, monospace'
    const { scrollback } = agentsSettings()
    const xt = new Terminal({
      fontFamily: mono, fontSize: terminalFontSize(), lineHeight: 1.15, theme: terminalTheme(), scrollback, cursorBlink: true, allowProposedApi: true, macOptionClickForcesSelection: true,
      linkHandler: { activate: openOnModifier, allowNonHttpProtocols: false },
    })
    term = xt
    const fit = new FitAddon()
    fitter = fit
    xt.loadAddon(fit)
    xt.loadAddon(clipboardAddon())
    xt.loadAddon(new WebLinksAddon(openOnModifier))
    // Tickets and MRs named in the output (outside a URL): ⌘-click opens their panel.
    xt.registerLinkProvider({
      provideLinks(y, done) {
        const line = xt.buffer.active.getLine(y - 1)?.translateToString(true) ?? ''
        const refs = refsIn(line, mrScopeOf(id, live.snapshot), live.snapshot)
        done(refs.length ? refs.map((r) => ({
          range: { start: { x: r.start + 1, y }, end: { x: r.end, y } },
          text: r.text,
          decorations: { pointerCursor: true, underline: true },
          activate: (e: MouseEvent) => {
            if (modifier(e)) openRef(r.ref, e)
          },
        })) : undefined)
      },
    })
    xt.open(el)
    try {
      const webgl = new WebglAddon()
      webgl.onContextLoss(() => webgl.dispose())
      xt.loadAddon(webgl)
    } catch {
      // Without WebGL, xterm keeps its DOM renderer, slower but complete.
    }
    // The right button (and Ctrl-click on macOS) opens Prism's menu: caught before xterm passes it
    // on to tmux as a mouse event.
    const secondary = (e: MouseEvent) => e.button === 2 || (isMac && e.button === 0 && e.ctrlKey)
    const swallow = (e: MouseEvent) => {
      if (secondary(e)) e.stopPropagation()
    }
    const openMenu = (e: MouseEvent) => {
      e.preventDefault()
      e.stopPropagation()
      const r = el.getBoundingClientRect()
      menu = { x: Math.min(e.clientX - r.left, r.width - 170), y: Math.min(e.clientY - r.top, r.height - 80), selection: xt.getSelection() }
    }
    el.addEventListener('mousedown', swallow, true)
    el.addEventListener('mouseup', swallow, true)
    el.addEventListener('contextmenu', openMenu, true)
    // App chords stay with Prism and the system: ⌘ on macOS (⌘C / ⌘V go through the clipboard).
    // Elsewhere Ctrl belongs to the program (Ctrl+C, Ctrl+V for Claude's image paste) and Prism
    // takes Ctrl+Shift. Ctrl+Shift+V pastes the system clipboard, read by the backend: WebKitGTK
    // binds no paste to it.
    xt.attachCustomKeyEventHandler((e) => {
      // A shortcut the terminal keeps (`[agents] terminal_keys`): ⌘K clears it, as in a terminal
      // app; another goes to the program.
      if (terminalKeeps(e)) {
        if (e.type === 'keydown' && (isMac ? e.metaKey : e.ctrlKey) && e.key.toLowerCase() === 'k' && !e.shiftKey && !e.altKey) {
          e.preventDefault()
          xt.clear()
          return false
        }
        return true
      }
      // The Agents view's own: its layout, and the focus moving between agents side by side.
      if (layoutKey(e) || paneArrow(e)) return false
      if (isMac) return !e.metaKey
      if (!(e.ctrlKey && e.shiftKey)) return true
      if (e.type === 'keydown' && e.code === 'KeyV') {
        e.preventDefault()
        invoke<string>('paste_text').then((text) => text && xt.paste(text)).catch((err) => toast(t('Could not paste: {error}', { error: String(err) })))
      }
      return false
    })
    fit.fit()
    // A font still loading (the bundled one, at launch) was measured as its fallback: measure it
    // again once there (a different font stack makes xterm measure).
    if (!document.fonts.check(`16px ${mono}`)) {
      void document.fonts.load(`16px ${mono}`).then(() => {
        if (term !== xt) return
        xt.options.fontFamily = `${mono}, monospace`
        fit.fit()
      })
    }

    // The terminal follows the app theme and its font (`[appearance] font_terminal`, otherwise
    // `font_mono`, on `--mono-term`).
    const themed = new MutationObserver(() => {
      xt.options.theme = terminalTheme()
      const font = terminalFont()
      if (font && font !== xt.options.fontFamily) {
        xt.options.fontFamily = font
        fit.fit()
      }
    })
    themed.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'style'] })

    let disposed = false
    // A key while the pane may be in copy mode: out of it first, then the key, in order. Mouse
    // reports (`\e[<…`, `\e[M…`) and focus ones go to tmux as they are, and may enter it.
    let queue: Promise<unknown> = Promise.resolve()
    let leaving = false
    const input = xt.onData((d) => {
      if (/^\x1b\[(<|M|I$|O$)/.test(d)) {
        if (d.startsWith('\x1b[<') || d.startsWith('\x1b[M')) {
          mayBeInCopyMode = true
          watchCopyMode()
        }
        return void writeAgent(id, d)
      }
      if (mayBeInCopyMode) {
        mayBeInCopyMode = false
        forgetCopyMode()
        leaving = true
        queue = leaveCopyMode(id).catch(() => {}).finally(() => (leaving = false))
      }
      if (leaving) queue = queue.then(() => writeAgent(id, d))
      else void writeAgent(id, d)
    })
    const resize = xt.onResize(({ cols, rows }) => resizeAgent(id, cols, rows))
    attachAgent(id, xt.cols, xt.rows, (bytes) => xt.write(bytes), () => !disposed && onend?.())
      .then(() => active && xt.focus())
      .catch((e) => xt.write(`\r\n\x1b[31m${String(e)}\x1b[0m\r\n`))

    // The terminal follows its frame's size; tmux redraws the agent's screen at the new
    // size. Hidden view (zero-size frame): nothing changes, the agent keeps its size.
    let frame = 0
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        if (el.clientWidth && el.clientHeight) fit.fit()
      })
    })
    ro.observe(el)

    return () => {
      el.removeEventListener('mousedown', swallow, true)
      el.removeEventListener('mouseup', swallow, true)
      el.removeEventListener('contextmenu', openMenu, true)
      disposed = true
      clearTimeout(copyTimer)
      ro.disconnect()
      themed.disconnect()
      input.dispose()
      resize.dispose()
      void detachAgent(id)
      xt.dispose()
      term = undefined
    }
  })

  /** Back on the Agents view, or on this agent: the keyboard returns to the displayed terminal. */
  $effect(() => {
    if (agentsView.open && active) requestAnimationFrame(() => term?.focus())
  })
</script>

<svelte:window onmousedown={(e) => menu && !(e.target as HTMLElement).closest('.tmenu') && (menu = null)} onkeydown={(e) => menu && e.key === 'Escape' && (menu = null)} onblur={() => (menu = null)} />

<div class="term" bind:this={el}>
  {#if copyMode}
    <!-- tmux shows a still view while in copy mode: the agent goes on behind it. -->
    <button class="copymode" onclick={leave} title={t('tmux’s copy mode: the view is frozen while the agent goes on. A key, a click here, or scrolling to the bottom leaves it.')}>{t('Copy mode · view paused — press a key')}</button>
  {/if}
  {#if menu}
    {@const m = menu}
    <div class="tmenu" role="menu" style:left="{m.x}px" style:top="{m.y}px">
      <button role="menuitem" disabled={!m.selection} onclick={() => copySelection(m.selection)} title={m.selection ? undefined : t('Drag to select: the selection is copied at once (⌥-drag: a selection kept to copy here)')}>{t('Copy')}{#if isMac}<kbd>⌘C</kbd>{/if}</button>
      <button role="menuitem" onclick={paste}>{t('Paste')}<kbd>{isMac ? '⌘V' : 'Ctrl+Shift+V'}</kbd></button>
    </div>
  {/if}
</div>

<style>
  /* content-box: xterm's fit reads this frame's height and width as the room for the rows and
     columns; with the padding counted in, a last row went under the frame (in the status bar). */
  .term { position: relative; flex: 1; min-height: 0; padding: 8px 4px 4px 10px; box-sizing: content-box; background: var(--surface); }
  .tmenu { position: absolute; z-index: 10; display: flex; flex-direction: column; min-width: 160px; padding: 4px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-pop); }
  .tmenu button { display: flex; justify-content: space-between; gap: 16px; padding: 4px 8px; border: none; border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .tmenu button:hover:not(:disabled) { background: var(--surface-muted); }
  .tmenu button:disabled { color: var(--text-muted); cursor: default; }
  .tmenu kbd { color: var(--text-muted); font: inherit; font-size: var(--fs-tag); }
  .copymode { position: absolute; top: 8px; right: 12px; z-index: 5; padding: 2px 8px; border: 1px solid var(--warn); border-radius: 999px; background: var(--surface); color: var(--warn); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .copymode:hover { background: var(--surface-muted); }
  .term :global(.xterm) { height: 100%; }
  .term :global(.xterm-viewport) { background: var(--surface) !important; }
  /* xterm 6 draws its own scrollbar (VS Code's): its slider as the app's. */
  .term :global(.xterm .scrollbar > .slider) { background: var(--scroll-thumb) !important; border-radius: 5px; }
</style>
