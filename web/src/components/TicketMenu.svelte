<script lang="ts">
  import { t as tr } from '../i18n.svelte'
  import ActionMenu from './ActionMenu.svelte'
  import { ticketMenu } from '../menus'
  import type { ClaudeAction } from '../claude-actions'
  import type { Person, StatusOption, Ticket } from '../types'

  /** A ticket's ⋯ menu (`ticketMenu`: the palette's ⌘↵ offers the same actions). */
  let {
    ticket: t, me, statuses, claude, toSprint = false,
  }: { ticket: Ticket; me: Person | null; statuses: StatusOption[]; claude?: ClaudeAction[]; toSprint?: boolean } = $props()
  const menu = $derived(ticketMenu(t, me, statuses, claude, toSprint))
</script>

<ActionMenu sections={menu.sections} head={menu.head} quick={menu.quick} label={tr('All actions on {id}', { id: t.id })} />
