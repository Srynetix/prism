<script lang="ts">
  import { t } from '../i18n.svelte'
  import { setEstimate } from '../ticket-view.svelte'
  import { canPreview } from '../ticket-view.svelte'

  /**
   * An estimate in hours, entered in place: a click turns the value into a field, Enter (or
   * leaving it) saves, Esc cancels, an empty field clears it. `field`: a custom field's name, or
   * null for the tracker's own estimate. `compact`: on a ticket row, a small pill.
   */
  let { taskId, field, hours, label, compact = false }: { taskId: string; field: string | null; hours: number | null; label: string; compact?: boolean } = $props()

  let editing = $state(false)
  let text = $state('')
  let saving = $state(false)
  let error = $state('')
  const editable = canPreview()

  const shown = (h: number) => `${Math.round(h * 100) / 100} h`
  function start(e: MouseEvent) {
    e.stopPropagation()
    if (!editable || saving) return
    text = hours === null ? '' : String(Math.round(hours * 100) / 100)
    error = ''
    editing = true
  }
  async function save() {
    if (!editing) return
    editing = false
    const raw = text.trim().replace(',', '.').replace(/\s*h$/i, '')
    const next = raw === '' ? null : Number(raw)
    if (next !== null && (!Number.isFinite(next) || next < 0)) return void (error = t('“{text}” is not a number of hours', { text }))
    if (next === hours) return
    saving = true
    try {
      await setEstimate(taskId, field, next)
      error = ''
    } catch (e) {
      error = String(e)
    } finally {
      saving = false
    }
  }
  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation()
    if (e.key === 'Enter') save()
    else if (e.key === 'Escape') editing = false
  }
  const focus = (el: HTMLInputElement) => {
    el.focus()
    el.select()
  }
</script>

{#if editing}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <span class="edit" class:compact onclick={(e) => e.stopPropagation()}>
    <input type="text" inputmode="decimal" bind:value={text} {onkeydown} onblur={save} aria-label={label} placeholder="h" use:focus />{#if !compact}<span class="unit">{t('h · Enter to save, empty to clear')}</span>{/if}
  </span>
{:else}
  <button
    class="est"
    class:compact
    class:empty={hours === null}
    class:err={!!error}
    class:saving
    disabled={!editable}
    onclick={start}
    title={error ? t('Not saved: {error}', { error }) : editable ? (hours === null ? t('{label}: click to enter it (hours)', { label }) : t('{label}: click to change it (hours)', { label })) : label}
  >{#if saving}…{:else if hours !== null}{shown(hours)}{:else}{compact ? '+ h' : t('Enter an estimate')}{/if}</button>
{/if}

<style>
  .est { padding: 0 6px; border: 1px dashed transparent; border-radius: var(--r4); background: none; color: var(--text); font: inherit; font-variant-numeric: tabular-nums; cursor: text; }
  .est:hover:not(:disabled) { border-color: var(--border); background: var(--surface-muted); }
  .est:disabled { cursor: default; }
  .est.empty { color: var(--text-muted); border-color: var(--border); }
  .est.err { border-color: var(--ko); color: var(--ko); }
  .est.saving { opacity: 0.6; }
  .est.compact { padding: 0 5px; border-style: solid; border-color: var(--border); font-size: var(--fs-tag); line-height: 16px; white-space: nowrap; }
  .est.compact.empty { border-style: dashed; }
  .edit { display: inline-flex; align-items: center; gap: 6px; }
  input { width: 64px; padding: 1px 5px; border: 1px solid var(--accent); border-radius: var(--r4); background: var(--surface); color: var(--text); font: inherit; font-variant-numeric: tabular-nums; }
  .compact input { width: 44px; font-size: var(--fs-tag); padding: 0 4px; }
  .unit { color: var(--text-muted); font-size: var(--fs-meta); }
</style>
