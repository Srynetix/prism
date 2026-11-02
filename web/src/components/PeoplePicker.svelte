<script lang="ts">
  import { t } from '../i18n.svelte'
  import { forgeName } from '../forge'
  import { onDestroy, onMount, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import { anchored, exclusive } from '../popup'
  import { loadMembers, members, membersOf, setMrPeople, type Member, type PeopleKind } from '../mr-people.svelte'
  import { forgeAvatar } from '../people'
  import { mrKey, type MrRef } from '../mr-ref'

  /**
   * Pick an MR's assignees or reviewers among the project members: one checkbox per
   * person, checked on screen immediately, and the selection sent to GitLab in a single call a
   * moment after the last checkbox (or when the picker closes). Me first, then the
   * chosen people, then the others.
   */
  let {
    mr, kind, current, me, trigger, onclose,
  }: { mr: MrRef; kind: PeopleKind; current: string[]; me: string | null; trigger: () => HTMLElement | null | undefined; onclose: () => void } = $props()

  /**
   * The MR and the list this picker was opened for: a send after it closes (pending choices) goes
   * there, even if the panel has moved on to another MR or the other list meanwhile.
   */
  const target = untrack(() => ({ mr: { ...mr }, kind }))

  let query = $state('')
  let error = $state('')
  /** Checked boxes not yet sent, and a send in progress. */
  let pending = $state(false)
  let saving = $state(false)
  let saved = $state(false)
  let timer: ReturnType<typeof setTimeout> | undefined
  /** Delay left to check several people before calling GitLab. */
  const SETTLE_MS = 900
  /** The displayed selection follows clicks without waiting for the MR to be re-read, then the MR itself. */
  let chosen = $state<string[]>(untrack(() => [...current]))
  $effect(() => {
    const now = [...current]
    if (!untrack(() => pending || saving)) chosen = now
  })
  let input = $state<HTMLInputElement>()
  let el = $state<HTMLDivElement>()

  /** A click outside closes the picker; on its button, the button decides (toggle). */
  function onOutside(e: PointerEvent) {
    const target = e.target as Node | null
    if (!target?.isConnected || el?.contains(target) || trigger()?.contains(target)) return
    onclose()
  }

  onMount(() => {
    void loadMembers(mr.repo)
    input?.focus()
  })

  const rank = (m: Member) => (m.username === me ? 0 : chosen.includes(m.username) ? 1 : 2)
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase()
    const list = (membersOf(mr.repo) ?? []).filter((m) => !q || m.name.toLowerCase().includes(q) || m.username.toLowerCase().includes(q))
    return list.sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name)).slice(0, 60)
  })
  // A chosen person who is no longer a member (left the project) stays shown, to be removed.
  const strangers = $derived(chosen.filter((u) => { const list = membersOf(mr.repo); return !!list && !list.some((m) => m.username === u) }))

  function toggle(username: string) {
    chosen = chosen.includes(username) ? chosen.filter((u) => u !== username) : [...chosen, username]
    pending = true
    saved = false
    error = ''
    clearTimeout(timer)
    timer = setTimeout(save, SETTLE_MS)
  }

  /** Sends the selection as it is; a box checked during the send goes out afterwards. */
  async function save() {
    clearTimeout(timer)
    if (!pending || saving) return
    pending = false
    saving = true
    try {
      await setMrPeople(target.mr, target.kind, [...chosen])
      saved = !pending
    } catch (e) {
      error = String(e)
    } finally {
      saving = false
      if (pending) timer = setTimeout(save, SETTLE_MS)
    }
  }
  // Closed before the delay: whatever is pending goes out right away.
  onDestroy(() => {
    if (pending) void save()
  })

  const initials = (m: Member) => m.name.split(/\s+/).map((w) => w[0]).join('').slice(0, 2).toUpperCase()
  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation()
      onclose()
    } else if (e.key === 'Enter' && shown[0]) {
      e.preventDefault()
      void toggle(shown[0].username)
    }
  }
</script>

<svelte:window onpointerdown={onOutside} />

<div class="picker" bind:this={el} role="dialog" aria-label={kind === 'assignees' ? t('Assignees') : t('Reviewers')} tabindex="-1" {@attach anchored(trigger, { align: 'start' })} {@attach exclusive(onclose)}>
  <div class="hd">
    <b>{kind === 'assignees' ? t('Assignees') : t('Reviewers')}</b><small>{mrKey(mr)}</small>
    <button class="x" onclick={onclose} aria-label={t('Close')}><Icon name="close" /></button>
  </div>
  <label class="search"><Icon name="search" /><input bind:this={input} bind:value={query} placeholder={t('Search people…')} spellcheck="false" onkeydown={onkey} /></label>
  <div class="list">
    {#if members.error}<p class="err">{members.error}</p>
    {:else if !membersOf(mr.repo)}<p class="muted">{t('Loading the project members…')}</p>
    {:else}
      {#each strangers as u (u)}
        <button class="row on" onclick={() => toggle(u)}><span class="box"><Icon name="check" stroke={3} /></span><span class="av">?</span><span class="nm">{u}<small>{t('no longer a member')}</small></span></button>
      {/each}
      {#each shown as m (m.id)}
        {@const on = chosen.includes(m.username)}
        {@const pic = forgeAvatar(m.avatar_url)}
        <button class="row" class:on onclick={() => toggle(m.username)} title="@{m.username}">
          <span class="box">{#if on}<Icon name="check" stroke={3} />{/if}</span>
          <span class="av" class:pic={!!pic}>{#if pic}<img src={pic} alt="" />{:else}{initials(m)}{/if}</span>
          <span class="nm">{m.name}{m.username === me ? ' (you)' : ''}<small>@{m.username}</small></span>
        </button>
      {:else}
        <p class="muted">{t('Nobody matches “{query}”.', { query })}</p>
      {/each}
    {/if}
  </div>
  {#if error}<p class="err">{t('Not saved: {error}', { error })}</p>
  {:else if pending || saving || saved}<p class="state">{saving ? t('Saving to {forge}…', { forge: forgeName() }) : pending ? t('Saving in a moment…') : t('Saved')}</p>{/if}
</div>

<style>
  .picker { position: fixed; z-index: 60; width: 280px; max-height: 380px; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); overflow: hidden; }
  .hd { display: flex; align-items: center; gap: 6px; padding: 8px 10px 4px; font-size: var(--fs-body); }
  .hd small { color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); }
  .x { margin-left: auto; padding: 2px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .search { display: flex; align-items: center; gap: 6px; margin: 4px 8px 6px; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); color: var(--text-muted); }
  .search input { flex: 1; min-width: 0; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); outline: none; }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 0 4px 6px; }
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 6px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .row:hover:not(:disabled) { background: var(--surface-muted); }
  .row:disabled { cursor: progress; }
  .box { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 15px; height: 15px; border: 1.5px solid var(--text-muted); border-radius: var(--r3); color: var(--surface); font-size: 10px; }
  .row.on .box { border-color: var(--src-gitlab); background: var(--src-gitlab); }
  .av { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 22px; height: 22px; border-radius: var(--r-avatar); background: var(--surface-muted); color: var(--text-muted); font-size: 9px; font-weight: 700; }
  .av.pic { overflow: hidden; }
  .av.pic img { width: 100%; height: 100%; object-fit: cover; }
  .nm { display: flex; flex-direction: column; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .nm small { color: var(--text-muted); font-size: var(--fs-tag); }
  .muted { margin: 6px 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .err { margin: 6px 10px; color: var(--ko); font-size: var(--fs-meta); }
  .state { margin: 0; padding: 5px 10px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-tag); }
</style>
