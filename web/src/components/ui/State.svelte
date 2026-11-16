<script lang="ts">
  import { t } from '../../i18n.svelte'
  import Icon from '../Icon.svelte'
  import type { IconName } from '../../icons'

  /**
   * What a block, a list or a drawer shows when it has nothing to show yet, nothing at all, or
   * failed: one look everywhere.
   * - `loading`: grey lines that shimmer (`lines`), or with a `title` a spinner and that line
   *   (« Loading the log… »), `inline` on one line;
   * - `empty`: a sign, a sentence, perhaps an action;
   * - `error`: what failed, why (`detail`), and Retry when there is `retry`;
   * - `off`: a source not set up, and where to set it up (`action`).
   */
  let {
    kind,
    title = '',
    detail = '',
    icon,
    lines = 3,
    inline = false,
    retry,
    action,
  }: {
    kind: 'loading' | 'empty' | 'error' | 'off'
    title?: string
    detail?: string
    icon?: IconName
    lines?: number
    inline?: boolean
    retry?: () => void
    action?: { label: string; run: () => void }
  } = $props()

  const sign = $derived<IconName>(icon ?? (kind === 'error' ? 'warning' : kind === 'off' ? 'settings' : 'check'))
  const WIDTHS = [72, 92, 55, 80, 64, 88]
</script>

{#if kind === 'loading' && !title}
  <div class="skel" role="status" aria-label={t('Loading…')}>
    {#each Array.from({ length: lines }, (_, i) => i) as i (i)}<i style:width="{WIDTHS[i % WIDTHS.length]}%"></i>{/each}
  </div>
{:else if kind === 'loading'}
  <p class="line" class:inline role="status"><span class="spinner"></span>{title}</p>
{:else if inline}
  <p class="line {kind}" class:inline role={kind === 'error' ? 'alert' : undefined}>
    {#if kind === 'error'}<Icon name="warning" />{/if}<span>{title}{#if detail}<span class="detail"> · {detail}</span>{/if}</span>
    {#if retry}<button class="btn sm" onclick={retry}>{t('Try again')}</button>{/if}
    {#if action}<button class="btn sm" onclick={action.run}>{action.label}</button>{/if}
  </p>
{:else}
  <div class="state {kind}" role={kind === 'error' ? 'alert' : undefined}>
    <span class="sign"><Icon name={sign} /></span>
    {#if title}<b>{title}</b>{/if}
    {#if detail}<span class="detail">{detail}</span>{/if}
    {#if retry || action}
      <span class="acts">
        {#if retry}<button class="btn sm" onclick={retry}>{t('Try again')}</button>{/if}
        {#if action}<button class="btn sm" onclick={action.run}>{action.label}</button>{/if}
      </span>
    {/if}
  </div>
{/if}

<style>
  .state { display: grid; justify-items: center; gap: var(--sp-3); padding: var(--sp-6) var(--sp-5); text-align: center; color: var(--text-muted); font-size: var(--fs-meta); }
  .sign { display: grid; place-items: center; width: 28px; height: 28px; border-radius: 50%; background: var(--surface-muted); color: var(--text-muted); }
  .error .sign { background: color-mix(in srgb, var(--ko-bg) var(--pill-fill), transparent); color: var(--ko); }
  .state b { color: var(--text); font-size: var(--fs-body); font-weight: 500; overflow-wrap: anywhere; }
  .detail { overflow-wrap: anywhere; white-space: pre-wrap; }
  .acts { display: inline-flex; gap: var(--sp-3); margin-top: var(--sp-1); }
  .line { display: flex; align-items: center; gap: var(--sp-3); margin: 0; padding: var(--sp-4) var(--sp-5); color: var(--text-muted); font-size: var(--fs-body); }
  .line.inline { padding: 0; display: inline-flex; }
  .line.error { color: var(--ko); }
  .line.error .detail { color: var(--text-muted); }
  .skel { display: grid; gap: var(--sp-4); padding: var(--sp-4) var(--sp-5); }
  .skel i { display: block; height: 10px; border-radius: var(--r4); background: linear-gradient(90deg, var(--surface-muted) 0%, color-mix(in srgb, var(--surface-muted) 40%, var(--surface)) 50%, var(--surface-muted) 100%); background-size: 200% 100%; animation: shimmer 1.2s linear infinite; }
  :global(html[data-reduce-motion]) .skel i { animation: none; }
  @keyframes shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
</style>
