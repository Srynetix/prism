<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { IconName } from '../icons'
  import Icon from './Icon.svelte'

  /**
   * A chip on an agent that opens something (AsideChip, ProposalChip, OverlapChip): a `.pill` of a
   * tone, an icon and a few words. A span, not a button: it sits inside clickable rows; without
   * `go`, it is only said (`still`).
   */
  let {
    tone,
    icon,
    title,
    go,
    children,
  }: { tone: 'info' | 'warn' | 'ko' | 'src'; icon: IconName; title: string; go?: () => void; children: Snippet } = $props()

  function click(e: Event) {
    e.stopPropagation()
    e.preventDefault()
    go?.()
  }
</script>

{#if go}
  <span class="pill {tone} chip" role="button" tabindex="0" {title} onclick={click} onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && click(e)}>
    <Icon name={icon} size={11} />{@render children()}
  </span>
{:else}
  <span class="pill {tone} chip still" {title}><Icon name={icon} size={11} />{@render children()}</span>
{/if}

<style>
  .chip { display: inline-flex; align-items: center; gap: 3px; max-width: 220px; font-weight: 600; cursor: pointer; vertical-align: middle; }
  .chip:hover:not(.still) { filter: brightness(1.1); }
  .chip.still { cursor: default; }
</style>
