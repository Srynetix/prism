<script lang="ts">
  import { ICONS, type IconName } from '../icons'

  /**
   * Lucide icon by its role (`icons.ts`). At text size by default (`1em`) and in its
   * color, aligned on the baseline; `spin` makes it rotate on itself (loading).
   */
  let { name, size = '1em', spin = false, stroke = 2, title, class: cls = '' }: { name: IconName; size?: string | number; spin?: boolean; stroke?: number; title?: string; class?: string } = $props()
  const Glyph = $derived(ICONS[name])
</script>

<Glyph class="icon {spin ? 'spin' : ''} {cls}" {size} strokeWidth={stroke} aria-hidden={title ? undefined : 'true'} aria-label={title} role={title ? 'img' : undefined} />

<style>
  :global(svg.icon) { flex-shrink: 0; vertical-align: -0.125em; }
  :global(svg.icon.spin) { animation: icon-spin 1s linear infinite; }
  @keyframes -global-icon-spin { to { transform: rotate(360deg); } }
</style>
