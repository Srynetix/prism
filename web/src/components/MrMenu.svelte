<script lang="ts">
  import { t } from '../i18n.svelte'
  import ActionMenu from './ActionMenu.svelte'
  import { mrMenu } from '../menus'
  import { mrNumber } from '../forge'
  import type { MergeRequest } from '../types'

  /** An MR's ⋯ menu (`mrMenu`: the palette's ⌘↵ offers the same actions). */
  let { mr, queueLabel, me = null }: { mr: MergeRequest; queueLabel: string | null | undefined; me?: string | null } = $props()
  const menu = $derived(mrMenu(mr, queueLabel, me))
</script>

<ActionMenu sections={menu.sections} head={menu.head} quick={menu.quick} label={t('All actions on {mr}', { mr: mrNumber(mr.iid, mr.forge) })} />
