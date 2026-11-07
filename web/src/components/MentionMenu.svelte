<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { mentionMatches, type Mentionable, type MentionSource } from '../mentions.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The people offered while a comment's word before the caret is `@` and what follows it (`@ann`,
   * or `@` alone), as the forge's own box does; the same keys as the emoji list (EmojiMenu): ↑↓ pick,
   * Tab or Enter put the person's token in its place, Esc puts the list away; a click as well. The
   * two lists never open together: the word decides (`:thu` or `@ann`). `onpick`: told of each
   * person picked, with the token put in the text (a tracker's mentions are sent with the comment).
   */
  let {
    text = $bindable(), caret = $bindable(), field, source, onpick,
  }: { text: string; caret: number; field?: HTMLTextAreaElement; source: MentionSource; onpick?: (p: Mentionable, token: string) => void } = $props()

  let sel = $state(0)
  /** Put away by Esc, for this word at this place. */
  let dismissed = $state<string | null>(null)
  let list = $state<HTMLElement>()

  /** The word typed, from its `@` to the caret. */
  const word = $derived.by(() => {
    const m = /(?:^|[\s([])(@[\p{L}\p{N}_.-]*)$/u.exec(text.slice(0, caret))
    return m ? { start: caret - m[1].length, query: m[1].slice(1) } : null
  })
  const key = $derived(word ? `${word.start}:${word.query}` : null)
  const people = $derived(word ? source.people() : null)
  const matches = $derived(word && people ? mentionMatches(source, word.query) : [])
  /** Still being read: said, rather than an empty list. */
  const loading = $derived(!!word && people === null && !source.error())
  const open = $derived((matches.length > 0 || loading) && dismissed !== key)
  $effect(() => {
    if (word) untrack(() => source.load())
  })
  $effect(() => {
    void key
    sel = 0
  })
  $effect(() => {
    list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' })
  })

  function pick(p: Mentionable) {
    if (!word) return
    const token = source.token(p)
    const insert = `${token} `
    // A space already after the word: not a second one.
    const rest = text.slice(caret).replace(/^ /, '')
    text = text.slice(0, word.start) + insert + rest
    const at = word.start + insert.length
    caret = at
    onpick?.(p, token)
    field?.focus()
    void tick().then(() => field?.setSelectionRange(at, at))
  }

  export function keydown(e: KeyboardEvent): boolean {
    if (!open || !matches.length || e.isComposing || e.metaKey || e.ctrlKey) return false
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const n = matches.length
      sel = (sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n
    } else if ((e.key === 'Tab' && !e.shiftKey) || (e.key === 'Enter' && !e.shiftKey)) {
      pick(matches[sel])
    } else if (e.key === 'Escape') {
      dismissed = key
    } else {
      return false
    }
    e.preventDefault()
    e.stopPropagation()
    return true
  }
</script>

{#if open}
  <div class="people" role="listbox" aria-label={t('People to mention')} bind:this={list}>
    {#each matches as p, i (p.key)}
      {@const pic = source.picture(p)}
      {@const thread = source.first.includes(p.key)}
      {#if i > 0 && !thread && source.first.includes(matches[i - 1].key)}<hr />{/if}
      <button type="button" role="option" aria-selected={i === sel} class:on={i === sel} onmousedown={(e) => e.preventDefault()} onclick={() => pick(p)} onmouseenter={() => (sel = i)}>
        <span class="av" class:pic={!!pic} style:--av={p.color}>{#if pic}<img src={pic} alt="" />{:else}{p.initials}{/if}</span>
        <span class="name">{p.name}</span>
        {#if p.handle}<span class="handle">@{p.handle}</span>{/if}
        {#if thread}<span class="here">{t('in this thread')}</span>{/if}
      </button>
    {:else}
      <p class="muted">{t('Loading the members…')}</p>
    {/each}
  </div>
{/if}

<style>
  .people { position: absolute; left: 0; right: 0; bottom: 100%; z-index: 20; max-height: 240px; overflow-y: auto; margin-bottom: 4px; padding: 4px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); box-shadow: 0 6px 20px rgb(0 0 0 / 0.18); }
  button { display: flex; align-items: center; gap: 8px; width: 100%; padding: 3px 8px; border: none; border-radius: 6px; background: none; color: var(--text); font: inherit; font-size: 13px; text-align: left; cursor: pointer; }
  button.on { background: var(--surface-muted); }
  .av { flex-shrink: 0; display: inline-grid; place-items: center; width: 20px; height: 20px; overflow: hidden; border-radius: 50%; background: var(--av, var(--text-muted)); color: #fff; font-size: 9px; font-weight: 600; }
  .av.pic { background: none; }
  .av img { width: 100%; height: 100%; object-fit: cover; }
  .name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .handle { font-family: var(--mono); font-size: 12px; color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .here { margin-left: auto; flex-shrink: 0; font-size: 11px; color: var(--text-muted); }
  hr { margin: 3px 4px; border: none; border-top: 1px solid var(--border); }
  .muted { margin: 0; padding: 3px 8px; color: var(--text-muted); font-size: 12px; }
</style>
