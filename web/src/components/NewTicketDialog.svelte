<script lang="ts">
  import { t } from '../i18n.svelte'
  import { trackerLogo } from '../tracker'
  import { closeNewTicket, ticketBoards } from '../new-ticket.svelte'
  import NewTicketForm from './NewTicketForm.svelte'
  import Logo from './Logo.svelte'
  import Icon from './Icon.svelte'

  /** The palette's "New ticket…": the Sprint block's form, over the page. */
  const none = $derived(ticketBoards().length === 0)
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && closeNewTicket()}>
  <div class="sheet" role="dialog" aria-label={t('New ticket')}>
    <header>
      <Logo name={trackerLogo()} size={14} />
      <b>{t('New ticket')}</b>
      <span class="grow"></span>
      <button class="x" onclick={closeNewTicket} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      {#if none}<p class="muted">{t('The tracker offers no board to create a ticket on.')}</p>
      {:else}<NewTicketForm wide onclose={closeNewTicket} />{/if}
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; z-index: 60; display: flex; align-items: flex-start; justify-content: center; padding-top: 14vh; background: rgba(0, 0, 0, 0.45); }
  .sheet { width: min(620px, 94vw); border: 1px solid var(--border); border-radius: var(--r14); background: var(--surface); box-shadow: var(--shadow-modal); overflow: hidden; }
  header { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  .grow { flex: 1; }
  .x { padding: 4px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .body { padding: 14px 16px; }
  .muted { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
</style>
