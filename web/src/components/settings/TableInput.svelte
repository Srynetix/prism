<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import Control from './Control.svelte'
  import CheckBadge from './CheckBadge.svelte'
  import { requestOf } from '../../settings-checks.svelte'
  import type { Column } from '../../settings-schema'

  /**
   * A `[[tables]]` table: one row per item, one column per key; + adds a row
   * (`blank`), × removes it, ↑ moves it up (order is the file's). An empty optional
   * cell removes its key from the row. `fromPack`: a pack's entry ([[repos]], merged by name), whose
   * × only drops the file's changes and whose place is the pack's. Too narrow for its columns (a
   * wide table, the TOML panel open): one card per row instead, its first column as the title, its
   * fields side by side, its toggles as chips.
   */
  type Row = Record<string, unknown>
  let { rows, columns, blank, onchange, fromPack }: { rows: Row[]; columns: Column[]; blank: Row; onchange: (rows: Row[]) => void; fromPack?: (r: Row) => boolean } = $props()

  function set(i: number, key: string, v: unknown) {
    onchange(rows.map((r, n) => {
      if (n !== i) return r
      const next = { ...r, [key]: v }
      if (v === null) delete next[key]
      return next
    }))
  }
  function up(i: number) {
    const next = [...rows]
    ;[next[i - 1], next[i]] = [next[i], next[i - 1]]
    onchange(next)
  }
  const grid = $derived(`${columns.map((c) => c.width ?? 'minmax(0, 1fr)').join(' ')} 52px`)

  /** What the table needs: its fixed columns, and room for a path or a project in the flexible ones. */
  const needed = $derived(columns.reduce((sum, c) => sum + (c.width?.endsWith('px') ? parseInt(c.width) : 170) + 8, 52))
  let width = $state(0)
  const cards = $derived(width > 0 && width < needed)
  const title = $derived(columns[0])
  const fields = $derived(columns.slice(1).filter((c) => c.kind !== 'toggle'))
  const toggles = $derived(columns.slice(1).filter((c) => c.kind === 'toggle'))
  const cellValue = (r: Row, c: Column) => r[c.key] ?? (c.kind === 'list' ? [] : c.kind === 'toggle' ? false : null)
</script>

{#snippet acts(r: Row, i: number)}
  <span class="acts">
    {#if fromPack?.(r)}
      <span class="pk" title={t('From the pack: its place is the pack’s')}>pack</span>
      <button class="rm" onclick={() => onchange(rows.filter((_, n) => n !== i))} title={t('Back to the pack’s values: drops what config.toml changes in this row')}><Icon name="close" /></button>
    {:else}
      <button disabled={i === 0 || !!fromPack?.(rows[i - 1])} onclick={() => up(i)} title={t('Move up')}><Icon name="up" /></button>
      <button class="rm" onclick={() => onchange(rows.filter((_, n) => n !== i))} title={t('Remove this row')}><Icon name="close" /></button>
    {/if}
  </span>
{/snippet}

{#snippet cell(r: Row, i: number, c: Column)}
  <div class="cell"><Control kind={c.kind} value={cellValue(r, c)} onchange={(v) => set(i, c.key, v)} options={c.options} optional={c.optional} placeholder={c.placeholder} compact />{#if c.check}<CheckBadge req={requestOf(c.check, r[c.key], (k) => r[k])} compact />{/if}</div>
{/snippet}

<div class="table" class:cards style:--grid={grid} bind:clientWidth={width}>
  {#if cards}
    {#each rows as r, i (i)}
      <div class="card">
        <div class="top">{@render cell(r, i, title)}{@render acts(r, i)}</div>
        {#if fields.length}
          <div class="fields">
            {#each fields as c (c.key)}<label class="field"><span>{c.label}</span>{@render cell(r, i, c)}</label>{/each}
          </div>
        {/if}
        {#if toggles.length}
          <div class="chips" role="group" aria-label={t('Switches')}>
            {#each toggles as c (c.key)}
              <button class="chip" class:on={!!r[c.key]} aria-pressed={!!r[c.key]} onclick={() => set(i, c.key, !r[c.key])}>{#if r[c.key]}<Icon name="check" />{/if}{c.label}</button>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      <p class="none">{t('None.')}</p>
    {/each}
  {:else}
    <div class="head">{#each columns as c (c.key)}<span>{c.label}</span>{/each}<span></span></div>
    {#each rows as r, i (i)}
      <div class="row">
        {#each columns as c (c.key)}{@render cell(r, i, c)}{/each}
        {@render acts(r, i)}
      </div>
    {:else}
      <p class="none">{t('None.')}</p>
    {/each}
  {/if}
  <button class="add" onclick={() => onchange([...rows, { ...blank }])}><Icon name="add" />{t('Add')}</button>
</div>

<style>
  .table { display: flex; flex-direction: column; gap: 5px; width: 100%; }
  .head, .row { display: grid; grid-template-columns: var(--grid); gap: 8px; align-items: center; }
  .head span { font-size: 10px; font-weight: 500; letter-spacing: 0.3px; text-transform: uppercase; color: var(--text-muted); }
  .cell { min-width: 0; display: flex; align-items: center; gap: 4px; }
  .acts { display: flex; gap: 2px; }
  .acts button { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; padding: 0; border: none; border-radius: var(--r6); background: none; color: var(--text-muted); cursor: pointer; }
  .acts button:hover:not(:disabled) { color: var(--text); background: var(--surface-muted); }
  .acts .rm:hover { color: var(--ko); background: var(--ko-bg); }
  .acts button:disabled { opacity: 0.3; cursor: default; }
  .pk { width: 24px; font-size: 9px; text-align: center; color: var(--info); }
  .add { align-self: flex-start; display: inline-flex; align-items: center; gap: 5px; margin-top: 2px; padding: 3px 10px; border: 1px dashed var(--border); border-radius: var(--r6); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .add:hover { color: var(--text); border-color: var(--accent); }
  .card { display: flex; flex-direction: column; gap: 7px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--bg); }
  .top { display: flex; align-items: center; gap: 8px; }
  .top .cell { flex: 1; max-width: 260px; font-weight: 550; }
  .top .acts { margin-left: auto; }
  .fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 6px 12px; }
  .field { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .field > span { font-size: 10px; font-weight: 500; letter-spacing: 0.3px; text-transform: uppercase; color: var(--text-muted); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip { display: inline-flex; align-items: center; gap: 3px; padding: 1px 9px; border: 1px solid var(--border); border-radius: 11px; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); line-height: 18px; cursor: pointer; }
  .chip:hover { border-color: var(--accent); color: var(--text); }
  .chip.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--text); }
  .none { margin: 0; font-size: var(--fs-meta); color: var(--text-muted); }
</style>
