<script lang="ts">
  import { t } from '../i18n.svelte'
  import { offers, trackerName } from '../tracker'
  import { hasCore } from '../bridge'
  import { changeStatus } from '../ticket-view.svelte'
  import type { StatusOption, Ticket } from '../types'
  import { statusColor } from '../rules'

  /**
   * A ticket's status in a table, editable in place among the board's statuses. Without
   * known statuses, or outside the app, a plain badge. A ticket from another board may
   * have other ones: ClickUp then refuses, and the error shows on hover.
   */
  let { ticket, statuses, highlight = false }: { ticket: Ticket; statuses: StatusOption[]; highlight?: boolean } = $props()

  let saving = $state(false)
  let error = $state<string | null>(null)
  let select: HTMLSelectElement | undefined = $state()

  const options = $derived(statuses.some((s) => s.status === ticket.status) ? statuses : [{ status: ticket.status, color: null, closed: ticket.closed }, ...statuses])
  const color = $derived(statusColor(ticket.status, statuses))

  async function onchange(e: Event & { currentTarget: HTMLSelectElement }) {
    const status = e.currentTarget.value
    if (status === ticket.status) return
    saving = true
    error = null
    try {
      await changeStatus(ticket.task_id, status, statuses)
    } catch (err) {
      error = String(err)
      // The picker reverts to the actual status.
      if (select) select.value = ticket.status
    } finally {
      saving = false
    }
  }
</script>

{#if hasCore() && offers('status') && statuses.length}
  <label
    class="pill sel status"
    class:mine={highlight}
    class:error={!!error}
    class:saving
    style="--c: {color}"
    title={error ? t('Status change failed: {error}', { error }) : `${ticket.status} · ${t('change the status in {tracker}', { tracker: trackerName() })}`}
  >
    <select class="bare" bind:this={select} value={ticket.status} disabled={saving} {onchange} aria-label={t('Status of {id}', { id: ticket.id })}>
      {#each options as s (s.status)}<option value={s.status}>{s.status}</option>{/each}
    </select>
  </label>
{:else}
  <span class="pill status" class:mine={highlight} style="--c: {color}" title={ticket.status}>{ticket.status}</span>
{/if}

<style>
  .pill { max-width: 100%; overflow: hidden; text-overflow: ellipsis; vertical-align: middle; }
  /* Tint of the status colour, readable in light as in dark. */
  .status { display: inline-flex; align-items: center; gap: 4px; background: color-mix(in srgb, var(--c) 16%, transparent); color: color-mix(in srgb, var(--c) 55%, var(--text)); }
  .status::before { content: ''; flex-shrink: 0; width: 6px; height: 6px; border-radius: var(--r2); background: var(--c); }
  /* My tickets in progress: status in bold (my assignee badges say so too). */
  .status.mine { font-weight: 600; }
  .sel { position: relative; display: inline-flex; align-items: center; padding-right: 2px; cursor: pointer; box-shadow: inset 0 0 0 1px transparent; }
  .sel:hover { box-shadow: inset 0 0 0 1px var(--c); }
  .sel.error { box-shadow: inset 0 0 0 1px var(--ko); color: var(--ko); }
  .sel.saving { opacity: 0.6; cursor: progress; }
  select { appearance: none; min-width: 0; max-width: 100%; border: none; background: none; padding: 0; color: inherit; font: inherit; text-overflow: ellipsis; cursor: inherit; outline: none; }
</style>
