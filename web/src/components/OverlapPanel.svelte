<script lang="ts">
  import { clickedOutside } from '../drawers'
  import { t } from '../i18n.svelte'
  import { live } from '../live.svelte'
  import { hasCore, invoke } from '../bridge'
  import { overlapView, closeOverlap, openOverlap } from '../overlap-view.svelte'
  import { adjustPrompt, agentAt, hunksOf, ignore, other, overlapId, pathOf, rangesText, sideMr, sideName, whoAdjusts, type Side } from '../overlaps'
  import { agentsSettings, openAgents } from '../agents.svelte'
  import { promptAgent } from '../note-send'
  import { mrKey } from '../mr-ref'
  import Icon from './Icon.svelte'
  import type { OverlapDiff } from '../bindings/OverlapDiff'

  /**
   * An overlap between two agents (or worktrees): for each file they share, the two sides' hunks
   * side by side (one after the other on a phone, a switch between them), and what to do: open the
   * other agent, ask one to adjust (its prompt prefilled, the other's diff joined), ignore it until
   * its hunks change.
   */
  let { phone = false }: { phone?: boolean } = $props()

  const s = $derived(live.snapshot)
  const o = $derived(s?.overlaps?.pairs.find((p) => overlapId(p) === overlapView.id) ?? null)
  const me = $derived(overlapView.side)
  let diff = $state<OverlapDiff | null>(null)
  let error = $state('')
  /** The diffs are read once per overlap and version of its hunks. */
  let readFor = ''
  $effect(() => {
    if (!o || !overlapView.open) return
    const key = `${overlapId(o)}#${o.key}`
    if (key === readFor) return
    readFor = key
    diff = null
    error = ''
    // The browser preview has no core to read them: the files and lines only.
    if (!hasCore()) return
    invoke<OverlapDiff>('overlap_diff', { a: o.a, b: o.b })
      .then((d) => (diff = d))
      .catch((e) => (error = String(e)))
  })

  /** Who the prompt goes to: by default the one without an MR (or with the youngest). */
  let target = $state<Side | null>(null)
  let prompt = $state('')
  let edited = $state(false)
  let menu = $state(false)
  /** On a phone, the side shown. */
  let phoneSide = $state<Side>('a')
  const to = $derived(target ?? (o && s ? whoAdjusts(s, o) : 'b'))
  const lang = $derived(agentsSettings().prompt_language)
  $effect(() => {
    if (!o || !s || edited) return
    prompt = adjustPrompt(s, o, to, diff?.[other(to)], lang)
  })
  $effect(() => {
    void overlapView.id
    target = null
    edited = false
    menu = false
    phoneSide = 'a'
  })
  const agentOf = (side: Side) => (o && s ? agentAt(s, pathOf(o, side)) : undefined)
  const nameOf = (side: Side) => (o && s ? sideName(s, o, side) : '')
  const mrOf = (side: Side) => (o && s ? sideMr(s, o, side) : undefined)
  let sending = $state(false)
  async function send() {
    const a = agentOf(to)
    if (!a || !prompt.trim()) return
    sending = true
    try {
      if (await promptAgent(a, prompt)) closeOverlap()
    } finally {
      sending = false
    }
  }
  function openOther() {
    const a = agentOf(other(me))
    if (!a) return
    closeOverlap()
    openAgents(a.id)
  }
  function ignoreIt() {
    if (!o || !s) return
    ignore(s, o)
    closeOverlap()
  }

  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape' || !overlapView.open) return
    e.stopImmediatePropagation()
    closeOverlap()
  }
  let drawerEl: HTMLElement | undefined = $state()
  function onOutside(e: PointerEvent) {
    if (overlapView.open && clickedOutside(e, drawerEl)) closeOverlap()
  }
  const lineClass = (l: string) => (l.startsWith('+') ? 'add' : l.startsWith('-') ? 'del' : l.startsWith('@@') ? 'hunk' : '')
</script>

<svelte:window onkeydowncapture={onKey} onpointerdowncapture={onOutside} />

