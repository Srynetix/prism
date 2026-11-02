<script lang="ts">
  import ArmedLabel from '../ArmedLabel.svelte'
  import { armer } from '../../armed.svelte'
  import { cfg, resetValue, setValue, valueOf } from '../../settings.svelte'
  import { keepFeature, usageDoc } from '../../feature-use.svelte'
  import { WINDOW_DAYS, countsOver, dayOf, daysCounted, differences, features, labelOf, mostUsed, neverUsed, shown } from '../../cleanup'
  import { t, tn } from '../../i18n.svelte'
  import { dateFormat } from '../../locale'

  /**
   * Settings › Cleanup: what Prism's counts say (feature-use.svelte.ts, this machine only). After
   * 30 days of counts, the features on and never used, each to turn off (its switch, written like
   * any field, saved with ⌘S) or to keep (never listed again); the most used; the keys config.toml
   * sets to another value than their default, each back to it with Reset.
   */
  let { onToml }: { onToml: () => void } = $props()

  const today = dayOf(new Date())
  const doc = $derived(usageDoc())
  const counts = $derived(countsOver(doc, today))
  const counted = $derived(daysCounted(doc, today))
  const ready = $derived(counted >= WINDOW_DAYS)
  const tracker = () => String(valueOf(['tracker', 'provider'], 'clickup')) === 'clickup'
  const isOn = (path: string[]) => !!cfg.file && !!valueOf(path)
  const unused = $derived(neverUsed(features(tracker), counts, isOn, doc.kept))
  const top = $derived(mostUsed(counts))
  const diffs = $derived(cfg.file ? differences(cfg.file.values as Record<string, unknown>, cfg.file.defaults) : [])
  /** Shown before "… more": the rest is in the TOML panel. */
  const DIFFS_SHOWN = 8

  /** Keep is for good: a second click within 3 s. */
  const armed = armer()
  const keep = (id: string) => armed.confirm(id) && keepFeature(id)
  const tableOf = (path: string[]) => `[${path.slice(0, -1).join('.')}]`
</script>

<p class="intro">{t('Over the last {n} days, counted on this machine only: nothing is sent anywhere (the profile’s usage.json). A block counts when a page showing it comes up, an action when it runs, a palette entry when it is chosen.', { n: WINDOW_DAYS })}</p>
{#if !ready}
  <p class="note">{tn(counted, 'Counting since {date}: {n} day out of {total}. “On, never used” comes once the counts cover {total} days.', 'Counting since {date}: {n} days out of {total}. “On, never used” comes once the counts cover {total} days.', { date: dateFormat({ day: 'numeric', month: 'long' }).format(new Date(`${doc.since}T12:00`)), total: WINDOW_DAYS })}</p>
{/if}

{#if ready}
  <h4 class="grp">{t('On, never used')} · {unused.length}</h4>
  <div class="card">
    {#each unused as f (f.id)}
      <div class="line">
        <span class="lb"><b>{f.label}</b><small>{f.area} · <span class="mono">{tableOf(f.path)} {f.path.at(-1)}</span></small></span>
        <span class="use zero">0 ×</span>
        <button class="btn" onclick={() => setValue(f.path, false)} title={t('Write its switch off in config.toml (saved with the other changes)')}>{t('Turn off')}</button>
        <button class="btn ghost" class:armed={armed.is(f.id)} onclick={() => keep(f.id)} title={t('Never list it again (click twice)')}><ArmedLabel label={t('Keep')} armedLabel={t('Keep for good?')} armed={armed.is(f.id)} /></button>
      </div>
    {:else}
      <p class="hint pad">{t('Every feature on was used in the last {n} days.', { n: WINDOW_DAYS })}</p>
    {/each}
  </div>
{/if}

<h4 class="grp">{t('Most used')}</h4>
<div class="card">
  {#each top as [id, n] (id)}
    <div class="line"><span class="lb">{labelOf(id)}</span><span class="use">{n} ×</span></div>
  {:else}
    <p class="hint pad">{t('Nothing counted yet.')}</p>
  {/each}
</div>

<h4 class="grp">{t('Different from default')} · {diffs.length}</h4>
<div class="card">
  {#each diffs.slice(0, DIFFS_SHOWN) as d (d.path.join('.'))}
    <div class="line">
      <span class="lb mono">{tableOf(d.path)} {d.path.at(-1)}</span>
      <span class="use">{shown(d.value)} → {t('default {value}', { value: shown(d.def) })}</span>
      <button class="btn" onclick={() => resetValue(d.path)} title={t('Remove the key from config.toml: the pack’s value, else the default, applies (saved with the other changes)')}>{t('Reset')}</button>
    </div>
  {:else}
    <p class="hint pad">{cfg.file ? t('Every key has its default value.') : t('Reading config.toml…')}</p>
  {/each}
  {#if diffs.length > DIFFS_SHOWN}
    <div class="line"><span class="lb hint">{tn(diffs.length - DIFFS_SHOWN, '… {n} other', '… {n} others')}</span><button class="btn ghost" onclick={onToml}>{t('See them in the TOML panel')}</button></div>
  {/if}
</div>

<style>
  .intro, .note { margin: 0 0 10px; color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.45; }
  .note { padding: 8px 10px; border: 1px dashed var(--border); border-radius: var(--r8); }
  .grp { margin: 16px 0 6px; font-size: var(--fs-tag); font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); }
  .card { display: flex; flex-direction: column; gap: 4px; }
  .line { display: flex; align-items: center; gap: 10px; padding: 6px 10px; border-radius: var(--r8); background: var(--surface); font-size: var(--fs-body); }
  .lb { flex: 1; min-width: 0; display: flex; flex-direction: column; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lb small { color: var(--text-muted); font-size: var(--fs-meta); }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .use { color: var(--text-muted); font-size: var(--fs-meta); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .use.zero { color: var(--warn); }
  .hint { color: var(--text-muted); font-size: var(--fs-meta); }
  .pad { margin: 0; padding: 6px 10px; }
  .btn { padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover { border-color: var(--accent); }
  .btn.ghost { background: none; color: var(--text-muted); }
  .btn.armed { border-color: var(--warn); color: var(--warn); }
</style>
