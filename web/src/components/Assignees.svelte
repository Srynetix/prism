<script lang="ts">
  import { t } from '../i18n.svelte'
  import { hasCore } from '../bridge'
  import { ticketAvatar } from '../people'
  import { setMine } from '../ticket-view.svelte'
  import type { Person, Ticket } from '../types'

  /**
   * A ticket's assignees, as pills. A click assigns the ticket to me, or removes me if it is already
   * mine; the other assignees stay put. On hover, the "+" pill or my struck-through one
   * announces what the click will do.
   */
  let { ticket, me, max = 3 }: { ticket: Ticket; me: Person | null; max?: number } = $props()

  let busy = $state(false)
  let error = $state<string | null>(null)

  const mine = $derived(!!me && ticket.assignees.some((a) => a.id === me.id))
  const names = $derived(ticket.assignees.map((a) => a.name).join(', ') || t('Unassigned'))
  const can = $derived(hasCore() && !!me)

  async function onclick(e: MouseEvent) {
    e.stopPropagation()
    if (busy || !can) return
    busy = true
    error = null
    try {
      await setMine(ticket.task_id, !mine)
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }

  const title = $derived(
    error ? t('Assignment failed: {error}', { error }) : can ? `${names} · ${mine ? t('click to unassign me') : t('click to assign me')}` : names,
  )
</script>


{#snippet avatars()}
  {#each ticket.assignees.slice(0, max) as a (a.id)}
    {@const src = ticketAvatar(a.avatar)}
    <span class="av" class:me={me?.id === a.id} class:colored={!!a.color} class:pic={!!src} style:--av={a.color ?? null} title={a.name}>{#if src}<img {src} alt="" />{:else}{a.initials}{/if}</span>
  {:else}
    {#if !can}<span class="av none">—</span>{/if}
  {/each}
  {#if ticket.assignees.length > max}<span class="more">+{ticket.assignees.length - max}</span>{/if}
{/snippet}

{#if can}
  <button class="people" class:mine class:busy class:error={!!error} disabled={busy} {onclick} {title} aria-label={mine ? t('Unassign me from {id}', { id: ticket.id }) : t('Assign me to {id}', { id: ticket.id })}>
    {@render avatars()}
    {#if !mine}<span class="av add" aria-hidden="true">+</span>{/if}
  </button>
{:else}
  <span class="people" {title}>{@render avatars()}</span>
{/if}

<style>
  .people { display: inline-flex; align-items: center; vertical-align: middle; border: none; background: none; padding: 0; font: inherit; cursor: default; }
  button.people { cursor: pointer; }
  button.people:disabled { cursor: progress; opacity: 0.6; }
  .av { display: inline-flex; width: 18px; height: 18px; margin-left: -4px; border-radius: var(--r-avatar); align-items: center; justify-content: center; font-size: 8.5px; font-weight: 500; background: var(--surface-muted); color: var(--text-muted); border: 1px solid var(--surface); }
  .av:first-child { margin-left: 0; }
  .av.me { background: var(--info-bg); color: var(--info); }
  /* The tracker's color (ClickUp's), white initials on it; mine keeps a ring to stand out. */
  .av.colored { background: var(--av); color: #fff; }
  .av.colored.me { box-shadow: 0 0 0 1.5px var(--info); }
  /* A profile picture fills the pill; mine keeps its ring. */
  .av.pic { overflow: hidden; }
  .av.pic img { width: 100%; height: 100%; object-fit: cover; }
  .av.pic.me { box-shadow: 0 0 0 1.5px var(--info); }
  .av.none { background: none; border-color: transparent; }
  .more { margin-left: 2px; font-size: var(--fs-tag); color: var(--text-muted); }
  /* "+": visible on hover, or alone when the ticket has nobody. */
  .av.add { border: 1px dashed var(--text-muted); background: var(--surface); font-size: 11px; opacity: 0; transition: opacity 0.15s; }
  .people .av.add:first-child { opacity: 0.45; }
  button.people:hover .av.add, button.people:focus-visible .av.add { opacity: 1; border-color: var(--info); color: var(--info); }
  /* My pill struck through on hover: the click removes me. */
  button.people.mine:hover .av.me { background: var(--ko-bg); color: var(--ko); text-decoration: line-through; }
  button.people.mine:hover .av.me.pic { box-shadow: 0 0 0 1.5px var(--ko); }
  button.people.mine:hover .av.me.pic img { opacity: 0.35; }
  .error .av { border-color: var(--ko); }
</style>
