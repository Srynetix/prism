<script lang="ts">
  import { t } from '../i18n.svelte'
  import { faIcon } from '../fa-icons'
  /**
   * A ticket's ClickUp task type, when it is not an ordinary task: EPIC highlighted.
   * With `icon`, an icon alone (the name on hover), for lists; otherwise a label.
   * `glyph` is the Font Awesome icon the tracker draws the type with: drawn when bundled,
   * else Prism's own (lightning bolt for an EPIC, diamond for a milestone, the initial).
   */
  let { type, glyph = null, icon = false }: { type: string | null | undefined; glyph?: string | null; icon?: boolean } = $props()

  const kind = $derived(type?.toLowerCase() === 'epic' ? 'epic' : type?.toLowerCase() === 'milestone' ? 'milestone' : 'other')
  const title = $derived(kind === 'epic' ? t('EPIC: groups other tickets') : t('ClickUp task type: {type}', { type: type ?? '' }))
  const fa = $derived(faIcon(glyph))
</script>

{#snippet drawn(f: { width: number; path: string })}
  <svg viewBox="0 0 {f.width} 512" aria-hidden="true"><path d={f.path} /></svg>
{/snippet}

{#if type && icon}
  <span class="icon {kind}" {title} role="img" aria-label={type}>
    {#if fa}
      {@render drawn(fa)}
    {:else if kind === 'epic'}
      <!-- Lightning bolt: the EPIC convention in ticketing tools. -->
      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M7 .8 2.2 6.9h3.2L4.6 11.2l5.2-6.5H6.5Z" /></svg>
    {:else if kind === 'milestone'}
      <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M6 1 11 6 6 11 1 6Z" /></svg>
    {:else}
      {type[0].toUpperCase()}
    {/if}
  </span>
{:else if type}
  <span class="type" class:epic={kind === 'epic'} {title}>{#if fa}{@render drawn(fa)}{/if}{kind === 'epic' ? 'EPIC' : type}</span>
{/if}

<style>
  .type { display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0; margin-right: 5px; padding: 0 5px; border-radius: var(--r4); font-size: 9.5px; font-weight: 600; line-height: 15px; letter-spacing: 0.3px; vertical-align: 1px; white-space: nowrap; background: var(--surface-muted); color: var(--text-muted); }
  .type svg { width: 9px; height: 9px; fill: currentColor; }
  .epic { background: color-mix(in srgb, var(--violet) 22%, transparent); color: color-mix(in srgb, var(--violet) 70%, var(--text)); }
  .icon { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 14px; height: 14px; margin-right: 4px; border-radius: var(--r3); vertical-align: -2px; font-size: 8.5px; font-weight: 700; line-height: 1; background: var(--surface-muted); color: var(--text-muted); }
  .icon svg { width: 10px; height: 10px; fill: currentColor; }
  .icon.epic { background: var(--violet); color: #fff; }
  .icon.milestone { background: color-mix(in srgb, var(--info) 18%, transparent); color: var(--info); }
</style>
