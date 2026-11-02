<script lang="ts">
  import type { ContextUse } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * How much of an agent's context window its conversation fills, at its last reply: under its
   * terminal, a bar, the tokens used and what remains; `compact`, in the list, the percentage
   * only. Fuller than 70 %, then 85 %: warned, then alarming — the CLI compacts it on its own when
   * it is nearly full. Window unknown (a Command Code model other than its default one): the
   * tokens alone, no bar. `fill`, on a phone: the bar takes the width left, the percentage and
   * what remains beside it (the tokens used in its title).
   */
  let { context, compact = false, fill = false }: { context: ContextUse; compact?: boolean; fill?: boolean } = $props()

  const size = $derived(context.size)
  const share = $derived(size ? Math.min(1, context.used / size) : 0)
  const pct = $derived(Math.round(share * 100))
  const level = $derived(share >= 0.85 ? 'ko' : share >= 0.7 ? 'warn' : 'ok')
  const tokens = (n: number) => (n >= 1_000_000 ? `${+(n / 1_000_000).toFixed(1)}M` : `${Math.round(n / 1000)}k`)
  const left = $derived(size ? Math.max(0, size - context.used) : 0)
  const title = $derived(
    size
      ? t('Context window: {used} of {size} tokens used at the last reply, {left} left', { used: tokens(context.used), size: tokens(size), left: tokens(left) })
      : t('Context: {used} tokens at the last reply; the window of this model is not known', { used: tokens(context.used) }),
  )
</script>

{#if size}
  <span class="ctx {level}" class:compact class:fill {title} aria-label={title}>
    <span class="bar"><i style:width="{pct}%"></i></span>
    {#if compact}{pct}%{:else if fill}<b>{pct}%</b><span>· {t('{n} left', { n: tokens(left) })}</span>{:else}<b>{pct}%</b><span>{t('{used} / {size} tokens', { used: tokens(context.used), size: tokens(size) })}</span><span>·</span><span>{t('{n} left', { n: tokens(left) })}</span>{/if}
  </span>
{:else}
  <span class="ctx" class:compact {title} aria-label={title}>{#if compact}{tokens(context.used)}{:else}<span>{t('{n} tokens', { n: tokens(context.used) })}</span>{/if}</span>
{/if}

<style>
  .ctx { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-meta); color: var(--text-muted); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .ctx:not(.compact) { gap: 8px; }
  .ctx:not(.compact)::before { content: 'Context'; }
  b { font-weight: 600; }
  .bar { width: 160px; height: 4px; border-radius: var(--r2); background: var(--surface-muted); overflow: hidden; }
  .compact .bar { width: 22px; }
  .fill { flex: 1; min-width: 0; }
  .fill .bar { flex: 1; width: auto; min-width: 32px; }
  .bar i { display: block; height: 100%; border-radius: var(--r2); background: var(--k); }
  .ok { --k: var(--ok); }
  .warn { --k: var(--warn); color: var(--warn); }
  .ko { --k: var(--ko); color: var(--ko); }
</style>
