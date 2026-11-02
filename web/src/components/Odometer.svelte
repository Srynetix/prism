<script lang="ts">
  import { moves } from '../fun.svelte'

  /**
   * A number that rolls like an odometer when it changes: one 0-9 column per
   * digit, sliding to the right one. Plain text without animation (Off mode, reduced motion).
   */
  let { value }: { value: number | string } = $props()
  const text = $derived(String(value))
  const DIGITS = ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']
</script>

{#if moves()}
  <span class="odo" aria-label={text}>
    {#each [...text] as ch, i (text.length - i)}
      {#if /\d/.test(ch)}
        <span class="col" aria-hidden="true" style:transform="translateY({-Number(ch)}em)">{#each DIGITS as d (d)}<span>{d}</span>{/each}</span>
      {:else}
        <span aria-hidden="true">{ch}</span>
      {/if}
    {/each}
  </span>
{:else}{text}{/if}

<style>
  /* One line high: 1em, so that translateY(-n em) lands on digit n. */
  .odo { display: inline-flex; height: 1em; line-height: 1; overflow: hidden; vertical-align: -0.1em; font-variant-numeric: tabular-nums; }
  .col { display: flex; flex-direction: column; transition: transform 0.6s cubic-bezier(0.2, 0.8, 0.3, 1.15); }
  .col span { flex-shrink: 0; height: 1em; }
</style>
