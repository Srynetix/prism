<script lang="ts">
  import { clickedOutside } from '../drawers'
  import { t, tn } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { armer } from '../armed.svelte'
  import { ago } from '../time'
  import { answer, closeProposal, counts, mcpView, openProposal, proposals, shown, title } from '../mcp-view.svelte'
  import Icon from './Icon.svelte'

  /**
   * A change an agent proposes through Prism's MCP server (`prism mcp`): its reason, the diff of
   * config.toml, what the check found, and the answer that goes back to the agent as its tool's
   * result — Apply (two clicks, no confirm()), Refuse (with a reason), or Edit then save (the agent
   * gets the difference). A custom block shows its sandbox test and is trusted as it is added.
   */
  const s = $derived(live.snapshot)
  const list = $derived(proposals(s))
  const p = $derived(shown(s))
  const agentTitle = $derived(p?.agent ? s?.agents.find((a) => a.id === p.agent)?.title : undefined)
  const arm = armer(4000)

  let refusing = $state(false)
  let reason = $state('')
  let editing = $state(false)
  let text = $state('')
  let busy = $state(false)
  let error = $state('')

  $effect(() => {
    void p?.id
    refusing = false
    reason = ''
    editing = false
    error = ''
    arm.disarm()
  })
  // Answered from elsewhere, or the agent gone: closed once none is left.
  $effect(() => {
    if (mcpView.open && s && !list.length) closeProposal()
  })

  async function act(action: 'apply' | 'refuse' | 'modified') {
    if (!p) return
    if (action === 'apply' && !arm.confirm('apply')) return
    busy = true
    error = ''
    try {
      await answer(p.id, action, { reason: action === 'refuse' ? reason.trim() : undefined, text: action === 'modified' ? text : undefined })
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  function edit() {
    if (!p) return
    text = p.text
    editing = true
  }

  /** The sandbox test of a custom block, as the agent passed it. */
  interface Tested { sandbox?: string; error?: string | null; took_ms?: number; output?: { body?: { kind?: string } }; stderr?: string }
  const test = $derived.by(() => {
    const v = p?.test as { tested_in_sandbox?: Tested } | Tested | undefined
    if (!v) return undefined
    return ('tested_in_sandbox' in v ? v.tested_in_sandbox : v) as Tested
  })

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !mcpView.open) return
    e.stopImmediatePropagation()
    if (refusing) refusing = false
    else if (editing) editing = false
    else closeProposal()
  }
  let drawerEl: HTMLElement | undefined = $state()
  function onOutside(e: PointerEvent) {
    if (mcpView.open && clickedOutside(e, drawerEl)) closeProposal()
  }
  const lineClass = (op: string) => (op === '+' ? 'add' : op === '-' ? 'del' : op === '…' ? 'gap' : '')
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#if mcpView.open && p}
  {@const c = counts(p)}
  <aside class="drawer" bind:this={drawerEl} data-drawer="52" aria-label={t('Proposal')}>
    <header>
      <Icon name={p.kind === 'custom_block' ? 'blockNormal' : 'settings'} />
      <span class="what">{title(p)}</span>
      <span class="muted">· {t('from {who}', { who: agentTitle ? `${agentTitle} (${p.client})` : p.client })} · {ago(p.at)}</span>
      <span class="grow"></span>
      {#if list.length > 1}
        {@const i = list.findIndex((x) => x.id === p.id)}
        <button class="icon" disabled={i <= 0} onclick={() => openProposal(list[i - 1].id)} aria-label={t('Previous')}><Icon name="chevronLeft" /></button>
        <span class="muted">{i + 1}/{list.length}</span>
        <button class="icon" disabled={i >= list.length - 1} onclick={() => openProposal(list[i + 1].id)} aria-label={t('Next')}><Icon name="chevronRight" /></button>
      {/if}
      <button class="icon" onclick={closeProposal} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    <div class="body">
      {#if p.why}<blockquote>« {p.why} »</blockquote>{/if}

      <div class="file">
        <span class="mono">config.toml</span>
        <span class="muted">{[c.add ? `+${c.add}` : '', c.del ? `−${c.del}` : ''].filter(Boolean).join(' ')}</span>
      </div>
      {#if editing}
        <textarea class="edit" bind:value={text} spellcheck="false" rows="22"></textarea>
      {:else}
        <pre class="code">{#each p.diff as l, i (i)}<span class={lineClass(l.op)}>{l.op === '…' ? '…' : `${l.op} ${l.text}`}</span>{'\n'}{/each}</pre>
      {/if}

      <div class="checks">
        <span class="chip ok"><Icon name="check" size={11} /> {t('valid')}</span>
        {#if p.stale}
          <span class="chip warn"><Icon name="warning" size={11} /> {t('config.toml changed since: computed again on the current version')}</span>
        {:else}
          <span class="chip ok"><Icon name="check" size={11} /> {t('file unchanged since the proposal')}</span>
        {/if}
        {#if p.kind !== 'custom_block'}<span class="chip">{t('applied live')}</span>{/if}
        {#if p.touches.length}<span class="chip">{t('touches: {what}', { what: p.touches.join(', ') })}</span>{/if}
      </div>
      {#if p.overrides.length}
        <p class="note">{tn(p.overrides.length, 'Overrides the value a pack gives: {keys} (written in config.toml, never in the pack).', 'Overrides the values a pack gives: {keys} (written in config.toml, never in the pack).', { keys: p.overrides.join(', ') })}</p>
      {/if}
      {#each p.warnings as w (w)}<p class="note">{w}</p>{/each}

      {#if p.kind === 'custom_block'}
        <section class="trust">
          {#if test}
            <p>
              <b>{t('Tested in the sandbox')}</b> ({test.sandbox}{#if test.took_ms != null}, {(test.took_ms / 1000).toFixed(1)} s{/if}):
              {#if test.error}<span class="err">{test.error}</span>{:else}{t('its JSON follows the contract ({kind})', { kind: test.output?.body?.kind ?? '?' })}{/if}
            </p>
          {:else}
            <p class="muted">{t('Not tested in the sandbox by the agent.')}</p>
          {/if}
          <p>{t('This block runs a script on this computer, with your rights, at intervals. It comes from an agent, not from Settings: trusting it records its hash outside config.toml, asked again at each change made elsewhere. Read the whole script in the diff above.')}</p>
        </section>
      {/if}

      {#if error}<p class="err">{error}</p>{/if}

      {#if refusing}
        <div class="refuse">
          <input bind:value={reason} placeholder={t('Why (sent to the agent): not this repository, rather…')} onkeydown={(e) => { if (e.key === 'Enter') void act('refuse') }} />
          <button disabled={busy} onclick={() => act('refuse')}>{t('Refuse')}</button>
          <button class="ghost" onclick={() => (refusing = false)}>{t('Cancel')}</button>
        </div>
      {/if}
    </div>
    <footer>
      {#if editing}
        <button class="ghost" onclick={() => (editing = false)}>{t('Back to the diff')}</button>
        <span class="grow"></span>
        <button class="primary" disabled={busy || !text.trim()} onclick={() => act('modified')} title={t('Saves config.toml as edited: the agent gets what differs from its proposal')}>{t('Save as edited')}</button>
      {:else}
        {#if p.kind !== 'custom_block'}
          <button class="ghost" onclick={edit} title={t('The whole config.toml with the proposal in it, to change before saving')}><Icon name="edit" size={12} /> {t('Edit before applying')}</button>
        {/if}
        <span class="grow"></span>
        {#if !refusing}<button disabled={busy} onclick={() => (refusing = true)}>{t('Refuse…')}</button>{/if}
        <button class="primary" class:armed={arm.is('apply')} disabled={busy} onclick={() => act('apply')}>
          {#if arm.is('apply')}{p.kind === 'custom_block' ? t('Trust and add?') : t('Apply?')}{:else if p.kind === 'custom_block'}{t('Trust and add')}{:else if p.stale}{t('Apply on the current version')}{:else}{t('Apply')}{/if}
        </button>
      {/if}
    </footer>
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 52; width: min(760px, 96vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid var(--border); box-shadow: var(--shadow-drawer);
  }
  header, footer {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; white-space: nowrap;
    font-size: var(--fs-meta); color: var(--text-muted);
  }
  header { border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--accent); }
  footer { border-top: 1px solid var(--border); }
  .what { color: var(--text); font-weight: 600; }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); }
  .mono { font-family: var(--font-mono); }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { background: var(--surface-muted); color: var(--text); }
  .icon:disabled { opacity: 0.4; cursor: default; }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px 24px; font-size: var(--fs-body); }
  blockquote { margin: 0 0 12px; padding: 8px 12px; border-left: 3px solid var(--accent); background: var(--surface-muted); border-radius: var(--r6); font-style: italic; }
  .file { display: flex; gap: 8px; align-items: baseline; margin-bottom: 6px; font-size: var(--fs-meta); }
  .code { margin: 0; padding: 6px 0; overflow: auto; max-height: 52vh; font-family: var(--font-mono); font-size: var(--fs-code, 12px); line-height: 1.45; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); }
  .code span { display: inline-block; min-width: 100%; padding: 0 10px; box-sizing: border-box; white-space: pre; }
  .code .add { background: var(--ok-bg); }
  .code .del { background: var(--ko-bg); }
  .code .gap { color: var(--text-muted); }
  .edit { width: 100%; box-sizing: border-box; font: inherit; font-family: var(--font-mono); font-size: var(--fs-code, 12px); line-height: 1.45; background: var(--surface-muted); color: var(--text); border: 1px solid var(--border); border-radius: var(--r6); padding: 8px 10px; resize: vertical; }
  .checks { display: flex; gap: 6px; flex-wrap: wrap; margin: 10px 0 4px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 1px 8px; border-radius: var(--r9); background: var(--surface-muted); font-size: var(--fs-tag); }
  .chip.ok { color: var(--ok); background: var(--ok-bg); }
  .chip.warn { color: var(--warn); background: var(--warn-bg); }
  .note { margin: 6px 0 0; color: var(--warn); font-size: var(--fs-meta); }
  .err { color: var(--ko); }
  .trust { margin-top: 12px; padding: 8px 12px; border: 1px solid var(--warn); border-radius: var(--r6); background: var(--warn-bg); font-size: var(--fs-meta); }
  .trust p { margin: 4px 0; }
  .refuse { display: flex; gap: 6px; margin-top: 12px; }
  .refuse input { flex: 1; font: inherit; font-size: var(--fs-meta); padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); }
  button:not(.icon) { font: inherit; font-size: var(--fs-meta); padding: 4px 12px; border-radius: var(--r6); border: 1px solid var(--border); background: var(--surface); color: var(--text); cursor: pointer; display: inline-flex; align-items: center; gap: 5px; }
  button:not(.icon):hover:not(:disabled) { background: var(--surface-muted); }
  button:disabled { opacity: 0.5; cursor: default; }
  button.ghost { border-color: transparent; }
  button.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, white); }
  button.primary.armed { background: var(--warn-bg); border-color: var(--warn); color: var(--warn); font-weight: 600; }
</style>
