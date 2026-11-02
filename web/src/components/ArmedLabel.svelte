<script lang="ts">
  /**
   * Label of a two-click button: both texts occupy the same cell, one hidden, so
   * the button keeps its size when it arms and nothing around it moves.
   */
  let { label, armedLabel, armed }: { label: string; armedLabel: string; armed: boolean } = $props()
</script>

<span class="swap"><span class:hide={armed} aria-hidden={armed}>{label}</span><span class:hide={!armed} aria-hidden={!armed}>{armedLabel}</span></span>

<style>
  .swap { display: inline-grid; }
  .swap > span { grid-area: 1 / 1; white-space: nowrap; text-align: center; }
  .hide { visibility: hidden; }
</style>
