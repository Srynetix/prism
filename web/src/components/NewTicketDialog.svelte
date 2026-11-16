<script lang="ts">
  import { t } from '../i18n.svelte'
  import { trackerLogo } from '../tracker'
  import { closeNewTicket, newTicketDialog, ticketBoards } from '../new-ticket.svelte'
  import NewTicketForm from './NewTicketForm.svelte'
  import Logo from './Logo.svelte'
  import Modal from './ui/Modal.svelte'

  /** The palette's "New ticket…": the Sprint block's form, over the page. */
  const none = $derived(ticketBoards().length === 0)
</script>

<Modal open key="new-ticket" label={t('New ticket')} title={t('New ticket')} width={620} onclose={closeNewTicket}>
  {#snippet head()}<Logo name={trackerLogo()} size={14} />{/snippet}
  <div class="body">
    {#if none}<p class="muted">{t('The tracker offers no board to create a ticket on.')}</p>
    {:else}<NewTicketForm wide onclose={closeNewTicket} draft={newTicketDialog.draft} oncreated={newTicketDialog.created} />{/if}
  </div>
</Modal>

<style>
  .body { padding: var(--sp-5) var(--sp-6); overflow: auto; }
  .muted { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
