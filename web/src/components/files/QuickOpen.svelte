<script lang="ts">
  import { bufferOf, goToLine, openFile, tabsOf } from '../../files-editor.svelte'
  import { matchPath, parseAsk, rankPaths } from '../../editor/quick-open'
  import { symbolsOf } from '../../editor/symbols'
  import { draftOf, setDraft } from '../../chat-drafts.svelte'
  import { toast } from '../../toast.svelte'
  import { t } from '../../i18n.svelte'
  import type { AgentFile } from '../../agents.svelte'
  import type { Agent } from '../../types'

  /**
   * Quick open (⌘P) of an agent's Files tab: a file by letters of its path, open and recent ones
   * first, then the changed ones. `:42` goes to a line of the file shown, `@name` to one of its
   * symbols, `#words` searches every file. ↵ opens, ⌘↵ beside the file shown, ⇧↵ adds `@path`
   * to the agent's Chat prompt.
   */
  let { agent, files, onclose, onsearch }: { agent: Agent; files: AgentFile[]; onclose: () => void; onsearch: (text: string) => void } = $props()

  let input = $state('')
  let field: HTMLInputElement | undefined = $state()
  $effect(() => field?.focus())
  let selected = $state(0)
  const asked = $derived(parseAsk(input))
  const tabs = $derived(tabsOf(agent.id))
  const active = $derived(tabs.active)

  type Line = { kind: 'file'; path: string; at: number[]; status: string | null; group: string } | { kind: 'line'; line: number } | { kind: 'symbol'; name: string; from: number; depth: number; at: number[]; sort: string } | { kind: 'text'; query: string }

  const statusOf = $derived(new Map(files.map((f) => [f.path, f.status])))
  const lines = $derived.by((): Line[] => {
    const a = asked
    if (a.kind === 'line') return active ? [{ kind: 'line', line: a.line }] : []
    if (a.kind === 'text') return [{ kind: 'text', query: a.query }]
    if (a.kind === 'symbol') {
      const st = active ? bufferOf(agent.id, active)?.state : null
      if (!st) return []
      return symbolsOf(st)
        .map((s) => ({ s, m: matchPath(s.name, a.query) }))
        .filter((x) => x.m)
        .sort((x, y) => (a.query ? y.m!.score - x.m!.score : 0))
        .slice(0, 60)
        .map(({ s, m }) => ({ kind: 'symbol', name: s.name, from: s.from, depth: a.query ? 0 : s.depth, at: m!.at, sort: s.kind }))
    }
    const open = new Set(tabs.tabs.map((x) => x.path))
    const recent = tabs.recent
    const boost = (p: string) => {
      const r = recent.indexOf(p)
      return (r >= 0 ? 30 - r : 0) + (open.has(p) ? 10 : 0) + (statusOf.get(p) ? 4 : 0)
    }
    const paths = files.filter((f) => !f.ignored || a.query).map((f) => f.path)
    if (!a.query) {
      const first = [...new Set([...recent, ...tabs.tabs.map((x) => x.path)])].filter((p) => statusOf.has(p)).slice(0, 12)
      const changed = files.filter((f) => f.status && !first.includes(f.path)).map((f) => f.path).slice(0, 30)
      return [...first.map((path) => ({ kind: 'file' as const, path, at: [], status: statusOf.get(path) ?? null, group: t('Open & recent') })), ...changed.map((path) => ({ kind: 'file' as const, path, at: [], status: statusOf.get(path) ?? null, group: t('Changed') }))]
    }
    return rankPaths(paths, a.query, boost, 60).map((r) => ({ kind: 'file', path: r.path, at: r.at, status: statusOf.get(r.path) ?? null, group: '' }))
  })
  $effect(() => {
    void lines
    selected = 0
  })

  function choose(l: Line | undefined, how: 'open' | 'side' | 'prompt' = 'open') {
    if (!l) return
    if (l.kind === 'file') {
      if (how === 'prompt') {
        const now = draftOf(agent.id)
        setDraft(agent.id, `${now ? `${now.replace(/\s+$/, '')} ` : ''}@${l.path} `)
        toast(t('Added to the prompt of {name} (Chat)', { name: agent.title || agent.id }), 3000)
      } else openFile(agent.id, l.path, { side: how === 'side', line: asked.kind === 'file' ? asked.line ?? undefined : undefined })
    } else if (l.kind === 'line' && active) goToLine(agent.id, active, l.line)
    else if (l.kind === 'symbol' && active) {
      const st = bufferOf(agent.id, active)?.state
      if (st) goToLine(agent.id, active, st.doc.lineAt(l.from).number)
    } else if (l.kind === 'text') onsearch(l.query)
    onclose()
  }

  let list: HTMLDivElement | undefined = $state()
  function onkeydown(e: KeyboardEvent) {
    e.stopPropagation()
    if (e.key === 'Escape') onclose()
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      const n = lines.length
      if (n) selected = (selected + (e.key === 'ArrowDown' ? 1 : -1) + n) % n
      queueMicrotask(() => list?.querySelector('.it.on')?.scrollIntoView({ block: 'nearest' }))
    } else if (e.key === 'Enter') choose(lines[selected], e.metaKey || e.ctrlKey ? 'side' : e.shiftKey ? 'prompt' : 'open')
    else return
    e.preventDefault()
  }
  /** The text with its matched characters marked. */
  function marks(text: string, at: number[], offset = 0): { s: string; on: boolean }[] {
    const set = new Set(at.map((i) => i - offset))
    const out: { s: string; on: boolean }[] = []
    for (let i = 0; i < text.length; i++) {
      const on = set.has(i)
      if (out.length && out[out.length - 1].on === on) out[out.length - 1].s += text[i]
      else out.push({ s: text[i], on })
    }
    return out
  }
  const nameAt = (p: string) => p.lastIndexOf('/') + 1
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose}></div>
<div class="qo" role="dialog" aria-label={t('Open a file')}>
  <input bind:this={field} bind:value={input} spellcheck="false" autocomplete="off" placeholder={t('A file by its name · :line · @symbol · #text')} {onkeydown} />
  <div class="list" bind:this={list}>
    {#each lines as l, i (i)}
      {#if l.kind === 'file' && l.group && (i === 0 || (lines[i - 1].kind === 'file' && (lines[i - 1] as { group: string }).group !== l.group))}<div class="grp">{l.group}</div>{/if}
      <button class="it" class:on={i === selected} onmouseenter={() => (selected = i)} onclick={(e) => choose(l, e.metaKey || e.ctrlKey ? 'side' : 'open')}>
        {#if l.kind === 'file'}
          <span class="nm">{#each marks(l.path.slice(nameAt(l.path)), l.at, nameAt(l.path)) as m, k (k)}{#if m.on}<b>{m.s}</b>{:else}{m.s}{/if}{/each}</span>
          <span class="p">{#each marks(l.path.slice(0, nameAt(l.path)), l.at) as m, k (k)}{#if m.on}<b>{m.s}</b>{:else}{m.s}{/if}{/each}</span>
          {#if l.status}<span class="s s{l.status === '?' ? 'u' : l.status}">{l.status}</span>{/if}
        {:else if l.kind === 'line'}
          <span>{t('Go to line {line}', { line: l.line })}</span><span class="p">{active}</span>
        {:else if l.kind === 'symbol'}
          <span class="kind">{l.sort === 'function' ? 'ƒ' : l.sort === 'heading' ? '#' : l.sort === 'type' ? 'T' : 'C'}</span>
          <span class="nm" style:padding-left="{l.depth * 12}px">{#each marks(l.name, l.at) as m, k (k)}{#if m.on}<b>{m.s}</b>{:else}{m.s}{/if}{/each}</span>
        {:else}
          <span>{l.query ? t('Search “{text}” in every file', { text: l.query }) : t('Search in every file')}</span>
        {/if}
      </button>
    {:else}
      <p class="none">{asked.kind === 'symbol' ? (active ? t('No symbol found in this file.') : t('Open a file first.')) : asked.kind === 'line' ? t('Open a file first.') : t('No file matches.')}</p>
    {/each}
  </div>
  <div class="foot"><span>↑↓ {t('move')}</span><span>↵ {t('open')}</span><span>⌘↵ {t('beside')}</span><span>⇧↵ {t('to the prompt')}</span><span class="grow"></span><span>:{t('line')} · @{t('symbol')} · #{t('text')}</span></div>
</div>

<style>
  .scrim { position: absolute; inset: 0; z-index: 20; background: rgba(0, 0, 0, 0.18); }
  .qo { position: absolute; z-index: 21; left: 50%; top: 36px; width: min(540px, calc(100% - 24px)); transform: translateX(-50%); display: flex; flex-direction: column; max-height: calc(100% - 60px); border: 1px solid var(--border); border-radius: var(--r10, 10px); background: var(--surface); box-shadow: 0 16px 50px rgba(0, 0, 0, 0.25); overflow: hidden; font-size: var(--fs-meta); }
  input { padding: 9px 12px; border: none; border-bottom: 1px solid var(--border); background: none; color: var(--text); font: 13px var(--mono); outline: none; }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 3px 0; }
  .grp { padding: 6px 12px 2px; font-size: var(--fs-tag); color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
  .it { display: flex; align-items: baseline; gap: 8px; width: 100%; padding: 4px 12px; border: none; background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; white-space: nowrap; }
  .it.on { background: color-mix(in srgb, var(--src-claude) 14%, transparent); }
  .it b { color: var(--src-claude); font-weight: 700; }
  .nm { font-family: var(--mono); }
  .p { min-width: 0; overflow: hidden; text-overflow: ellipsis; font: var(--fs-tag) var(--mono); color: var(--text-muted); }
  .kind { width: 12px; color: var(--src-claude); font-family: var(--mono); }
  .s { margin-left: auto; width: 14px; text-align: center; font: 700 var(--fs-tag) var(--mono); }
  .sM, .sR { color: var(--warn); }
  .sA, .su { color: var(--ok); }
  .sD { color: var(--ko); }
  .none { margin: 0; padding: 10px 12px; color: var(--text-muted); }
  .foot { display: flex; gap: 12px; padding: 5px 12px; border-top: 1px solid var(--border); background: var(--surface-muted); font-size: var(--fs-tag); color: var(--text-muted); white-space: nowrap; overflow: hidden; }
  .grow { flex: 1; }
  :global(html.remote) input { font-size: 16px; }
  :global(html.remote) .it { padding-block: 8px; }
  :global(html.remote) .foot { display: none; }
</style>
