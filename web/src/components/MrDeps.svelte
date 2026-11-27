<script lang="ts">
  import { t } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { hasCore } from '../bridge'
  import { mrForge } from '../forge'
  import { mrKey, sameMr } from '../mr-ref'
  import { openMr } from '../mr-view.svelte'
  import { cleanTitle } from '../mr-actions'
  import { openMrs } from '../rules'
  import { blocking, declareDependency, dependsOn, findMr, viaLabel, waitedFor, waiting, type MrDep } from '../mr-deps'
  import MrGraph from './MrGraph.svelte'
  import type { MergeRequest } from '../types'

  /**
   * An MR panel's dependencies: what it waits for, what waits for it, each with why (stacked
   * branch, declared, via tickets) and where it stands; its chain as a small graph; and, on an
   * open MR, one more declared on the forge (a GitLab MR dependency, a GitHub « Depends on »).
   */
  let { mr }: { mr: MergeRequest } = $props()

  const snap = $derived(live.snapshot)
  const on = $derived(snap ? dependsOn(snap, mr) : [])
  const by = $derived(snap ? blocking(snap, mr) : [])
  const canDeclare = $derived(hasCore() && mr.state === 'opened')

  let picking = $state(false)
  let choice = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  /** The open MRs it could wait for: not itself, not one it already waits for. */
  /** On its forge only (a forge names only its own), and not one waiting for it (a cycle). */
  const choices = $derived(snap ? openMrs(snap).filter((m) => !sameMr(m, mr) && mrForge(m) === mrForge(mr) && !on.some((d) => sameMr(waitedFor(d), m)) && !by.some((d) => sameMr(waiting(d), m))).sort((a, b) => mrKey(a).localeCompare(mrKey(b), undefined, { numeric: true })) : [])

  async function declare() {
    const target = choices.find((m) => mrKey(m) === choice)
    if (!target || busy) return
    busy = true
    error = null
    try {
      await declareDependency(mr, target)
      picking = false
      choice = ''
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

{#snippet line(d: MrDep, other: MergeRequest | undefined, key: string)}
  <div class="dl">
    <button class="iid mono" onclick={() => other && openMr(other)} disabled={!other}>{key}</button>
    <span class="t" title={other ? cleanTitle(other) : undefined}>{other ? cleanTitle(other) : ''}</span>
    <span class="chip" class:inferred={d.via === 'ticket'}>{viaLabel(d.via)}</span>
    {#if other?.state === 'merged'}<span class="chip ok">✓ {t('merged')}</span>{:else if other?.pipeline === 'failed'}<span class="dot ko" title={t('Pipeline failed')}></span>{:else if other?.pipeline === 'success'}<span class="dot ok" title={t('Pipeline passed')}></span>{/if}
  </div>
{/snippet}

{#if on.length || by.length || canDeclare}
  <section class="deps">
    <h3>
      {t('Dependencies')}
      {#if !on.length && !by.length}<span class="muted">{t('none')}</span>{/if}
      <span class="grow"></span>
      {#if canDeclare && !picking}<button class="btn sm" onclick={() => (picking = true)}>+ {t('Declare a dependency…')}</button>{/if}
    </h3>
    {#if on.length}
      <div class="lab">{t('Waits for')} <b>{on.length}</b></div>
      {#each on as d (mrKey(waitedFor(d)))}{@render line(d, snap ? findMr(snap, waitedFor(d)) : undefined, mrKey(waitedFor(d)))}{/each}
    {/if}
    {#if by.length}
      <div class="lab">{t('Blocks')} <b>{by.length}</b></div>
      {#each by as d (mrKey(waiting(d)))}{@render line(d, snap ? findMr(snap, waiting(d)) : undefined, mrKey(waiting(d)))}{/each}
    {/if}
    {#if on.length || by.length}
      <div class="mini"><MrGraph mrs={[mr, ...by.map((d) => (snap ? findMr(snap, waiting(d)) : undefined)).filter((m): m is MergeRequest => !!m)]} filter={{ tickets: true, merged: true }} compact center={mr} /></div>
    {/if}
    {#if picking}
      <div class="pick">
        <select bind:value={choice} aria-label={t('The MR it waits for')}>
          <option value="" disabled>{t('Waits for…')}</option>
          {#each choices as m (mrKey(m))}<option value={mrKey(m)}>{mrKey(m)} · {cleanTitle(m)}</option>{/each}
        </select>
        <button class="btn sm primary" disabled={!choice || busy} onclick={declare}>{busy ? '…' : t('Declare')}</button>
        <button class="btn sm ghost" onclick={() => ((picking = false), (error = null))}>{t('Cancel')}</button>
      </div>
      <p class="muted hint">{(mrForge(mr) ?? snap?.forge) === 'github' ? t('Adds a “Depends on” line to its description.') : t('Adds an MR dependency on GitLab.')}</p>
      {#if error}<p class="err">{error}</p>{/if}
    {/if}
  </section>
{/if}

<style>
  section { margin-top: 14px; padding-top: 10px; border-top: 1px solid var(--border); }
  h3 { display: flex; align-items: center; gap: 8px; margin: 0 0 8px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  h3 > :not(:first-child) { text-transform: none; letter-spacing: 0; font-weight: 400; }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); }
  .lab { margin: 6px 0 2px; font-size: var(--fs-meta); color: var(--text-muted); }
  .lab b { color: var(--text); font-weight: 600; }
  .dl { display: flex; align-items: center; gap: 8px; padding: 3px 0; border-top: 1px solid var(--border); font-size: var(--fs-body); min-width: 0; }
  .dl:first-of-type { border-top: none; }
  .iid { border: none; background: none; padding: 0; color: var(--info); font-size: var(--fs-meta); cursor: pointer; }
  .iid:disabled { color: var(--text-muted); cursor: default; }
  .mono { font-family: var(--mono); }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chip { padding: 0 6px; border-radius: var(--r10); font-size: var(--fs-tag); line-height: 16px; white-space: nowrap; background: var(--surface-muted); color: var(--text-muted); border: 1px solid transparent; }
  .chip.inferred { background: none; border: 1px dashed var(--border); }
  .chip.ok { color: var(--ok); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .dot.ok { background: var(--ok); }
  .dot.ko { background: var(--ko); }
  .mini { margin-top: 8px; padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface-muted); }
  .pick { display: flex; gap: 6px; align-items: center; margin-top: 8px; }
  .pick select { flex: 1; min-width: 0; }
  .hint { margin: 4px 0 0; font-size: var(--fs-meta); }
</style>