{#snippet hunks(side: Side, file: NonNullable<typeof o>['files'][number])}
  {@const hs = hunksOf(diff?.[side], file, side)}
  <div class="side">
    <div class="who">
      <span class="n">{nameOf(side)}</span>
      {#if file.level === 'lines'}<small>{t('lines {r}', { r: rangesText(side === 'a' ? file.a : file.b) })}</small>{/if}
    </div>
    {#if hs.length}
      <pre class="code">{#each hs.join('\n').split('\n') as l, i (i)}<span class={lineClass(l)}>{l}</span>{'\n'}{/each}</pre>
    {:else if diff && diff[`${side}_error`]}
      <p class="muted">{t('Cannot read its diff: {error}', { error: diff[`${side}_error`] ?? '' })}</p>
    {:else if diff && !diff[side]}
      <p class="muted">{o?.merged && side === 'a' ? t('Merged: its changes are in the base.') : t('Its worktree is gone.')}</p>
    {:else if diff}
      <p class="muted">{t('No hunk to show.')}</p>
    {:else if !error && hasCore()}
      <p class="muted">{t('Reading…')}</p>
    {/if}
  </div>
{/snippet}

{#if overlapView.open}
  <aside class="drawer" class:phone bind:this={drawerEl} data-drawer="52" aria-label={t('Overlap')}>
    <header>
      <Icon name="overlap" />
      <span class="what">{t('Overlap')}</span>
      {#if o}<span class="muted">{o.repo} · {o.base}</span>{/if}
      <span class="grow"></span>
      <button class="icon" onclick={closeOverlap} title={t('Close (Esc)')} aria-label={t('Close')}><Icon name="close" /></button>
    </header>
    {#if !o || !s}
      <p class="muted pad">{t('This overlap is gone: neither side changes these lines any more.')}</p>
    {:else}
      <div class="body">
        <div class="pair">
          {#each ['a', 'b'] as const as side (side)}
            {@const m = mrOf(side)}
            <span class="sidechip" class:me={side === me}>
              {nameOf(side)}
              {#if m}<small>{mrKey(m)}</small>{/if}
            </span>
            {#if side === 'a'}<span class="pill {o.level === 'lines' ? 'ko' : 'warn'}">{o.level === 'lines' ? t('same lines') : t('same file')}</span>{/if}
          {/each}
        </div>
        {#if o.merged}
          <p class="note">{t('{b} has to rebase on {a} (merged)', { a: nameOf('a'), b: nameOf('b') })}</p>
        {/if}
        <div class="actions">
          {#if agentOf(other(me))}
            <button onclick={openOther}><Icon name="external" size={12} /> {t('Open the other agent')}</button>
          {/if}
          <button onclick={ignoreIt} title={t('Hidden until one of the two sides changes these hunks')}>{t('Ignore this overlap')}</button>
          {#if s.overlaps.pairs.some((p) => overlapId(p) !== overlapId(o) && (p.a === pathOf(o, me) || p.b === pathOf(o, me)))}
            {@const next = s.overlaps.pairs.find((p) => overlapId(p) !== overlapId(o) && (p.a === pathOf(o, me) || p.b === pathOf(o, me)))!}
            <button class="ghost" onclick={() => openOverlap(overlapId(next), next.a === pathOf(o, me) ? 'a' : 'b')}>{t('Next overlap')}</button>
          {/if}
        </div>
        {#if error}<p class="err">{error}</p>{/if}

        <section class="adjust">
          <div class="adjust-head">
            <span>{t('Ask {name} to adjust', { name: nameOf(to) })}</span>
            <span class="menu">
              <button class="ghost" onclick={() => (menu = !menu)} aria-expanded={menu} title={t('Who adjusts')}>▾</button>
              {#if menu}
                <span class="pop">
                  {#each ['a', 'b'] as const as side (side)}
                    <button class:on={to === side} disabled={!agentOf(side)} onclick={() => { target = side; edited = false; menu = false }}>{nameOf(side)}{#if !agentOf(side)} · {t('no agent')}{/if}</button>
                  {/each}
                </span>
              {/if}
            </span>
          </div>
          <textarea rows={phone ? 6 : 8} bind:value={prompt} oninput={() => (edited = true)}></textarea>
          <div class="send">
            {#if !agentOf(to)}<small class="muted">{t('No agent works there: nobody to ask.')}</small>{/if}
            <span class="grow"></span>
            <button class="primary" disabled={!agentOf(to) || sending || !prompt.trim()} onclick={send}>{t('Send to {name}', { name: nameOf(to) })}</button>
          </div>
        </section>

        {#if phone}
          <div class="switch" role="tablist">
            {#each ['a', 'b'] as const as side (side)}
              <button role="tab" aria-selected={phoneSide === side} class:on={phoneSide === side} onclick={() => (phoneSide = side)}>{nameOf(side)}</button>
            {/each}
          </div>
        {/if}
        {#each o.files as f (f.path)}
          <section class="file">
            <h4><span class="dot {f.level === 'lines' ? 'ko' : 'warn'}"></span>{f.path}</h4>
            <div class="sides" class:stack={phone}>
              {#if phone}
                {@render hunks(phoneSide, f)}
              {:else}
                {@render hunks('a', f)}
                {@render hunks('b', f)}
              {/if}
            </div>
          </section>
        {/each}
      </div>
    {/if}
  </aside>
{/if}

<style>
  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; z-index: 52; width: min(1040px, 96vw);
    display: flex; flex-direction: column; background: var(--surface);
    border-left: 1px solid var(--border); box-shadow: var(--shadow-drawer);
  }
  .drawer.phone { width: 100vw; }
  header {
    display: flex; align-items: center; gap: 8px; padding: 9px 12px; flex-shrink: 0; white-space: nowrap;
    border-bottom: 1px solid var(--border); box-shadow: inset 0 2px var(--ko);
    font-size: var(--fs-meta); color: var(--text-muted);
  }
  .what { color: var(--text); font-weight: 600; }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); }
  .pad { padding: 16px; }
  .icon { width: 24px; height: 24px; display: inline-flex; align-items: center; justify-content: center; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover { background: var(--surface-muted); color: var(--text); }
  .body { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px 24px; font-size: var(--fs-body); }
  .pair { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .sidechip { padding: 2px 9px; border-radius: var(--r9); background: var(--surface-muted); font-weight: 500; }
  .sidechip.me { box-shadow: inset 0 0 0 1px var(--border-strong, var(--border)); }
  .sidechip small { color: var(--text-muted); margin-left: 4px; font-weight: 400; }
  .pill { padding: 0 7px; border-radius: var(--r9); font-size: var(--fs-tag); font-weight: 500; line-height: 17px; }
  .pill.ko { color: var(--ko); background: var(--ko-bg); } .pill.warn { color: var(--warn); background: var(--warn-bg); }
  .note { margin: 8px 0 0; color: var(--warn); }
  .err { color: var(--ko); }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; margin: 10px 0; }
  button { font: inherit; font-size: var(--fs-meta); padding: 3px 10px; border-radius: var(--r6); border: 1px solid var(--border); background: var(--surface); color: var(--text); cursor: pointer; display: inline-flex; align-items: center; gap: 5px; }
  button:hover:not(:disabled) { background: var(--surface-muted); }
  button:disabled { opacity: 0.5; cursor: default; }
  button.ghost { border-color: transparent; }
  button.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, white); }
  .adjust { border: 1px solid var(--border); border-radius: var(--r8, 8px); padding: 8px 10px; margin-bottom: 12px; }
  .adjust-head { display: flex; align-items: center; gap: 6px; font-weight: 600; font-size: var(--fs-meta); }
  .menu { position: relative; }
  .pop { position: absolute; top: 100%; left: 0; z-index: 2; display: flex; flex-direction: column; min-width: 200px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r6); box-shadow: var(--shadow-drawer); }
  .pop button { border: none; justify-content: flex-start; }
  .pop button.on { font-weight: 600; }
  textarea { width: 100%; box-sizing: border-box; margin-top: 6px; font: inherit; font-size: var(--fs-meta); font-family: var(--font-mono); background: var(--surface-muted); color: var(--text); border: 1px solid var(--border); border-radius: var(--r6); padding: 6px 8px; resize: vertical; }
  .send { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
  .switch { display: flex; gap: 4px; margin-bottom: 6px; }
  .switch button.on { background: var(--surface-muted); font-weight: 600; }
  .file h4 { display: flex; align-items: center; gap: 6px; margin: 14px 0 6px; font-size: var(--fs-meta); font-family: var(--font-mono); overflow-wrap: anywhere; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .dot.ko { background: var(--ko); } .dot.warn { background: var(--warn); }
  .sides { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 8px; }
  .sides.stack { grid-template-columns: minmax(0, 1fr); }
  .side { min-width: 0; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .who { display: flex; align-items: baseline; gap: 6px; padding: 4px 8px; background: var(--surface-muted); font-size: var(--fs-meta); }
  .who .n { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .who small { color: var(--text-muted); }
  .side p { margin: 0; padding: 8px; font-size: var(--fs-meta); }
  .code { margin: 0; padding: 6px 0; overflow: auto; max-height: 360px; font-family: var(--font-mono); font-size: var(--fs-code, 12px); line-height: 1.45; }
  .code span { display: inline-block; min-width: 100%; padding: 0 8px; box-sizing: border-box; }
  .code .add { background: var(--ok-bg); }
  .code .del { background: var(--ko-bg); }
  .code .hunk { color: var(--text-muted); }
</style>
