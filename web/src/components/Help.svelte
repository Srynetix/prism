<script lang="ts">
  import Markdown from './Markdown.svelte'
  import Icon from './Icon.svelte'
  import { helpView } from '../help-view.svelte'
  import { GROUPS, guideById, guideFor, guides, search, type Guide } from '../help-guides'
  import { openHelp, runAction } from '../help.svelte'
  import { parse, slug } from '../markdown'
  import { BLOCK_HELP } from '../block-help'
  import { ALL_BLOCKS, BLOCK_LABELS, views } from '../views'
  import { columnsOf } from '../layout.svelte'
  import { SCHEMA } from '../settings-schema'
  import { keyLabel, SHORTCUTS } from '../shortcuts'
  import { t } from '../i18n.svelte'

  /**
   * The Help view: the guides on the left (by group, searchable), the open one in the middle
   * with its outline and related guides on the right. The three references are drawn here
   * from the app's own lists (blocks, shortcuts, settings).
   */
  const guide = $derived(guideById(helpView.guide) ?? guides[0])
  const hits = $derived(search(helpView.query))
  const shown = (g: Guide) => !helpView.query.trim() || hits.has(g.id)
  const order = $derived(GROUPS.flatMap((grp) => guides.filter((g) => g.group === grp)))
  const at = $derived(order.findIndex((g) => g.id === guide.id))

  /** Sections of the open guide, for "On this page". */
  const toc = $derived.by((): { id: string; text: string }[] => {
    if (guide.id === 'ref-keys') return SHORTCUTS.map((s) => ({ id: slugOf(s.title), text: s.title }))
    if (guide.id === 'ref-settings') return SCHEMA.map((s) => ({ id: `set-${s.id}`, text: t(s.label) }))
    if (guide.id === 'ref-blocks') return []
    return parse(guide.body)
      .filter((b) => b.kind === 'heading' && b.level === 2)
      .map((b) => ('spans' in b ? { id: slug(b.spans), text: b.spans.map((s) => s.text).join('') } : { id: '', text: '' }))
  })
  const slugOf = (text: string) => slug([{ text }])

  let doc: HTMLElement | undefined = $state()
  let active = $state('')
  // A new guide starts at its top.
  $effect(() => {
    void guide.id
    doc?.scrollTo({ top: 0 })
    active = ''
  })
  function onscroll() {
    if (!doc) return
    const top = doc.getBoundingClientRect().top + 60
    let current = ''
    for (const entry of toc) {
      const el = doc.querySelector<HTMLElement>(`#${CSS.escape(entry.id)}`)
      if (el && el.getBoundingClientRect().top <= top) current = entry.id
    }
    active = current
  }
  const jump = (id: string) => doc?.querySelector(`#${CSS.escape(id)}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })

  /** The query, emphasised in a title. */
  function marked(title: string): { text: string; hit: boolean }[] {
    const q = helpView.query.trim()
    if (!q) return [{ text: title, hit: false }]
    const out: { text: string; hit: boolean }[] = []
    const lower = title.toLowerCase()
    let i = 0
    for (let j = lower.indexOf(q.toLowerCase()); j >= 0; j = lower.indexOf(q.toLowerCase(), i)) {
      if (j > i) out.push({ text: title.slice(i, j), hit: false })
      out.push({ text: title.slice(j, j + q.length), hit: true })
      i = j + q.length
    }
    if (i < title.length) out.push({ text: title.slice(i), hit: false })
    return out
  }

  /** A block is shown by one of the pages: "Show it" can go there. */
  const onPage = (b: (typeof ALL_BLOCKS)[number]) => views().some((v) => columnsOf(v).flat().includes(b))
  let search_: HTMLInputElement | undefined = $state()
  /** An image of the guide, enlarged over the view: a click or Esc (App) closes it. */
  const onimage = (src: string, alt: string) => (helpView.zoom = { src, alt })
  $effect(() => {
    void guide.id
    if (!helpView.open) helpView.zoom = null
  })
  $effect(() => {
    if (helpView.open) search_?.focus()
  })
</script>

<div class="help layer-in">
  <aside class="nav">
    <div class="title"><Icon name="help" />{t('Help')}</div>
    <label class="search"><Icon name="search" /><input bind:this={search_} bind:value={helpView.query} placeholder={t('Search the guides…')} spellcheck="false" /></label>
    <nav aria-label={t('Guides')}>
      {#each GROUPS as grp (grp)}
        {@const list = guides.filter((g) => g.group === grp && shown(g))}
        {#if list.length}
          <span class="grp">{t(grp)}</span>
          {#each list as g (g.id)}
            <button class:on={g.id === guide.id} onclick={() => openHelp(g.id)} aria-current={g.id === guide.id ? 'page' : undefined}>
              <span class="name">{#each marked(g.title) as p, i (i)}{#if p.hit}<mark>{p.text}</mark>{:else}{p.text}{/if}{/each}</span>
              {#if helpView.query.trim()}<span class="count">{hits.get(g.id)}</span>{:else if grp === 'Reference'}<span class="auto" title={t('Built from the app itself: always up to date')}>{t('auto')}</span>{/if}
            </button>
          {/each}
        {/if}
      {:else}
        <p class="none">{t('No guide.')}</p>
      {/each}
      {#if helpView.query.trim() && !hits.size}<p class="none">{t('No guide mentions “{query}”.', { query: helpView.query.trim() })}</p>{/if}
    </nav>
  </aside>

  <main class="doc" bind:this={doc} {onscroll}>
    <div class="page">
      <article>
        <p class="crumbs">{t('Help')} › {t(guide.group)}</p>
        <h1>{guide.title}</h1>
        <div class="lead"><Markdown text={guide.lead} plain onaction={runAction} images={guide.images} {onimage} /></div>

        {#if guide.id === 'ref-blocks'}
          {#each ALL_BLOCKS as b (b)}
            {@const h = BLOCK_HELP[b]}
            {@const g = guideFor(b)}
            <section class="ref">
              <div class="rhead">
                <b>{t(BLOCK_LABELS[b])}</b>
                <span class="grow"></span>
                {#if g}<button class="link" onclick={() => openHelp(g.id)}>{t('Guide: {title}', { title: g.title })}</button>{/if}
                {#if onPage(b)}<button class="link" onclick={() => runAction(`block/${b}`)}>{t('Show it')}</button>{/if}
              </div>
              {#if h}
                <p>{t(h.purpose)}</p>
                <ul>{#each h.points as p, i (i)}<li>{t(p)}</li>{/each}</ul>
                <p class="src">{t('Data: {source}', { source: t(h.source) })}</p>
              {/if}
            </section>
          {/each}
        {:else if guide.id === 'ref-keys'}
          {#each SHORTCUTS as s (s.title)}
            <h2 id={slugOf(s.title)}>{s.title}</h2>
            <div class="keys">{#each s.keys as [k, what] (k)}<kbd>{keyLabel(k)}</kbd><span>{what}</span>{/each}</div>
          {/each}
        {:else if guide.id === 'ref-settings'}
          <p class="muted">{t('Appearance, Console and Notes have a page of their own in Settings:')} <button class="link" onclick={() => runAction('settings/appearance')}>{t('Appearance')}</button> · <button class="link" onclick={() => runAction('settings/console')}>{t('Console')}</button> · <button class="link" onclick={() => runAction('settings/notes')}>{t('Notes')}</button>.</p>
          {#each SCHEMA as s (s.id)}
            <h2 id="set-{s.id}"><span>{t(s.label)}</span><code>{s.table}</code><span class="grow"></span><button class="link" onclick={() => runAction(`settings/${s.id}`)}>{t('Open in Settings')}</button></h2>
            {#if s.intro}<p class="muted">{t(s.intro)}</p>{/if}
            <dl>
              {#each s.fields as f (f.key)}
                <dt><code>{f.key}</code> {t(f.label)}{#if f.unit}<span class="muted"> · {t(f.unit)}</span>{/if}</dt>
                <dd>{f.hint ? t(f.hint) : ''}</dd>
              {/each}
            </dl>
          {/each}
        {:else}
          <div class="body"><Markdown text={guide.body} reading anchors onaction={runAction} images={guide.images} {onimage} /></div>
        {/if}

        <div class="foot">
          {#if order[at - 1]}<button onclick={() => openHelp(order[at - 1].id)}><small>← {t('Previous')}</small>{order[at - 1].title}</button>{:else}<span></span>{/if}
          {#if order[at + 1]}<button class="next" onclick={() => openHelp(order[at + 1].id)}><small>{t('Next')} →</small>{order[at + 1].title}</button>{/if}
        </div>
      </article>

      <aside class="side">
        {#if toc.length}
          <p class="k">{t('On this page')}</p>
          {#each toc as entry (entry.id)}<button class="toc" class:on={active === entry.id} onclick={() => jump(entry.id)}>{entry.text}</button>{/each}
        {/if}
        {#if guide.related.some((r) => guideById(r))}
          <p class="k rel">{t('Related')}</p>
          {#each guide.related.map(guideById).filter((g) => !!g) as r (r!.id)}<button class="rel-link" onclick={() => openHelp(r!.id)}>{r!.title}</button>{/each}
        {/if}
      </aside>
    </div>
  </main>
  {#if helpView.zoom}
    <div class="lightbox" role="dialog" aria-label={helpView.zoom.alt} tabindex="-1" onclick={() => (helpView.zoom = null)} onkeydown={() => {}}>
      <img src={helpView.zoom.src} alt={helpView.zoom.alt} />
      <p class="caption">{helpView.zoom.alt}</p>
    </div>
  {/if}
</div>

<style>
  .help { flex: 1; min-height: 0; display: grid; grid-template-columns: 240px minmax(0, 1fr); container-type: inline-size; }
  .nav { display: flex; flex-direction: column; min-height: 0; padding: 12px 8px; background: var(--surface); border-right: 1px solid var(--border); }
  .title { display: flex; align-items: center; gap: 6px; margin: 0 6px 10px; color: var(--accent); font-size: var(--fs-title); font-weight: 600; }
  .title :global(svg) { width: 16px; height: 16px; }
  .search { display: flex; align-items: center; gap: 6px; height: 28px; margin: 0 2px 6px; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text-muted); }
  .search input { flex: 1; min-width: 0; border: none; outline: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); }
  nav { display: flex; flex-direction: column; gap: 1px; overflow: auto; }
  .grp { margin: 10px 8px 3px; font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  nav button { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  nav button:hover { background: var(--surface-muted); }
  nav button.on { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); font-weight: 600; }
  .name { flex: 1; min-width: 0; }
  mark { border-radius: var(--r2); background: color-mix(in srgb, var(--warn) 30%, transparent); color: inherit; }
  .count, .auto { flex-shrink: 0; font-size: var(--fs-tag); color: var(--text-muted); }
  .auto { padding: 0 4px; border-radius: var(--r3); background: var(--surface-muted); font-size: 9px; font-weight: 600; text-transform: uppercase; }
  .none { margin: 10px 8px; color: var(--text-muted); font-size: var(--fs-meta); }

  .doc { min-width: 0; overflow: auto; }
  .page { display: grid; grid-template-columns: minmax(0, 720px) 190px; justify-content: center; gap: 36px; padding: 22px 32px 60px; }
  .crumbs { margin: 0 0 6px; font-size: var(--fs-meta); color: var(--text-muted); }
  h1 { margin: 0 0 6px; font-size: 20px; font-weight: 650; }
  .lead { margin: 0 0 18px; color: var(--text-muted); }
  .lead :global(.md) { font-size: 13.5px; }
  /* Guide text: a little larger than the blocks', sections set apart. */
  .body :global(.h2) { scroll-margin-top: 14px; }
  h2 { display: flex; align-items: baseline; gap: 8px; margin: 24px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--border); font-size: 14px; scroll-margin-top: 14px; }
  h2 code { font-family: var(--mono); font-size: var(--fs-tag); font-weight: 400; color: var(--text-muted); }
  .grow { flex: 1; }
  .muted { color: var(--text-muted); font-size: var(--fs-body); }
  .link { border: none; background: none; padding: 0; color: var(--accent); font: inherit; font-size: var(--fs-meta); font-weight: 500; cursor: pointer; }
  .link:hover { text-decoration: underline; }
  .ref { margin: 10px 0; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); font-size: var(--fs-body); }
  .rhead { display: flex; align-items: baseline; gap: 12px; }
  .ref p { margin: 4px 0; }
  .ref ul { margin: 4px 0; padding-left: 18px; color: var(--text-muted); }
  .ref .src { font-size: var(--fs-meta); color: var(--text-muted); }
  .keys { display: grid; grid-template-columns: max-content 1fr; gap: 4px 14px; font-size: var(--fs-body); }
  kbd { justify-self: start; padding: 0 6px; border: 1px solid var(--border); border-bottom-width: 2px; border-radius: var(--r5); background: var(--surface-muted); font-family: var(--mono); font-size: var(--fs-mono); white-space: nowrap; }
  dl { display: grid; grid-template-columns: minmax(180px, max-content) 1fr; gap: 6px 16px; margin: 0; font-size: var(--fs-body); }
  dt code { font-family: var(--mono); font-size: var(--fs-mono); color: var(--accent); }
  dd { margin: 0; color: var(--text-muted); }
  .foot { display: flex; justify-content: space-between; gap: 12px; margin-top: 30px; padding-top: 12px; border-top: 1px solid var(--border); }
  .foot button { padding: 8px 12px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .foot button:hover { border-color: var(--accent); }
  .foot .next { text-align: right; }
  .foot small { display: block; color: var(--text-muted); font-size: var(--fs-tag); }
  .side { position: sticky; top: 0; align-self: start; display: flex; flex-direction: column; font-size: var(--fs-meta); }
  .k { margin: 0 0 6px; font-size: var(--fs-tag); font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .k.rel { margin-top: 18px; }
  .toc { padding: 3px 0 3px 10px; border: none; border-left: 2px solid var(--border); background: none; color: var(--text-muted); font: inherit; text-align: left; cursor: pointer; }
  .toc.on { border-left-color: var(--accent); color: var(--accent); font-weight: 600; }
  .rel-link { padding: 2px 0; border: none; background: none; color: var(--info); font: inherit; text-align: left; cursor: pointer; }
  .lightbox { position: fixed; inset: 0; z-index: 60; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; padding: 24px; background: color-mix(in srgb, #000 82%, transparent); cursor: zoom-out; }
  .lightbox img { max-width: 100%; max-height: calc(100vh - 80px); border-radius: var(--r6); }
  .caption { margin: 0; color: #ddd; font-size: var(--fs-meta); }
  @container (max-width: 900px) {
    .page { grid-template-columns: minmax(0, 1fr); padding: 18px; }
    .side { display: none; }
  }
  @container (max-width: 620px) {
    .help { grid-template-columns: 180px minmax(0, 1fr); }
  }
</style>
