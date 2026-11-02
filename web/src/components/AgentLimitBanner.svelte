<script lang="ts">
  import Icon from './Icon.svelte'
  import { CLIS } from '../agent-cli'
  import { live } from '../live.svelte'
  import { USED_UP, planSections, resets } from '../usage'
  import { limitReached, openSwitch, planUse, suggestedFor } from '../agent-switch.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { t } from '../i18n.svelte'
  import type { Agent } from '../types'

  /**
   * Its CLI's plan used up (`[agents] suggest_switch`): when it comes back, and the CLI it can
   * continue with meanwhile, the least used of the others. Wait hides it until the plan resets.
   */
  let { a }: { a: Agent } = $props()

  const section = $derived(live.snapshot ? planSections(live.snapshot).find((s) => s.provider === a.provider) : undefined)
  /** The window that is full: when it resets. */
  const full = $derived([section?.usage?.five_hour, section?.usage?.seven_day].find((w) => w && w.utilization >= USED_UP) ?? null)
  const to = $derived(suggestedFor(a))
  const key = $derived(`switch-wait:${a.id}`)
  /** Wait: hidden until the reset it waits for (its date kept). */
  const waiting = $derived(!!full?.resets_at && pref<string | null>(key, null) === full.resets_at)
</script>

{#if limitReached(a) && !waiting}
  <div class="banner" role="status">
    <Icon name={CLIS[a.provider].icon} />
    <div class="what">
      <b>{t('{cli} limit reached', { cli: CLIS[a.provider].name })}{#if full}, {resets(full)}{/if}</b>
      {#if to}<small>{planUse(to) !== null ? t('{cli} is at {pct} % of its plan.', { cli: CLIS[to].name, pct: Math.round(planUse(to)!) }) : t('{cli} can take it on meanwhile.', { cli: CLIS[to].name })}</small>{/if}
    </div>
    {#if to}<button class="primary" onclick={() => openSwitch(a, to, 'limit')}>{t('Continue with {cli}…', { cli: CLIS[to].name })}</button>{/if}
    {#if full?.resets_at}<button class="quiet" onclick={() => setPref(key, full!.resets_at)} title={t('Hide this until its plan resets')}>{t('Wait')}</button>{/if}
  </div>
{/if}

<style>
  .banner { display: flex; align-items: center; gap: 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--ko) 9%, var(--surface)); }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; font-size: var(--fs-meta); }
  .what small { color: var(--text-muted); }
  button { padding: 4px 10px; border: 1px solid var(--border); border-radius: var(--r6, 6px); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  button.primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent); font-weight: 600; }
  button.quiet { background: none; }
</style>
