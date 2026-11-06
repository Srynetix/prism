<script lang="ts">
  import { t, tc } from '../i18n.svelte'
  import { keys } from '../platform'
  import { onGithubWeb } from '../forge'
  import { projectOf } from '../emoji.svelte'
  import EmojiMenu from './EmojiMenu.svelte'
  import MentionMenu from './MentionMenu.svelte'
  import { mentionsIn, type Mention, type MentionSource } from '../mentions.svelte'

  /**
   * Comment input (GitLab, ClickUp): ⌘↵ sends, Escape cancels. `value`: the initial
   * text (editing a comment); `accent`: the source's color. `starters`: sentence starts offered
   * as chips, inserted at the start of the text. `forge`: the project's web address, for a comment
   * on its forge, whose emoji shortcodes are offered as `:thu` is typed (EmojiMenu). `people`: who
   * can be mentioned, offered as `@ann` is typed (MentionMenu); `mentions`: those the text being
   * edited already has (a tracker's), kept as long as their token is in it.
   */
  let {
    placeholder = keys(t('Leave a comment… (⌘↵ to send, Markdown)')), label = tc('action', 'Comment'), autofocus = false, value = '', accent = 'var(--src-gitlab)', starters = [], forge, people, mentions = [], onsend, oncancel,
  }: {
    placeholder?: string
    label?: string
    autofocus?: boolean
    value?: string
    accent?: string
    starters?: string[]
    forge?: string
    people?: MentionSource
    mentions?: Mention[]
    /** `mentions`: a tracker's, those still in the text. */
    onsend: (body: string, mentions: Mention[]) => Promise<void>
    oncancel?: () => void
  } = $props()

  // svelte-ignore state_referenced_locally
  let text = $state(value)
  let busy = $state(false)
  let error = $state<string | null>(null)
  let field = $state<HTMLTextAreaElement>()
  let menu = $state<ReturnType<typeof EmojiMenu>>()
  let mentionMenu = $state<ReturnType<typeof MentionMenu>>()
  /** The people picked in the `@` list (a tracker's), and those of the text being edited. */
  // svelte-ignore state_referenced_locally
  let picked = $state<Mention[]>([...mentions])
  /** Where the caret is in `text`, for the emoji menu. */
  let caret = $state(0)
  const track = () => (caret = field?.selectionStart ?? text.length)

  async function send() {
    const body = text.trim()
    if (!body || busy) return
    busy = true
    error = null
    try {
      await onsend(body, mentionsIn(body, picked))
      text = ''
      picked = []
      oncancel?.()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (menu?.keydown(e) || mentionMenu?.keydown(e)) return
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      send()
    } else if (e.key === 'Escape' && oncancel) {
      // Before the drawer: Escape closes the reply, not the drawer.
      e.stopPropagation()
      oncancel()
    }
  }
  const focus = (el: HTMLTextAreaElement) => {
    if (autofocus) el.focus()
  }
</script>

<div class="box" style:--accent-c={accent}>
  <div class="field">
    <textarea bind:this={field} bind:value={text} {onkeydown} oninput={track} onkeyup={track} onclick={track} {placeholder} rows="3" disabled={busy} use:focus onfocus={() => people?.load()}></textarea>
    {#if forge}<EmojiMenu bind:this={menu} bind:text bind:caret {field} github={onGithubWeb(forge)} project={projectOf(forge)} />{/if}
    {#if people}<MentionMenu bind:this={mentionMenu} bind:text bind:caret {field} source={people} onpick={(p, token) => { if (people.rich) picked = [...picked, { id: p.id, text: token }] }} />{/if}
  </div>
  <div class="bar">
    {#each starters as s (s)}<button class="starter" onclick={() => (text = text.trim() ? `${s} ${text}` : `${s} `)} disabled={busy} title={t('Start with “{s}”', { s })}>{s}</button>{/each}
    {#if error}<span class="err" title={error}>{error}</span>{/if}
    <span class="grow"></span>
    {#if oncancel}<button class="btn" onclick={oncancel} disabled={busy}>{t('Cancel')}</button>{/if}
    <button class="btn primary" onclick={send} disabled={busy || !text.trim()}>{busy ? t('Sending…') : label}</button>
  </div>
</div>

<style>
  .box { display: flex; flex-direction: column; gap: 6px; }
  .field { position: relative; display: flex; flex-direction: column; }
  textarea { font: inherit; font-size: var(--fs-body); padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); resize: vertical; min-height: 56px; }
  textarea:focus { outline: none; border-color: var(--accent-c); }
  .bar { display: flex; align-items: center; gap: 6px; }
  .grow { flex: 1; }
  .err { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--ko); font-size: var(--fs-meta); }
  .btn { padding: 1px 10px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .btn:disabled { opacity: 0.55; cursor: default; }
  .starter { padding: 0 7px; border: 1px dashed var(--border); border-radius: var(--r9); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .starter:hover { border-style: solid; border-color: var(--accent-c); color: var(--accent-c); }
  .btn.primary { border-color: transparent; background: var(--accent-c); color: #fff; font-weight: 500; }
</style>
