<script lang="ts">
  import Icon from '../Icon.svelte'
  import { invoke } from '../../bridge'
  import { t, tn } from '../../i18n.svelte'
  import { toast } from '../../toast.svelte'
  import { armer } from '../../armed.svelte'
  import { FACT_KINDS, factKindLabel, factsOf, forgetEverything, forgetFact, loadMemory, memory, saveFact } from '../../assistant.svelte'
  import type { Fact } from '../../bindings/Fact'
  import type { FactKind } from '../../bindings/FactKind'

  /**
   * The assistant's memory (Assistant › Memory): one fact per Markdown file of the profile's
   * `assistant/memory/`, read at the start of each conversation. Filtered by kind; each changed in
   * place or forgotten; a new one added by hand; everything forgotten in two clicks.
   */
  let kind = $state<FactKind | null>(null)
  let editing = $state<string | null>(null)
  let text = $state('')
  let editKind = $state<FactKind>('project')
  let adding = $state(false)
  let busy = $state(false)
  const armed = armer()
  const shown = $derived(factsOf(kind))

  $effect(() => {
    void loadMemory()
  })

  async function act(f: () => Promise<unknown>) {
    if (busy) return
    busy = true
    try {
      await f()
    } catch (e) {
      toast(String(e), 8000)
    } finally {
      busy = false
    }
  }

  function edit(f: Fact) {
    adding = false
    editing = f.id
    text = f.text
    editKind = f.kind
  }

  function add() {
    editing = null
    adding = true
    text = ''
    editKind = kind ?? 'project'
  }

  const save = (f: Fact | null) =>
    act(async () => {
      await saveFact({ id: f?.id, kind: editKind, text: text.trim(), source: f?.source || t('written by hand') })
      editing = null
      adding = false
    })

  const forget = (f: Fact) => act(async () => { if (armed.confirm(f.id)) await forgetFact(f.id) })
  const forgetAll = () => act(async () => { if (armed.confirm('all')) toast(tn(await forgetEverything(), '{n} fact forgotten', '{n} facts forgotten')) })
  const openFolder = () => act(() => invoke('assistant_open', { with: 'finder' }))
  const when = (f: Fact) => (f.at ? new Date(`${f.at}T12:00:00`).toLocaleDateString(undefined, { day: 'numeric', month: 'short' }) : '')
</script>

{#snippet form(f: Fact | null)}
  <div class="form">
    <select bind:value={editKind} aria-label={t('Kind')}>{#each FACT_KINDS as k (k)}<option value={k}>{factKindLabel(k)}</option>{/each}</select>
    <!-- svelte-ignore a11y_autofocus -->
    <textarea bind:value={text} rows="3" autofocus aria-label={t('Fact')} placeholder={t('One fact, naming its repository: “The back is the server/ folder of shop.”')}
      onkeydown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) void save(f); if (e.key === 'Escape') { e.stopPropagation(); editing = null; adding = false } }}></textarea>
    <div class="acts">
      <button class="btn pri" onclick={() => save(f)} disabled={busy || !text.trim()}>{t('Save')}</button>
      <button class="btn ghost" onclick={() => { editing = null; adding = false }}>{t('Cancel')}</button>
    </div>
  </div>
{/snippet}

<div class="mem">
  <nav class="kinds" aria-label={t('Kinds')}>
    <button class:on={kind === null} onclick={() => (kind = null)}>{t('All')}<small>{memory.facts.length}</small></button>
    {#each FACT_KINDS as k (k)}
      <button class:on={kind === k} onclick={() => (kind = k)}>{factKindLabel(k)}<small>{factsOf(k).length}</small></button>
    {/each}
  </nav>
  <section>
    <div class="top">
      <span class="where" title={memory.dir}>{t('Memory of this profile')} · <span class="mono">{memory.dir}</span></span>
      <span class="grow"></span>
      <button class="btn" onclick={add} disabled={busy}><Icon name="plus" size={11} />{t('Add a fact')}</button>
      <button class="btn" onclick={openFolder} disabled={busy}>{t('Open the folder')}</button>
      <button class="btn danger" class:armed={armed.is('all')} onclick={forgetAll} disabled={busy || !memory.facts.length}>{armed.is('all') ? t('Click again: forget everything') : t('Forget everything…')}</button>
    </div>
    {#if memory.error}<p class="err">{memory.error}</p>{/if}
    {#if adding}{@render form(null)}{/if}
    {#each shown as f (f.id)}
      <div class="fact">
        {#if editing === f.id}
          {@render form(f)}
        {:else}
          <div class="txt"><span class="kind">{factKindLabel(f.kind)}</span>{f.text}</div>
          <div class="src">{[when(f) && t('Remembered on {date}', { date: when(f) }), f.source].filter(Boolean).join(' · ')} · <span class="mono">{f.id}.md</span></div>
          <div class="fa">
            <button class="btn ghost" onclick={() => edit(f)} disabled={busy}>{t('Edit')}</button>
            <button class="btn ghost" class:armed={armed.is(f.id)} onclick={() => forget(f)} disabled={busy}>{armed.is(f.id) ? t('Click again') : t('Forget')}</button>
          </div>
        {/if}
      </div>
    {:else}
      {#if memory.loaded && !adding}<p class="none">{kind ? t('Nothing of this kind.') : t('Nothing remembered yet. Tell the assistant “remember that…”, or add a fact here.')}</p>{/if}
    {/each}
  </section>
</div>

<style>
  .mem { display: grid; grid-template-columns: 180px minmax(0, 1fr); min-height: 100%; }
  .kinds { display: flex; flex-direction: column; gap: 2px; padding: 8px; border-right: 1px solid var(--border); background: var(--surface-muted); }
  .kinds button { display: flex; justify-content: space-between; padding: 5px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .kinds button.on { background: color-mix(in srgb, var(--info) 14%, var(--surface)); font-weight: 600; }
  .kinds small { color: var(--text-muted); }
  section { min-width: 0; }
  .top { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .where { overflow: hidden; color: var(--text-muted); font-size: var(--fs-meta); text-overflow: ellipsis; white-space: nowrap; }
  .grow { flex: 1; }
  .mono { font-family: var(--mono); font-size: var(--fs-tag); }
  .fact { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 2px 10px; padding: 8px 12px; border-bottom: 1px solid var(--border); }
  .fact > :global(.form) { grid-column: 1 / -1; }
  .txt { grid-column: 1; }
  .src { grid-column: 1; color: var(--text-muted); font-size: var(--fs-tag); }
  .fa { grid-row: 1 / span 2; grid-column: 2; display: flex; align-items: center; gap: 4px; }
  .kind { margin-right: 6px; padding: 0 5px; border-radius: 4px; background: var(--surface-muted); color: var(--text-muted); font-size: 9.5px; text-transform: uppercase; letter-spacing: 0.03em; }
  .form { display: grid; gap: 6px; padding: 8px 12px; }
  .form select { justify-self: start; font: inherit; }
  .form textarea { width: 100%; box-sizing: border-box; font: inherit; }
  .acts { display: flex; gap: 6px; }
  .btn { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn.pri { border-color: var(--info); background: var(--info); color: var(--surface); }
  .btn.ghost { border-color: transparent; background: none; color: var(--text-muted); }
  .btn.danger { color: var(--ko); }
  .btn.armed { border-color: var(--ko); background: var(--ko); color: var(--surface); }
  .btn:disabled { opacity: 0.55; cursor: default; }
  .none, .err { margin: 14px 12px; color: var(--text-muted); font-size: var(--fs-meta); }
  .err { color: var(--ko); }
</style>
