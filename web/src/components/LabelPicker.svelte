<script lang="ts">
  import { t } from '../i18n.svelte'
  import { forgeName } from '../forge'
  import { onDestroy, onMount, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import { anchored, exclusive } from '../popup'
  import { labelsOf, loadLabels, projectLabels, setMrLabels, validLabel, type ForgeLabel } from '../mr-labels.svelte'
  import { mrKey, type MrRef } from '../mr-ref'

  /**
   * Pick an MR's labels among the project's: one checkbox per label, checked on screen at once,
   * and the selection sent to the forge in a single call a moment after the last checkbox (or when
   * the picker closes). A name the project does not have yet can be added: the forge creates it.
   * The MR's labels first, then the others.
   */
  let { mr, current, trigger, onclose }: { mr: MrRef; current: string[]; trigger: () => HTMLElement | null | undefined; onclose: () => void } = $props()

  /** The MR this picker was opened for: a send after it closes goes there, even if the panel moved on. */
  const target = untrack(() => ({ ...mr }))

  let query = $state('')
  let error = $state('')
  /** Checked boxes not yet sent, and a send in progress. */
  let pending = $state(false)
  let saving = $state(false)
  let saved = $state(false)
  let timer: ReturnType<typeof setTimeout> | undefined
  /** Delay left to check several labels before calling the forge. */
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
    void loadLabels(mr.repo)
    input?.focus()
  })

  const q = $derived(query.trim().toLowerCase())
  const shown = $derived.by(() => {
    const list = (labelsOf(mr.repo) ?? []).filter((l) => !q || l.name.toLowerCase().includes(q) || !!l.description?.toLowerCase().includes(q))
    const rank = (l: ForgeLabel) => (current.includes(l.name) ? 0 : 1)
    return list.sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name)).slice(0, 80)
  })
  // A label of the MR the project's list does not have (a new one, or another group's) stays shown, to be removed.
  const strangers = $derived(chosen.filter((n) => { const list = labelsOf(mr.repo); return !!list && !list.some((l) => l.name === n) && (!q || n.toLowerCase().includes(q)) }))
  /** The name typed, offered as a new label when nothing has it. */
  const fresh = $derived.by(() => {
    const name = query.trim()
    if (!validLabel(name) || !labelsOf(mr.repo)) return null
    const taken = (labelsOf(mr.repo) ?? []).some((l) => l.name.toLowerCase() === name.toLowerCase()) || chosen.some((n) => n.toLowerCase() === name.toLowerCase())
    return taken ? null : name
  })

  function toggle(name: string) {
    chosen = chosen.includes(name) ? chosen.filter((n) => n !== name) : [...chosen, name]
    pending = true
    saved = false
    error = ''
    clearTimeout(timer)
    timer = setTimeout(save, SETTLE_MS)
  }

  function create(name: string) {
    toggle(name)
    query = ''
    input?.focus()
  }

  /** Sends the selection as it is; a box checked during the send goes out afterwards. */
  async function save() {
    clearTimeout(timer)
    if (!pending || saving) return
    pending = false
    saving = true
    const created = chosen.some((n) => !(labelsOf(target.repo) ?? []).some((l) => l.name === n))
    try {
      await setMrLabels(target, [...chosen])
      saved = !pending
      // A label just created: the project's list has it from now on.
      if (created) void loadLabels(target.repo, true)
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

  function onkey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation()
      onclose()
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const exact = shown.find((l) => l.name.toLowerCase() === q)
      if (exact) toggle(exact.name)
      else if (fresh) create(fresh)
      else if (shown[0]) toggle(shown[0].name)
    }
  }
</script>

<svelte:window onpointerdown={onOutside} />

<div class="picker" bind:this={el} role="dialog" aria-label={t('Labels')} tabindex="-1" {@attach anchored(trigger, { align: 'start' })} {@attach exclusive(onclose)}>
  <div class="hd">
    <b>{t('Labels')}</b><small>{mrKey(mr)}</small>
    <button class="x" onclick={onclose} aria-label={t('Close')}><Icon name="close" /></button>
  </div>
  <label class="search"><Icon name="search" /><input bind:this={input} bind:value={query} placeholder={t('Search or add a label…')} spellcheck="false" onkeydown={onkey} /></label>
  <div class="list">
    {#if projectLabels.error}<p class="err">{projectLabels.error}</p>
    {:else if !labelsOf(mr.repo)}<p class="muted">{t('Loading the project’s labels…')}</p>
    {:else}
      {#each strangers as n (n)}
        <button class="row on" onclick={() => toggle(n)}><span class="box"><Icon name="check" stroke={3} /></span><span class="dot"></span><span class="nm">{n}</span></button>
      {/each}
      {#each shown as l (l.name)}
        {@const on = chosen.includes(l.name)}
        <button class="row" class:on onclick={() => toggle(l.name)} title={l.description ?? undefined}>
          <span class="box">{#if on}<Icon name="check" stroke={3} />{/if}</span>
          <span class="dot" style:background={l.color ?? undefined}></span>
          <span class="nm">{l.name}{#if l.description}<small>{l.description}</small>{/if}</span>
        </button>
      {/each}
      {#if fresh}
        <button class="row new" onclick={() => create(fresh)} title={t('{forge} creates the label in the project', { forge: forgeName() })}><span class="box"><Icon name="plus" /></span><span class="nm">{t('Add “{label}”', { label: fresh })}<small>{t('new label')}</small></span></button>
      {:else if !shown.length && !strangers.length}
        <p class="muted">{q ? t('No label matches “{query}”.', { query }) : t('This project has no labels yet: type one.')}</p>
      {/if}
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
  .row:hover { background: var(--surface-muted); }
  .box { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 15px; height: 15px; border: 1.5px solid var(--text-muted); border-radius: var(--r3); color: var(--surface); font-size: 10px; }
  .row.on .box { border-color: var(--src-gitlab); background: var(--src-gitlab); }
  .row.new .box { border-style: dashed; color: var(--text-muted); }
  .dot { flex-shrink: 0; width: 10px; height: 10px; border-radius: 50%; background: var(--surface-muted); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text) 15%, transparent); }
  .nm { display: flex; flex-direction: column; min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .nm small { overflow: hidden; color: var(--text-muted); font-size: var(--fs-tag); text-overflow: ellipsis; }
  .muted { margin: 6px 8px; color: var(--text-muted); font-size: var(--fs-meta); }
  .err { margin: 6px 10px; color: var(--ko); font-size: var(--fs-meta); }
  .state { margin: 0; padding: 5px 10px; border-top: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-tag); }
</style>
