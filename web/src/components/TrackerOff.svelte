<script lang="ts">
  import { t } from '../i18n.svelte'
  import { openSettings } from '../settings.svelte'
  import State from './ui/State.svelte'

  /** Empty state of a tracker block when no tracker is read (`[clickup] enabled = false`). */
</script>

<State kind="off" title={t('ClickUp is off')} detail={t('No sprint, tickets nor time entries.')} action={{ label: t('Settings › Work › Tracker › ClickUp'), run: () => openSettings('clickup') }} />
