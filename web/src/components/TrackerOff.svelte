<script lang="ts">
  import { t } from '../i18n.svelte'
  import { openSettings } from '../settings.svelte'

  /** Empty state of a tracker block when no tracker is read (`[clickup] enabled = false`). */
</script>

<div class="empty">{t('ClickUp is off')} (<button class="link" onclick={() => openSettings('clickup')}>{t('Settings › Work › Tracker › ClickUp')}</button>){t(': no sprint, tickets nor time entries.')}</div>

<style>
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; text-underline-offset: 2px; }
</style>
