<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import Markdown from '../Markdown.svelte'
  import { passages, quoteOf, spanOf } from '../../comments'
  import { addComment, commentsOf, editComment, removeComment } from '../../comments.svelte'
  import DictateButton from './DictateButton.svelte'
  import DictationBar from './DictationBar.svelte'
  import { filesOfAgent } from '../../dictation.svelte'

  /**
   * An agent's text (its plan, a long answer) cut into passages (comments.ts), each commented on:
   * a + in its margin on hover (always shown on a phone) opens a box under it — shift-click another
   * passage's +, or drag from one + to another: the comment is on all of them —, Enter adds the
   * comment (⇧Enter: a line), Esc cancels. A commented passage is lit, numbered, its comments under
   * it, a draft edited (✎) or removed (×) until sent. `editable` off: read only, the comments shown.
   */
  let { id, target, text, editable, reading = false }: {
    id: string
    /** Its comments' target: `plan`, `answer:<hash>` (comments.svelte.ts). */
    target: string
    text: string
    editable: boolean
    reading?: boolean
  } = $props()

  const ps = $derived(passages(text))
  const shown = $derived(commentsOf(id, target).filter((c) => c.status !== 'addressed'))
  /** Numbered in the text's order. */
  const numbered = $derived.by(() => {
    const order = new Map(ps.map((p) => [p.source, p.at]))
    return [...shown].sort((a, b) => (order.get(a.quote) ?? 1e9) - (order.get(b.quote) ?? 1e9))
  })
  const numberOf = (c: string) => numbered.findIndex((x) => x.id === c) + 1
  /** Each comment's passages, first and last. */
  const spans = $derived(new Map(shown.map((c) => [c.id, spanOf(ps, c)])))
  /** Comments whose passages are not in this text any more: shown at its end. */
  const astray = $derived(shown.filter((c) => !spans.get(c.id)))

  /** The passages whose box is open (the first clicked, and the one it goes to), or the comment edited. */
  let writing = $state<{ a: number; b: number } | null>(null)
  let editing = $state<string | null>(null)
  let body = $state('')
  let bodyField = $state<HTMLTextAreaElement>()
  const range = $derived(writing ? ([Math.min(writing.a, writing.b), Math.max(writing.a, writing.b)] as const) : null)

  /** Its box under a passage; `extend`, while one is open: up to this passage. */
  function open(at: number, extend = false) {
    if (extend && writing) return void (writing = { ...writing, b: at })
    editing = null
    writing = { a: at, b: at }
    body = ''
  }
  /** Dragging from a passage's + to another's: the passages it goes over join the comment. */
  let dragging = false
  /** Opened on the press (a mouse): its click that follows does nothing more. */
  let pressed = false
  function press(e: PointerEvent, at: number) {
    if (e.button !== 0 || e.pointerType === 'touch') return
    e.preventDefault()
    open(at, e.shiftKey)
    dragging = true
    pressed = true
  }
  function click(e: MouseEvent, at: number) {
    if (pressed) return void (pressed = false)
    open(at, e.shiftKey)
  }
  let root: HTMLElement | undefined = $state()
  function dragOver(e: PointerEvent) {
    if (!dragging || !writing) return
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-at]')
    if (el && root?.contains(el)) writing = { ...writing, b: Number(el.dataset.at) }
  }
  function edit(c: { id: string; body: string }) {
    writing = null
    editing = c.id
    body = c.body
  }
  function save() {
    const b = body.trim()
    if (b) {
      if (editing) editComment(id, target, editing, b)
      else if (range) addComment(id, target, ps[range[0]].source, b, ps[range[1]].source)
    }
    cancel()
  }
  function cancel() {
    writing = null
    editing = null
    body = ''
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      save()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      cancel()
    }
  }
  const focus = (el: HTMLTextAreaElement) => el.focus()
</script>

