<script lang="ts">
  import { tick } from 'svelte'
  import { customOf, emojiMatches } from '../emoji.svelte'
  import CustomEmoji from './CustomEmoji.svelte'
  import { t } from '../i18n.svelte'

  /**
   * The emoji offered while a comment's word before the caret is `:` and two characters at least
   * (`:thu`), by the forge's names (`github`: GitHub's, otherwise GitLab's), as the forge's own
   * box does. ↑↓ pick, Tab or Enter put `:name: ` in its place, Esc puts the list away; a click as
   * well. The box hands its keys to `keydown` first, which says whether it took them; `caret`: the
   * caret's position in `text`, which the box keeps. `project`: a GitLab project's
   * (`group/project`), whose groups' custom emoji are offered too.
   */
  let { text = $bindable(), caret = $bindable(), field, github, project = null }: { text: string; caret: number; field?: HTMLTextAreaElement; github: boolean; project?: string | null } = $props()

  let sel = $state(0)
  /** Put away by Esc, for this word at this place. */
  let dismissed = $state<string | null>(null)
  let list = $state<HTMLElement>()

  /** The word typed, from its colon to the caret. */
  const word = $derived.by(() => {
    const m = /(?:^|[\s([])(:[a-z0-9_+-]{2,})$/i.exec(text.slice(0, caret))
    return m ? { start: caret - m[1].length, query: m[1].slice(1) } : null
  })
  const matches = $derived(word ? (emojiMatches(word.query, github, 40, github ? null : customOf(project)) ?? []) : [])
  const key = $derived(word ? `${word.start}:${word.query}` : null)
  const open = $derived(matches.length > 0 && dismissed !== key)
  $effect(() => {
    void key
    sel = 0
  })
  $effect(() => {
    list?.querySelector('.on')?.scrollIntoView({ block: 'nearest' })
  })

  function pick(m: { name: string }) {
    if (!word) return
    const insert = `:${m.name}: `
    // A space already after the word: not a second one.
    const rest = text.slice(caret).replace(/^ /, '')
    text = text.slice(0, word.start) + insert + rest
    const at = word.start + insert.length
    caret = at
    field?.focus()
    void tick().then(() => field?.setSelectionRange(at, at))
  }

  export function keydown(e: KeyboardEvent): boolean {
    if (!open || e.isComposing || e.metaKey || e.ctrlKey) return false
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
  <div class="emoji" role="listbox" aria-label={t('Emoji')} bind:this={list}>
    {#each matches as m, i (m.name)}
      <button type="button" role="option" aria-selected={i === sel} class:on={i === sel} onmousedown={(e) => e.preventDefault()} onclick={() => pick(m)} onmouseenter={() => (sel = i)}>
        <span class="e">{#if m.custom}<CustomEmoji url={m.emoji} name={`:${m.name}:`} />{:else}{m.emoji}{/if}</span><span class="name">:{m.name}:</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .emoji { position: absolute; left: 0; right: 0; bottom: 100%; z-index: 20; max-height: 220px; overflow-y: auto; margin-bottom: 4px; padding: 4px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); box-shadow: 0 6px 20px rgb(0 0 0 / 0.18); }
  button { display: flex; align-items: baseline; gap: 8px; width: 100%; padding: 3px 8px; border: none; border-radius: 6px; background: none; color: var(--text); font: inherit; font-size: 13px; text-align: left; cursor: pointer; }
  button.on { background: var(--surface-muted); }
  .e { width: 1.4em; text-align: center; font-size: 15px; }
  .name { font-family: var(--mono); color: var(--text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button.on .name { color: var(--text); }
</style>
