<script lang="ts">
  import { currentZoom } from '../zoom.svelte'
  import { tick } from 'svelte'
  import Icon from './Icon.svelte'
  import { invoke, isDesktop } from '../bridge'
  import { attachFiles, openLauncher, webOrigin } from '../agents.svelte'
  import { sendPrompt } from '../agent-inbox.svelte'
  import { nameOf } from '../agent-cli'
  import { pref, setPref } from '../layout.svelte'
  import { keys } from '../platform'
  import { holdEscape } from '../layers.svelte'
  import { toast, toastError } from '../toast.svelte'
  import type { WebOrigin } from '../bindings/WebOrigin'
  import { annotateType, captures, elementLabel, noteId, pinsOf, readFound, readPick, setWebDraft, webDraft, webPrompt, type Pick, type WebNote } from '../web-comments.svelte'
  import type { Snippet } from 'svelte'
  import type { Agent } from '../types'
  import { t, tn } from '../i18n.svelte'

  /**
   * Comment, in an agent's Web view: the page framed by `frame` (served by Prism, its annotation
   * script inside) outlines what the pointer is over and picks what is clicked; a remark is
   * written in a bubble under it, pinned on the page, listed here, and sent with the others as one
   * prompt (`webPrompt`). `on`: the Comment mode (the page not pressed meanwhile). On a phone
   * (`phone`), the bubble is a sheet from the bottom and nothing is captured.
   */
  let {
    agent, page, frame, on = $bindable(false), phone = false, onsend, children,
  }: {
    agent: Agent
    page: string
    frame: HTMLIFrameElement | undefined
    on: boolean
    phone?: boolean
    /** The remarks about to go: the view notes the page's date, to reload it once the agent rewrote it. */
    onsend?: () => Promise<void> | void
    /** The frame, beside the remarks. */
    children: Snippet
  } = $props()

  const draft = $derived(webDraft(agent.id, page))
  const count = $derived(draft.notes.length + (draft.whole.trim() ? 1 : 0))
  /** The element just clicked, its remark being written; or the remark being edited (from 1). */
  let pick = $state<Pick | null>(null)
  let editing = $state<number | null>(null)
  let text = $state('')
  let box = $state<HTMLTextAreaElement>()
  /** The remark pointed at in the list: its pin stands out. */
  let hl = $state<number | null>(null)
  /** The page's script answered; it did not within 3 s (its own policy refused it). */
  let ready = $state(false)
  let refused = $state(false)
  /** The remarks whose element the page found again (null: not said yet). */
  let found = $state<Set<number> | null>(null)
  // The view is made for a phone or not once (its prop does not change).
  // svelte-ignore state_referenced_locally
  let capture = $state(!phone && pref<boolean>('agents.web.capture', true))
  let showPrompt = $state(false)
  let sending = $state(false)
  /** The picture of the element just clicked, being taken. */
  let shot: Promise<string | null> | null = null

  const tell = (m: Record<string, unknown>) => frame?.contentWindow?.postMessage({ prism: 'annotate', ...m }, '*')
  const save = (whole: string, notes: WebNote[]) => setWebDraft(agent.id, page, { whole, notes })

  // A new frame (the page loaded again): its script says it is there, or Comment says it cannot.
  $effect(() => {
    if (!frame) return
    ready = false
    refused = false
    found = null
    const timer = setTimeout(() => (refused = !ready), 3000)
    return () => clearTimeout(timer)
  })
  // What the page draws: the mode and the pins, again whenever they change, and to each new
  // document of the frame (a link followed, a reload: its script says ready again).
  const sync = () => {
    tell({ type: 'mode', on })
    tell({ type: 'pins', pins: $state.snapshot(pinsOf(draft.notes, hl)) })
  }
  $effect(() => {
    if (ready) sync()
  })
  $effect(() => {
    if (!on) close()
  })
  // Esc in Prism (not in a field) leaves the mode; in the page, its script says it (`escape`).
  $effect(() => {
    if (on) return holdEscape(() => (on = false), (e) => !(e.target as HTMLElement | null)?.closest?.('input, textarea, select'))
  })

  // Only messages from this frame, only the fields a remark needs (`readPick`, `readFound`).
  $effect(() => {
    const heard = (e: MessageEvent) => {
      if (!frame || e.source !== frame.contentWindow) return
      const type = annotateType(e.data)
      if (type === 'ready') {
        ready = true
        refused = false
        sync()
      } else if (type === 'pick' && on) {
        const p = readPick(e.data)
        if (!p) return
        pick = p
        // Its picture now, where it was clicked (it may scroll while the remark is written).
        shot = capture && isDesktop() ? shoot(p) : null
        editing = null
        text = ''
        void tick().then(() => box?.focus())
      } else if (type === 'found') {
        found = readFound(e.data)
      } else if (type === 'escape') {
        if (pick || editing) close()
        else on = false
      }
    }
    window.addEventListener('message', heard)
    return () => window.removeEventListener('message', heard)
  })

  function close() {
    pick = null
    editing = null
    text = ''
    if (ready) tell({ type: 'clear' })
  }
  function edit(i: number) {
    pick = null
    editing = i
    text = draft.notes[i - 1]?.note ?? ''
    void tick().then(() => box?.focus())
  }
  async function add() {
    const v = text.trim()
    if (!v) return close()
    if (editing) {
      save(draft.whole, draft.notes.map((n, k) => (k + 1 === editing ? { ...n, note: v } : n)))
      return close()
    }
    const p = pick
    if (!p) return close()
    const id = noteId()
    const taken = shot
    save(draft.whole, [...draft.notes, { id, sel: p.sel, tag: p.tag, text: p.text, note: v }])
    close()
    const b64 = await taken
    if (b64) captures.set(id, b64)
  }
  /**
   * The element's picture, from the window, within the frame (its outline is drawn around it,
   * outside): kept in memory for the prompt. Failing, the remark goes without it.
   */
  async function shoot(p: Pick): Promise<string | null> {
    const f = frame?.getBoundingClientRect()
    if (!f) return null
    const x0 = Math.max(f.left, f.left + p.rect.x)
    const y0 = Math.max(f.top, f.top + p.rect.y)
    const x1 = Math.min(f.right, f.left + p.rect.x + p.rect.w)
    const y1 = Math.min(f.bottom, f.top + p.rect.y + p.rect.h)
    if (x1 - x0 < 2 || y1 - y0 < 2) return null
    try {
      // The page's pixels are the window's points divided by its zoom (zoom.svelte.ts).
      const z = currentZoom()
      return await invoke<string>('web_capture', { rect: [x0 * z, y0 * z, (x1 - x0) * z, (y1 - y0) * z] })
    } catch (e) {
      toast(t('No picture of the element: {error}', { error: String(e) }), 4000, null, 'info')
      return null
    }
  }
  function remove(i: number) {
    const n = draft.notes[i - 1]
    if (n) captures.delete(n.id)
    save(draft.whole, draft.notes.filter((_, k) => k + 1 !== i))
    if (editing === i) close()
  }
  function setCapture(v: boolean) {
    capture = v
    setPref('agents.web.capture', v)
  }
  const running = $derived(agent.state === 'running')
  /** An imported page's origin, said in the prompt (where to put the new version back). */
  let origin = $state<WebOrigin | null>(null)
  $effect(() => {
    const p = page
    origin = null
    let gone = false
    void webOrigin(agent.id, p).then((o) => !gone && (origin = o))
    return () => (gone = true)
  })
  const preview = $derived(webPrompt(page, draft, {}, undefined, origin))

  /** The drafts gone, once the agent has them. */
  function clear() {
    for (const n of draft.notes) captures.delete(n.id)
    save('', [])
    on = false
    showPrompt = false
  }
  /**
   * Sends the remarks as one prompt (its pictures attached), queued while the agent shows a menu;
   * an agent stopped: the launcher, prefilled in its folder, the page named by its place on disk
   * (a new agent's scratchpad is another), the drafts kept until it is launched.
   */
  async function send() {
    if (!count || sending) return
    sending = true
    try {
      const shots: Record<number, string> = {}
      if (capture) {
        for (const [k, n] of draft.notes.entries()) {
          const b64 = captures.get(n.id)
          if (!b64) continue
          const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))
          const [path] = await attachFiles([new File([bytes], `element-${k + 1}.png`, { type: 'image/png' })], agent.id)
          if (path) shots[k + 1] = path
        }
      }
      // Read now, not from the view: sent at once after opening the page, it is said all the same.
      const from = origin ?? (await webOrigin(agent.id, page))
      if (!running) {
        const where = await invoke<string>('agent_page_file', { id: agent.id, path: page }).catch(() => undefined)
        openLauncher({ title: t('Remarks on {page}', { page: page.split('/').pop() ?? page }), prompt: webPrompt(page, draft, shots, where, from), cwd: agent.worktree ?? agent.cwd }, null, async () => clear())
        return
      }
      await onsend?.()
      const r = await sendPrompt(agent.id, webPrompt(page, draft, shots, undefined, from))
      if (r === 'already-queued') return toast(t('These remarks already wait for {agent}', { agent: nameOf(agent) }), 3000, null, 'info')
      toast(r === 'queued' ? t('Remarks queued: {agent} reads them once its menu is answered', { agent: nameOf(agent) }) : r === 'already-sent' ? t('Already sent to {agent}', { agent: nameOf(agent) }) : tn(count, '{n} remark sent to {agent}', '{n} remarks sent to {agent}', { agent: nameOf(agent) }))
      clear()
    } catch (e) {
      toastError(String(e))
    } finally {
      sending = false
    }
  }

  /** Where the bubble goes: under the element, above it when there is no room, inside the frame. */
  const at = $derived.by(() => {
    const f = frame?.getBoundingClientRect()
    const target = pick?.rect
    if (!f || !target) return null
    const left = Math.max(f.left + 8, Math.min(f.left + target.x, f.right - 308))
    let top = f.top + target.y + target.h + 8
    if (top + 170 > f.bottom) top = Math.max(f.top + 8, f.top + target.y - 178)
    return { left, top }
  })
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      close()
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      void add()
    }
  }
