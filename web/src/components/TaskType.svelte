<script lang="ts">
  import { t } from '../i18n.svelte'
  import { typeIcon } from '../type-icons'
  import Icon from './Icon.svelte'
  /**
   * A ticket's ClickUp task type, when it is not an ordinary task: EPIC highlighted.
   * With `icon`, an icon alone (the name on hover), for lists; otherwise a label.
   * `glyph` is the Font Awesome name the tracker draws the type with: drawn with its Lucide
   * equivalent (type-icons.ts), else Prism's own (a bolt for an EPIC, a diamond for a milestone,
   * the initial).
   */
  let { type, glyph = null, icon = false }: { type: string | null | undefined; glyph?: string | null; icon?: boolean } = $props()

  const kind = $derived(type?.toLowerCase() === 'epic' ? 'epic' : type?.toLowerCase() === 'milestone' ? 'milestone' : 'other')
  const title = $derived(kind === 'epic' ? t('EPIC: groups other tickets') : t('ClickUp task type: {type}', { type: type ?? '' }))
  const Glyph = $derived(typeIcon(glyph))
</script>

{#if type && icon}
  <span class="icon {kind}" {title} role="img" aria-label={type}>
    {#if Glyph}
      <Glyph aria-hidden="true" />
    {:else if kind === 'epic'}
      <!-- Lightning bolt: the EPIC convention in ticketing tools. -->
      <Icon name="bolt" />
    {:else if kind === 'milestone'}
      <Icon name="milestone" />
    {:else}
      {type[0].toUpperCase()}
    {/if}
  </span>
{:else if type}
  <span class="type" class:epic={kind === 'epic'} {title}>{#if Glyph}<Glyph aria-hidden="true" />{/if}{kind === 'epic' ? 'EPIC' : type}</span>
{/if}

<style>
  .type { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; margin-right: 5px; padding: 0 5px; border-radius: var(--r4); font-size: var(--fs-micro); font-weight: 600; line-height: 15px; letter-spacing: 0.3px; vertical-align: 1px; white-space: nowrap; background: var(--surface-muted); color: var(--text-muted); }
  .type :global(svg) { width: 9px; height: 9px; stroke-width: 2.6; }
  .epic { background: color-mix(in srgb, var(--violet) 22%, transparent); color: color-mix(in srgb, var(--violet) 70%, var(--text)); }
  .icon { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 14px; height: 14px; margin-right: 4px; border-radius: var(--r3); vertical-align: -2px; font-size: var(--fs-micro); font-weight: 700; line-height: 1; background: var(--surface-muted); color: var(--text-muted); }
  .icon :global(svg) { width: 10px; height: 10px; stroke-width: 2.6; }
  .icon.epic { background: var(--violet); color: #fff; }
  .icon.milestone { background: color-mix(in srgb, var(--info) 18%, transparent); color: var(--info); }
</style>
