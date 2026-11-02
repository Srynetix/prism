<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import Markdown from '../components/Markdown.svelte'
  import { invoke } from '../bridge'
  import { t } from '../i18n.svelte'
  import { ago } from '../time'
  import { pref, setPref } from '../layout.svelte'
  import { agentArtifacts, agentPages, agentPageText, SCRATCHPAD, type AgentArtifact, type AgentPage } from '../agents.svelte'
  import { markSeen, seenAt } from './web-seen.svelte'

  /**
   * An agent's Web tab on a phone: its artifacts (the pages its CLI writes to the session's
   * scratchpad, newest first, the newest as a card with its preview) or the pages of its folder,
   * grouped by directory. A page opens over the agent's view, served for real by the remote
   * server (`agent_page_open`: its scripts and the files next to it, in a sandbox); Markdown and
   * text read and shown by Prism. "Ask a change…" puts the page in the agent's prompt.
   */
  let { id, onask }: { id: string; onask: (text: string) => void } = $props()

  type Source = 'artifacts' | 'folder'
  /** Picked here; until then, the artifacts when one is under an hour old (as the Mac's picker), else the last one picked. */
  let picked = $state<Source | null>(null)
  const setSource = (s: Source) => setPref('remote.web.source', (picked = s))

  let error = $state<string | null>(null)
  let artifacts = $state<AgentArtifact[] | null>(null)
  /** Seen before this opening: what is newer keeps its dot while the tab is open. */
  let seenBefore = $state('')
  let seenFor = ''
  const fresh = $derived((artifacts ?? []).some((a) => a.modified && Date.now() - new Date(a.modified).getTime() < 3_600_000))
  const source = $derived<Source>(picked ?? (fresh ? 'artifacts' : pref<Source>('remote.web.source', 'artifacts')))

  async function loadArtifacts() {
    try {
      const next = await agentArtifacts(id)
      if (seenFor !== id) {
        seenBefore = seenAt(id)
        seenFor = id
      }
      if (JSON.stringify(next) !== JSON.stringify(artifacts)) artifacts = next
      markSeen(id, next)
    } catch (e) {
      error = String(e)
      artifacts ??= []
    }
  }
  // Read again every 5 s: the agent writes them along the way.
  $effect(() => {
    void loadArtifacts()
    const timer = setInterval(loadArtifacts, 5000)
    return () => clearInterval(timer)
  })

  /* The folder's pages */
  const TEXT = /\.(md|markdown|txt|html?|svg)$/i
  let ignored = $state(pref<boolean>('agents.picker.ignored', false))
  let pages = $state<AgentPage[] | null>(null)
  let query = $state('')
  $effect(() => {
    if (source !== 'folder') return
    const also = ignored
    agentPages(id, also).then((list) => (pages = list.filter((p) => TEXT.test(p.path)))).catch((e) => ((error = String(e)), (pages = [])))
  })
  function toggleIgnored() {
    setPref('agents.picker.ignored', (ignored = !ignored))
    pages = null
  }
  const dirOf = (p: string) => (p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : '')
  const nameOf = (p: string) => p.split('/').pop() ?? p
  const groups = $derived.by(() => {
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
    const by = new Map<string, AgentPage[]>()
    for (const p of pages ?? []) {
      if (!terms.every((w) => p.path.toLowerCase().includes(w))) continue
      by.set(dirOf(p.path), [...(by.get(dirOf(p.path)) ?? []), p])
    }
    return [...by].sort(([a], [b]) => a.localeCompare(b))
  })

  /* Their addresses on the remote server, one per root (the folder, the scratchpad) */
  const bases = $state<Record<string, string | null>>({})
  async function base(scratchpad: boolean): Promise<string | null> {
    const k = scratchpad ? 's' : 'f'
    if (bases[k] === undefined) {
      try {
        bases[k] = (await invoke<{ url: string }>('agent_page_open', { id, scratchpad })).url
      } catch {
        // An older Mac, or the browser preview: the page's text only (no scripts).
        bases[k] = null
      }
    }
    return bases[k]
  }
  const address = (b: string, path: string) => b + path.split('/').map(encodeURIComponent).join('/')

  const kind = (p: string) => (/\.(md|markdown)$/i.test(p) ? 'md' : /\.txt$/i.test(p) ? 'txt' : /\.(png|jpe?g|gif)$/i.test(p) ? 'img' : /\.pdf$/i.test(p) ? 'pdf' : 'html')
  const ICON = { md: 'files', txt: 'files', img: 'image', pdf: 'files', html: 'web' } as const

  /** The newest artifact, previewed as a card when it is a page or an image. */
  const newest = $derived(artifacts?.[0] && (kind(artifacts[0].path) === 'html' || kind(artifacts[0].path) === 'img') ? artifacts[0] : null)
  let preview = $state<string | null>(null)
  $effect(() => {
    const n = newest
    if (!n) return void (preview = null)
    void base(true).then((b) => (preview = b ? `${address(b, n.path)}?v=${encodeURIComponent(n.modified ?? '')}` : null))
  })

  /* A page, over the agent's view */
  let page = $state<{ path: string; scratchpad: boolean; modified?: string | null; url: string | null; text: string | null } | null>(null)
  let nonce = $state(0)
  let busy = $state(false)
  async function show(path: string, scratchpad: boolean, modified?: string | null) {
    busy = true
    error = null
    try {
      const k = kind(path)
      const b = k === 'md' || k === 'txt' ? null : await base(scratchpad)
      const text = b ? null : await agentPageText(id, scratchpad ? `${SCRATCHPAD}/${path}` : path)
      page = { path, scratchpad, modified, url: b ? address(b, path) : null, text }
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  async function reload() {
    if (!page) return
    if (page.url) nonce++
    else await show(page.path, page.scratchpad, page.modified)
  }
  function ask() {
    if (!page) return
    // A prompt to the agent: in English, as Prism's other prompts.
    onask(page.scratchpad ? `About ${page.path} in your scratchpad: ` : `About ${page.path}: `)
    page = null
  }
</script>

<div class="web">
  {#if error}<p class="error">{error}</p>{/if}
  <div class="seg" role="tablist">
    <button role="tab" class:on={source === 'artifacts'} aria-selected={source === 'artifacts'} onclick={() => setSource('artifacts')}>{t('Artifacts')}<b>{artifacts?.length ?? '…'}</b></button>
    <button role="tab" class:on={source === 'folder'} aria-selected={source === 'folder'} onclick={() => setSource('folder')}>{t('Folder')}{#if pages}<b>{pages.length}</b>{/if}</button>
  </div>
  {#if source === 'artifacts'}
    {#if newest}
      <button class="card" onclick={() => show(newest.path, true, newest.modified)} disabled={busy}>
        <span class="thumb">
          {#if preview && kind(newest.path) === 'img'}<img src={preview} alt="" />
          {:else if preview}<iframe src={preview} title={newest.path} tabindex="-1" scrolling="no"></iframe>
          {:else}<Icon name="web" size={28} />{/if}
        </span>
        <span class="line">
          <span class="t"><span>{#if newest.modified && newest.modified > seenBefore}<i class="new"></i>{/if}{nameOf(newest.path)}</span><small>{dirOf(newest.path) || t('scratchpad')}</small></span>
          <span class="ago">{ago(newest.modified)}</span>
        </span>
      </button>
    {/if}
    {#each (artifacts ?? []).filter((a) => a !== newest) as a, i (a.path)}
      {#if i === 0 && newest}<p class="grp">{t('Earlier')}</p>{/if}
      <button class="it" onclick={() => show(a.path, true, a.modified)} disabled={busy}>
        <span class="ic {kind(a.path)}"><Icon name={ICON[kind(a.path)]} /></span>
        <span class="t"><span>{#if a.modified && a.modified > seenBefore}<i class="new"></i>{/if}{nameOf(a.path)}</span><small>{dirOf(a.path) || t('scratchpad')}</small></span>
        <span class="ago">{ago(a.modified)}</span>
      </button>
    {:else}
      {#if !newest}<p class="muted">{artifacts ? t('No artifact yet: the agent writes them in the scratchpad of its session.') : t('Loading…')}</p>{/if}
    {/each}
  {:else}
    <input class="search" type="search" bind:value={query} placeholder={t('Search the folder…')} spellcheck="false" aria-label={t('Search')} />
    <label class="switch"><input type="checkbox" checked={ignored} onchange={toggleIgnored} />{t('Also files git ignores')}</label>
    {#each groups as [dir, list] (dir)}
      <p class="dir"><Icon name="folder" />{dir || './'}</p>
      {#each list as p (p.path)}
        <button class="it" onclick={() => show(p.path, false)} disabled={busy}>
          <span class="ic {kind(p.path)}"><Icon name={ICON[kind(p.path)]} /></span>
          <span class="t"><span>{nameOf(p.path)}</span>{#if p.ignored}<small>{t('ignored by git')}</small>{/if}</span>
        </button>
      {/each}
    {:else}
      <p class="muted">{!pages ? t('Loading…') : query ? t('No page matches.') : ignored ? t('No Markdown, HTML or text page in its folder.') : t('No Markdown, HTML or text page in its folder (tracked or new files).')}</p>
    {/each}
  {/if}
</div>

{#if page}
  <div class="viewer">
    <div class="vbar">
      <button class="back" onclick={() => (page = null)}><Icon name="back" />{t('Web')}</button>
      <span class="name"><span>{nameOf(page.path)}</span><small>{page.scratchpad ? t('artifact') : dirOf(page.path) || './'}{page.modified ? ` · ${ago(page.modified)}` : ''}</small></span>
      <button class="b" onclick={reload} title={t('Reload')} aria-label={t('Reload')}><Icon name="refresh" /></button>
      {#if page.url}<a class="b" href={page.url} target="_blank" rel="noopener noreferrer" title={t('Open in the browser')} aria-label={t('Open in the browser')}><Icon name="external" /></a>{/if}
    </div>
    <div class="frame">
      {#if page.url}
        {#key nonce}<iframe src={nonce ? `${page.url}?r=${nonce}` : page.url} title={page.path}></iframe>{/key}
      {:else if kind(page.path) === 'md'}<div class="doc"><Markdown text={page.text ?? ''} reading /></div>
      {:else if kind(page.path) === 'html'}<iframe sandbox="" srcdoc={page.text ?? ''} title={page.path}></iframe>
      {:else}<pre class="txt">{page.text}</pre>{/if}
    </div>
    <div class="vfoot"><button onclick={ask}><Icon name="send" />{t('Ask a change…')}</button></div>
  </div>
{/if}

<style>
  .web { display: flex; flex-direction: column; gap: 6px; }
  .error { margin: 0; color: var(--ko); font-size: 13px; }
  .muted { margin: 4px 0; color: var(--text-muted); font-size: 12.5px; }
  .seg { display: flex; flex: none; padding: 2px; border-radius: var(--r10); background: var(--surface-muted); }
  .seg button { flex: 1; padding: 7px; border: 0; border-radius: var(--r8); background: none; color: var(--text-muted); font: inherit; font-size: 14px; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2); }
  .seg b { margin-left: 4px; font-weight: 500; opacity: 0.7; }
  .card, .it { display: flex; width: 100%; padding: 0; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); color: var(--text); font: inherit; text-align: left; }
  .card { flex-direction: column; overflow: hidden; }
  .thumb { position: relative; display: grid; place-items: center; height: 170px; overflow: hidden; border-bottom: 1px solid var(--border); background: var(--bg); color: var(--text-muted); }
  .thumb iframe { position: absolute; top: 0; left: 0; width: 200%; height: 200%; border: 0; transform: scale(0.5); transform-origin: 0 0; pointer-events: none; background: #fff; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; object-position: top; }
  .line, .it { align-items: center; gap: 10px; padding: 10px 12px; }
  .line { display: flex; }
  .t { display: flex; flex: 1; flex-direction: column; min-width: 0; font-size: 14px; }
  .t > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  small { overflow: hidden; color: var(--text-muted); font-size: 11.5px; text-overflow: ellipsis; white-space: nowrap; }
  .ago { flex: none; color: var(--text-muted); font-size: 11.5px; }
  .new { display: inline-block; width: 7px; height: 7px; margin-right: 6px; border-radius: 50%; background: var(--accent); vertical-align: 1px; }
  .ic { display: grid; flex: none; place-items: center; width: 34px; height: 34px; border-radius: var(--r8); background: var(--surface-muted); color: var(--text-muted); font-size: 16px; }
  .ic.html { background: color-mix(in srgb, var(--accent) 16%, transparent); color: var(--accent); }
  .ic.md, .ic.txt { background: color-mix(in srgb, var(--info) 16%, transparent); color: var(--info); }
  .ic.img { background: color-mix(in srgb, var(--ok) 16%, transparent); color: var(--ok); }
  .grp { margin: 8px 2px 0; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .search { padding: 8px 12px; border: 0; border-radius: var(--r10); background: var(--surface-muted); color: var(--text); font: inherit; font-size: 15px; }
  .switch { display: flex; align-items: center; gap: 8px; padding: 2px 2px 4px; color: var(--text-muted); font-size: 13px; }
  .switch input { width: 18px; height: 18px; margin: 0; }
  .dir { display: flex; align-items: center; gap: 6px; margin: 8px 2px 0; color: var(--text-muted); font: 12.5px var(--mono); }
  button:disabled { opacity: 0.5; }

  .viewer { position: fixed; inset: 0; z-index: 60; display: flex; flex-direction: column; padding-top: env(safe-area-inset-top); background: var(--bg); }
  .vbar { display: flex; flex: none; align-items: center; gap: 4px; padding: 6px 8px; border-bottom: 1px solid var(--border); background: var(--surface); }
  .back { display: flex; align-items: center; gap: 2px; padding: 6px; border: 0; background: none; color: var(--accent); font: inherit; font-size: 15px; }
  .name { display: flex; flex: 1; flex-direction: column; min-width: 0; }
  .name > span { overflow: hidden; font-size: 14px; font-weight: 600; text-overflow: ellipsis; white-space: nowrap; }
  .b { display: grid; flex: none; place-items: center; width: 38px; height: 38px; border: 0; border-radius: var(--r8); background: none; color: var(--text); font-size: 17px; }
  .frame { flex: 1; min-height: 0; overflow: auto; -webkit-overflow-scrolling: touch; }
  .frame iframe { display: block; width: 100%; height: 100%; border: 0; background: #fff; }
  .doc { padding: 12px 14px; }
  .txt { margin: 0; padding: 12px; white-space: pre-wrap; overflow-wrap: anywhere; font: 12px/1.5 var(--mono); }
  .vfoot { display: flex; flex: none; padding: 8px 12px calc(8px + env(safe-area-inset-bottom)); border-top: 1px solid var(--border); background: var(--surface); }
  .vfoot button { display: flex; flex: 1; align-items: center; justify-content: center; gap: 8px; padding: 10px; border: 1px solid var(--accent); border-radius: 11px; background: none; color: var(--accent); font: inherit; font-size: 14px; }
</style>