</script>

{#if on && refused && !ready}
  <p class="hint warn">{t('This page does not take Prism’s script (its own security policy): Comment is not available on it.')}</p>
{:else if on}
  <p class="hint"><Icon name="edit" size={11} />{phone ? t('Tap an element of the page to comment on it; the page is not pressed meanwhile.') : keys(t('Click an element of the page to comment on it (⌥-click: the one around it); the page is not pressed meanwhile.'))}</p>
{/if}

<div class="stage" class:phone>
{@render children()}
{#if on || count}
  <aside class="notes" class:phone aria-label={t('Remarks on the page')}>
    <header>
      <b>{t('Remarks')}</b>{#if count}<span class="n">{count}</span>{/if}
      <span class="grow"></span>
      {#if !on}<button class="btn sm" onclick={() => (on = true)}>{t('Comment')}</button>{/if}
    </header>
    <div class="items">
      <label class="whole">
        <span class="muted">{t('The whole page')}</span>
        <textarea rows="1" value={draft.whole} oninput={(e) => save(e.currentTarget.value, draft.notes)} placeholder={t('A remark on the page as a whole (optional)')}></textarea>
      </label>
      {#each draft.notes as n, k (k)}
        {@const i = k + 1}
        {@const gone = found !== null && !found.has(i)}
        <div class="it" class:hl={hl === i} class:gone role="listitem" onmouseenter={() => (hl = i)} onmouseleave={() => (hl = null)}>
          <span class="pin">{i}</span>
          <div class="body">
            <code class="el" title={n.sel}>{elementLabel(n)}</code>
            {#if editing === i && !phone}
              <textarea bind:this={box} bind:value={text} onkeydown={onKey} rows="3"></textarea>
              <div class="row"><span class="grow"></span><button class="btn sm" onclick={close}>{t('Cancel')}</button><button class="btn sm primary claude" onclick={add} disabled={!text.trim()}>{t('Save')}</button></div>
            {:else}
              <p>{n.note}</p>
            {/if}
            {#if gone}<small class="warn">{t('Not found in the page as it is now: the page changed.')}</small>{/if}
          </div>
          <span class="acts">
            <button class="btn sm icon ghost" onclick={() => edit(i)} title={t('Edit')} aria-label={t('Edit')}><Icon name="edit" /></button>
            <button class="btn sm icon ghost" onclick={() => remove(i)} title={t('Remove')} aria-label={t('Remove')}><Icon name="close" /></button>
          </span>
        </div>
      {:else}
        <p class="none">{t('No remark on an element yet: click one in the page.')}</p>
      {/each}
    </div>
    {#if count}
      <footer>
        {#if !phone}<label class="check"><input type="checkbox" checked={capture} onchange={(e) => setCapture(e.currentTarget.checked)} />{t('Attach a picture of each element')}</label>{/if}
        {#if showPrompt}<pre class="prompt">{preview}</pre>{/if}
        <div class="row">
          <button class="btn sm" class:on={showPrompt} onclick={() => (showPrompt = !showPrompt)}>{t('Preview')}</button>
          <button class="btn sm primary claude grow" class:busy={sending} onclick={send} disabled={sending} title={running ? t('Sent as one prompt; queued while {agent} shows a menu', { agent: nameOf(agent) }) : t('{agent} has stopped: a new agent in its folder takes them', { agent: nameOf(agent) })}>
            {#if sending}<span class="spinner"></span>{t('Sending…')}{:else}<Icon name="send" />{running ? t('Send to {agent}', { agent: nameOf(agent) }) : t('Launch an agent with them')}{/if}
          </button>
        </div>
        <small class="muted">{t('Drafts kept for this page and this agent until sent.')}</small>
      </footer>
    {/if}
  </aside>
{/if}
</div>

{#if (pick && (at || phone)) || (editing && phone)}
  {@const target = pick ?? draft.notes[(editing ?? 1) - 1]}
  <div class="pop" class:sheet={phone} style={!phone && at ? `left:${at.left}px;top:${at.top}px` : ''} role="dialog" aria-label={t('A remark on {element}', { element: elementLabel(target) })}>
    <code class="el" title={target.sel}>{elementLabel(target)}</code>
    <textarea bind:this={box} bind:value={text} onkeydown={onKey} rows="3" placeholder={t('Your remark on this element…')}></textarea>
    <div class="row">
      {#if !phone}<small class="muted">{keys('⌘↵')} · Esc</small>{/if}
      <span class="grow"></span>
      <button class="btn sm" onclick={close}>{t('Cancel')}</button>
      <button class="btn sm primary claude" onclick={add} disabled={!text.trim()}>{editing ? t('Save') : t('Add')}</button>
    </div>
  </div>
{/if}

<style>
  .hint { display: flex; align-items: center; gap: 6px; margin: 0; padding: 4px 10px; border-bottom: 1px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); background: color-mix(in srgb, var(--src-claude) 8%, var(--surface)); }
  .hint.warn { color: var(--text); background: color-mix(in srgb, var(--warn) 12%, var(--surface)); }
  .stage { flex: 1; min-height: 0; display: flex; }
  .stage.phone { flex-direction: column; }
  .notes { flex: 0 0 280px; display: flex; flex-direction: column; min-height: 0; border-left: 1px solid var(--border); background: var(--surface); }
  .notes.phone { flex: 0 0 auto; max-height: 45%; border-left: none; border-top: 1px solid var(--border); }
  header { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
  .grow { flex: 1; }
  .n { min-width: 16px; height: 16px; padding: 0 4px; border-radius: 999px; background: var(--src-claude); color: var(--on-accent, #fff); font-size: var(--fs-micro); font-weight: 700; display: inline-flex; align-items: center; justify-content: center; }
  .items { flex: 1; min-height: 0; overflow-y: auto; padding: 6px; display: flex; flex-direction: column; gap: 2px; }
  .whole { display: flex; flex-direction: column; gap: 3px; padding: 4px 4px 8px; font-size: var(--fs-meta); }
  textarea { width: 100%; min-height: 26px; padding: 5px 7px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; resize: vertical; outline: none; }
  textarea:focus { border-color: var(--src-claude); }
  .it { display: flex; gap: 7px; padding: 6px; border: 1px solid transparent; border-radius: var(--r6); }
  .it:hover, .it.hl { background: var(--surface-muted); border-color: var(--border); }
  .pin { flex: none; width: 18px; height: 18px; border-radius: 50%; background: #a78bfa; color: #17141f; font-size: var(--fs-micro); font-weight: 700; display: flex; align-items: center; justify-content: center; }
  .it.gone .pin { background: var(--warn); }
  .body { flex: 1; min-width: 0; }
  .body p { margin: 3px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .el { display: inline-block; max-width: 100%; padding: 0 5px; border-radius: var(--r5); background: var(--surface-muted); font: var(--fs-tag) var(--mono); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; vertical-align: bottom; }
  .warn { display: block; margin-top: 3px; color: var(--warn); font-size: var(--fs-meta); }
  .acts { display: flex; align-items: flex-start; opacity: 0; }
  .it:hover .acts, .it:focus-within .acts, .phone .acts { opacity: 1; }
  .none { margin: 8px 6px; color: var(--text-muted); font-size: var(--fs-meta); }
  footer { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; border-top: 1px solid var(--border); }
  .check { display: flex; align-items: center; gap: 6px; font-size: var(--fs-meta); color: var(--text-muted); }
  .row { display: flex; align-items: center; gap: 6px; }
  .row .btn.grow { justify-content: center; }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); }
  .prompt { margin: 0; max-height: 180px; overflow: auto; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); font: var(--fs-meta)/1.45 var(--mono); color: var(--text-muted); white-space: pre-wrap; }
  .pop { position: fixed; z-index: var(--z-top); width: 300px; display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); box-shadow: var(--shadow-pop, 0 10px 30px rgba(0, 0, 0, 0.35)); }
  .pop.sheet { left: 0; right: 0; bottom: 0; width: auto; border-radius: var(--r14) var(--r14) 0 0; padding: 12px 14px 18px; }
</style>
