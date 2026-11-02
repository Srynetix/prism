<script lang="ts">
  import Icon from '../Icon.svelte'
  import { keyOf, results, type CheckRequest } from '../../settings-checks.svelte'

  /** Verdict of a live check: ✓ and the finding, ⚠ and the problem, or "checking…". `compact`: the icon alone (table cell). */
  let { req, compact = false }: { req: CheckRequest | null; compact?: boolean } = $props()
  const r = $derived(req ? results[keyOf(req)] : undefined)
</script>

{#if r}
  <span class="check {r.state}" class:compact title={r.message}>
    {#if r.state === 'pending'}<Icon name="refresh" spin />{:else if r.state === 'ok'}<Icon name="check" />{:else}<Icon name="warning" />{/if}
    {#if !compact}<span>{r.message}</span>{/if}
  </span>
{/if}

<style>
  .check { display: inline-flex; align-items: center; gap: 5px; max-width: 100%; font-size: var(--fs-meta); color: var(--text-muted); }
  .check span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .check.ok { color: var(--ok); }
  .check.ko { color: var(--ko); }
  .check.compact { flex-shrink: 0; width: 16px; justify-content: center; }
</style>