{#snippet box()}
  {@const several = !editing && !!range && range[1] > range[0]}
  <div class="box">
    <textarea bind:this={bodyField} bind:value={body} use:focus onkeydown={onkeydown} rows="2" writingsuggestions="false" placeholder={several ? t('Your comment on these {n} passages…', { n: range![1] - range![0] + 1 }) : t('Your comment on this passage…')}></textarea>
    <DictationBar key="comment:{id}:{target}" />
    <div class="row"><DictateButton key="comment:{id}:{target}" field={() => bodyField} text={() => body} write={(v) => (body = v)} hints={() => filesOfAgent(id)} size={13} /><small>{editing ? t('Enter adds · ⇧Enter a line · Esc cancels') : t('Enter adds · ⇧Enter a line · Esc cancels · ⇧-click another + for several passages')}</small><span class="grow"></span><button class="b" onclick={cancel}>{t('Cancel')}</button><button class="b primary" onclick={save} disabled={!body.trim()}>{editing ? t('Save') : t('Add')}</button></div>
  </div>
{/snippet}

{#snippet comment(c: (typeof shown)[number])}
  {#if editing === c.id}
    {@render box()}
  {:else}
    <div class="cmt" class:sent={c.status === 'sent'}>
      <span class="n">{numberOf(c.id)}</span>
      <span class="body">{c.body}</span>
      {#if c.status === 'sent'}<small class="st">sent</small>{/if}
      {#if editable && c.status === 'draft'}
        <button class="ic" onclick={() => edit(c)} title={t('Edit')}><Icon name="edit" size={11} /></button>
        <button class="ic" onclick={() => removeComment(id, target, c.id)} title={t('Remove')}><Icon name="close" size={11} /></button>
      {/if}
    </div>
  {/if}
{/snippet}

<svelte:window onpointermove={dragOver} onpointerup={() => ((dragging = false), setTimeout(() => (pressed = false)))} />

<div class="ct" class:editable bind:this={root}>
  {#each ps as p, i (p.at)}
    <!-- The comments starting here (their number on the +), those on it, and those ending here (shown under it). -->
    {@const starting = shown.filter((c) => spans.get(c.id)?.[0] === i)}
    {@const on = shown.filter((c) => { const s = spans.get(c.id); return !!s && s[0] <= i && i <= s[1] })}
    {@const ending = shown.filter((c) => spans.get(c.id)?.[1] === i)}
    {@const picked = !!range && range[0] <= i && i <= range[1]}
    <div class="ps" class:has={on.length > 0} class:picked class:cont={on.length > 0 && !starting.length} data-at={i}>
      {#if editable}<button class="add" onpointerdown={(e) => press(e, i)} onclick={(e) => click(e, i)} title={writing ? t('Comment on this passage (shift-click: up to it)') : t('Comment on this passage')} aria-label={t('Comment on this passage')}>{#if starting.length}{numberOf(starting[0].id)}{:else}+{/if}</button>{/if}
      <Markdown text={p.source} parsed={[p.block]} {reading} mrs />
    </div>
    {#each ending as c (c.id)}{@render comment(c)}{/each}
    {#if range && range[1] === i}{@render box()}{/if}
  {/each}
  {#if astray.length}
    <div class="astray">
      {#each astray as c (c.id)}<small>{c.end ? t('From “{from}” to “{to}”', { from: quoteOf(c.quote, 40), to: quoteOf(c.end, 40) }) : t('On “{quote}”', { quote: quoteOf(c.quote, 60) })}</small>{@render comment(c)}{/each}
    </div>
  {/if}
</div>

<style>
  .ct { display: flex; flex-direction: column; gap: 2px; }
  .ct.editable { padding-left: 24px; }
  .ps { position: relative; border-radius: 5px; padding: 1px 6px; margin: 0 -6px; }
  .ps :global(p), .ps :global(ul), .ps :global(h1), .ps :global(h2), .ps :global(h3), .ps :global(h4), .ps :global(pre) { margin-top: 2px; margin-bottom: 2px; }
  .editable .ps:hover { background: var(--surface-muted); }
  .ps.has { background: color-mix(in srgb, var(--warn) 12%, transparent); box-shadow: inset 2px 0 0 var(--warn); }
  .add { position: absolute; left: -24px; top: 3px; width: 18px; height: 18px; padding: 0; border-radius: 50%; border: 1px solid var(--border); background: var(--surface); color: var(--text-muted); font: 600 12px/16px var(--sans, inherit); text-align: center; cursor: pointer; opacity: 0; }
  .ps:hover .add, .add:focus-visible { opacity: 1; }
  .ps.picked { background: color-mix(in srgb, var(--accent) 10%, transparent); box-shadow: inset 2px 0 0 var(--accent); }
  .ps.has .add { opacity: 1; background: var(--warn); border-color: var(--warn); color: var(--surface); font-size: 10px; }
  /* Inside a comment's run of passages, past its first: the lit band goes on, no number again. */
  .ps.cont .add { opacity: 0; background: var(--surface); border-color: var(--border); color: var(--text-muted); font-size: 12px; }
  .ps.cont:hover .add, .ps.cont .add:focus-visible { opacity: 1; }
  /* No hover on a phone: the + stays, faint. */
  @media (hover: none) { .add { opacity: 0.45; } }
  .cmt { display: flex; align-items: flex-start; gap: 6px; margin: 2px 0 6px; padding: 5px 8px; border: 1px solid color-mix(in srgb, var(--warn) 45%, var(--border)); border-radius: 8px; background: var(--surface); font-size: 12.5px; }
  .cmt.sent { opacity: 0.7; border-color: var(--border); }
  .cmt .n { flex: none; min-width: 16px; height: 16px; border-radius: 8px; background: var(--warn); color: var(--surface); font: 700 10px/16px var(--sans, inherit); text-align: center; }
  .cmt .body { flex: 1; min-width: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .cmt .st { flex: none; color: var(--text-muted); }
  .ic { flex: none; display: grid; place-items: center; width: 20px; height: 18px; padding: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .ic:hover { color: var(--text); }
  .box { display: flex; flex-direction: column; gap: 5px; margin: 2px 0 6px; padding: 6px 8px; border: 1px solid var(--warn); border-radius: 8px; background: var(--surface); }
  .box textarea { width: 100%; resize: vertical; min-height: 40px; border: 1px solid var(--border); border-radius: 6px; padding: 5px 7px; background: var(--bg, var(--surface-muted)); color: var(--text); font: inherit; font-size: 13px; }
  .row { display: flex; align-items: center; gap: 6px; }
  .row small { color: var(--text-muted); font-size: 11px; }
  .grow { flex: 1; }
  .b { height: 24px; padding: 0 9px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--text); font: inherit; font-size: 12px; cursor: pointer; }
  .b.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
  .b:disabled { opacity: 0.5; cursor: default; }
  .astray { margin-top: 6px; display: flex; flex-direction: column; }
  .astray small { color: var(--text-muted); font-size: 11px; }
  :global(html.remote) .box textarea { font-size: 16px; }
</style>
