<script lang="ts">
  import { SearchQuery } from '@codemirror/search'
  import { invoke } from '../../bridge'
  import Icon from '../Icon.svelte'
  import { agentFilePreview, type AgentFile } from '../../agents.svelte'
  import { agentWorking, checkStamps, filesSettings, metaOf, openFile } from '../../files-editor.svelte'
  import { armer } from '../../armed.svelte'
  import { pref, setPref } from '../../layout.svelte'
  import { toast } from '../../toast.svelte'
  import { t, tn } from '../../i18n.svelte'
  import type { FileSearch } from '../../bindings/FileSearch'
  import type { FileQuery } from '../../bindings/FileQuery'
  import type { Agent } from '../../types'

  /**
   * Search in every file of the agent's folder (⌘⇧F): `git grep` on the Mac — tracked and
   * untracked files, `.gitignore` honoured, binaries skipped — as you type. Case, whole words, a
   * regular expression; globs to include or leave out (`src/**`, `!*.test.ts`); the changed files
   * only. A line found opens its file there. Replace all writes each file found (as it was read,
   * never while the agent works, nor over a file you edited and did not save).
   */
  let { agent, files, seed }: { agent: Agent; files: AgentFile[]; seed: { text: string; at: number } | null } = $props()

  let text = $state('')
  let regex = $state(pref<boolean>('agents.files.search.regex', false))
  let caseSensitive = $state(pref<boolean>('agents.files.search.case', false))
  let word = $state(pref<boolean>('agents.files.search.word', false))
  let include = $state(pref<string>('agents.files.search.include', ''))
  let changedOnly = $state(pref<boolean>('agents.files.search.changed', false))
  let replacing = $state(false)
  let replacement = $state('')
  let found = $state<FileSearch | null>(null)
  let busy = $state(false)
  let error = $state('')
  let took = $state(0)
  let folded = $state<Record<string, boolean>>({})
  const remember = (k: string, v: unknown) => setPref(`agents.files.search.${k}`, v)

  let input: HTMLInputElement | undefined = $state()
  $effect(() => {
    if (!seed) return
    text = seed.text
    queueMicrotask(() => input?.focus())
  })
  export const focus = () => input?.focus()

  const changedPaths = $derived(files.filter((f) => f.status && f.status !== 'D').map((f) => f.path))
  const query = $derived<FileQuery>({ text, regex, case: caseSensitive, word, include, paths: changedOnly ? changedPaths : null, max: filesSettings().search_max || 2000 })

  let seq = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  $effect(() => {
    const q = query
    clearTimeout(timer)
    if (!q.text) {
      found = null
      error = ''
      return
    }
    timer = setTimeout(() => void run(q), 250)
    return () => clearTimeout(timer)
  })
  async function run(q: FileQuery) {
    const n = ++seq
    busy = true
    const start = performance.now()
    try {
      const r = await invoke<FileSearch>('agent_files_search', { id: agent.id, query: q })
      if (n !== seq) return
      found = r
      error = ''
      took = Math.round(performance.now() - start)
    } catch (e) {
      if (n === seq) error = String(e)
    } finally {
      if (n === seq) busy = false
    }
  }

  const groups = $derived.by(() => {
    const out = new Map<string, { line: number; text: string }[]>()
    for (const m of found?.matches ?? []) {
      let g = out.get(m.path)
      if (!g) out.set(m.path, (g = []))
      g.push({ line: m.line, text: m.text })
    }
    return [...out]
  })

  /** The query as JavaScript reads it: to mark the matches, and to replace. */
  const pattern = $derived.by(() => {
    if (!text) return null
    try {
      const src = regex ? text : text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
      return new RegExp(word ? `\\b(?:${src})\\b` : src, caseSensitive ? 'g' : 'gi')
    } catch {
      return null
    }
  })
  function marks(line: string): { s: string; on: boolean }[] {
    const p = pattern
    if (!p) return [{ s: line, on: false }]
    const out: { s: string; on: boolean }[] = []
    let last = 0
    for (const m of line.matchAll(p)) {
      if (!m[0]) break
      out.push({ s: line.slice(last, m.index), on: false }, { s: m[0], on: true })
      last = m.index! + m[0].length
    }
    out.push({ s: line.slice(last), on: false })
    return out
  }

  function open(path: string, line: number) {
    openFile(agent.id, path, { line, search: new SearchQuery({ search: text, caseSensitive, regexp: regex, wholeWord: word, replace: replacement }) })
  }

  /* ---------- Replace all ---------- */

  const armed = armer()
  let writing = $state(false)
  const working = $derived(agentWorking(agent.id))
  async function replaceAll() {
    if (!pattern || !found || writing) return
    if (!armed.confirm('replace')) return
    writing = true
    let done = 0
    const skipped: string[] = []
    try {
      for (const [path] of groups) {
        if (metaOf(agent.id, path)?.dirty) {
          skipped.push(path)
          continue
        }
        const p = await agentFilePreview(agent.id, path)
        if (p.kind !== 'text') continue
        const next = regex ? p.text.replace(new RegExp(pattern.source, pattern.flags), replacement) : p.text.replace(new RegExp(pattern.source, pattern.flags), () => replacement)
        if (next === p.text) continue
        await invoke('agent_file_save', { id: agent.id, path, original: p.text, text: next })
        done++
      }
      toast(tn(done, '{n} file changed', '{n} files changed') + (skipped.length ? ` · ${tn(skipped.length, '{n} edited and not saved: left as it is', '{n} edited and not saved: left as they are')}` : ''), 5000)
    } catch (e) {
      toast(String(e), 6000)
    } finally {
      writing = false
      await checkStamps(agent.id)
      void run(query)
    }
  }
</script>

<div class="search">
  <div class="row">
    <button class="tw" onclick={() => (replacing = !replacing)} title={t('Replace')}><Icon name={replacing ? 'chevronDown' : 'chevronRight'} size={10} /></button>
    <input bind:this={input} bind:value={text} placeholder={t('Search in every file')} spellcheck="false" autocomplete="off" onkeydown={(e) => { if (e.key === 'Escape') text = '' }} />
    <button class="tog" class:on={caseSensitive} onclick={() => remember('case', (caseSensitive = !caseSensitive))} title={t('Match case')}>Aa</button>
    <button class="tog" class:on={word} onclick={() => remember('word', (word = !word))} title={t('Whole words')}>W</button>
    <button class="tog" class:on={regex} onclick={() => remember('regex', (regex = !regex))} title={t('Regular expression')}>.*</button>
  </div>
  {#if replacing}
    <div class="row in">
      <input bind:value={replacement} placeholder={regex ? t('Replace ($1 for a group)') : t('Replace')} spellcheck="false" autocomplete="off" />
      <button class="all" class:armed={armed.is('replace')} onclick={replaceAll} disabled={!found?.matches.length || working || writing || !pattern} title={working ? t('The agent works: its files are not written under it') : t('Replace in every file found (a second click confirms)')}>{writing ? t('Replacing…') : armed.is('replace') ? t('Sure?') : t('All')}</button>
    </div>
  {/if}
  <div class="row in">
    <input bind:value={include} oninput={() => remember('include', include)} placeholder={t('Files to include · src/**, !*.test.ts')} spellcheck="false" autocomplete="off" />
    <button class="tog wide" class:on={changedOnly} onclick={() => remember('changed', (changedOnly = !changedOnly))} title={t('Only the files git sees as changed')}>{t('Changed')}</button>
  </div>
  {#if error}<p class="err">{error}</p>{/if}
  {#if found}
    <div class="sum">
      {#if busy}<Icon name="refresh" spin size={10} />{/if}
      {found.matches.length ? `${tn(found.matches.length, '{n} result', '{n} results')} · ${tn(groups.length, '{n} file', '{n} files')}` : t('No result')}{found.truncated ? ` · ${t('cut, narrow the search')}` : ''} · {took} ms
    </div>
  {/if}
  <div class="res">
    {#each groups as [path, lines] (path)}
      <button class="rf" onclick={() => (folded = { ...folded, [path]: !folded[path] })} title={path}>
        <Icon name={folded[path] ? 'chevronRight' : 'chevronDown'} size={10} />
        <span class="nm">{path.slice(path.lastIndexOf('/') + 1)}</span>
        <span class="p">{path.slice(0, path.lastIndexOf('/') + 1)}</span>
        <span class="c">{lines.length}</span>
      </button>
      {#if !folded[path]}
        {#each lines as l (l.line)}
          <button class="rl" onclick={() => open(path, l.line)}><span class="ln">{l.line}</span><span class="t">{#each marks(l.text.trimStart()) as m, k (k)}{#if m.on}<mark>{m.s}</mark>{:else}{m.s}{/if}{/each}</span></button>
        {/each}
      {/if}
    {/each}
  </div>
</div>

<style>
  .search { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: 4px; padding: 6px 8px 0; }
  .row.in { padding-left: 24px; }
  .row:last-of-type { padding-bottom: 6px; }
  input { flex: 1; min-width: 0; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: var(--fs-meta) var(--mono); outline: none; }
  input:focus { border-color: var(--src-claude); }
  .tw { display: inline-flex; padding: 0 2px; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .tog, .all { padding: 1px 5px; border: 1px solid transparent; border-radius: var(--r4); background: none; color: var(--text-muted); font: 600 var(--fs-tag) var(--mono); cursor: pointer; }
  .tog.wide { font-family: var(--sans); font-weight: 400; border-color: var(--border); }
  .tog.on { border-color: var(--src-claude); color: var(--src-claude); background: color-mix(in srgb, var(--src-claude) 8%, transparent); }
  .all { border-color: var(--border); font-family: var(--sans); font-weight: 400; font-size: var(--fs-meta); }
  .all.armed { border-color: var(--ko); color: var(--ko); }
  .all:disabled { opacity: 0.5; cursor: default; }
  .err { margin: 6px 8px 0; padding: 4px 8px; border-radius: var(--r4); background: var(--ko-bg); color: var(--ko); font-size: var(--fs-tag); white-space: pre-wrap; }
  .sum { display: flex; align-items: center; gap: 4px; padding: 4px 10px; border-block: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); }
  .res { flex: 1; min-height: 0; overflow: auto; padding: 3px 4px 8px; }
  .rf, .rl { display: flex; align-items: center; gap: 5px; width: 100%; border: none; border-radius: var(--r4); background: none; color: var(--text); text-align: left; cursor: pointer; white-space: nowrap; }
  .rf { padding: 3px 6px; font: 600 var(--fs-meta) var(--sans); }
  .rf .p { min-width: 0; overflow: hidden; text-overflow: ellipsis; font: var(--fs-tag) var(--mono); color: var(--text-muted); font-weight: 400; direction: rtl; text-align: left; }
  .rf .c { margin-left: auto; padding: 0 5px; border-radius: 8px; background: var(--surface); font-size: var(--fs-tag); color: var(--text-muted); font-weight: 400; }
  .rl { padding: 1px 6px 1px 20px; font: var(--fs-tag)/1.6 var(--mono); overflow: hidden; }
  .rf:hover, .rl:hover { background: color-mix(in srgb, var(--src-claude) 10%, transparent); }
  .rl .ln { flex: 0 0 26px; text-align: right; color: var(--text-muted); opacity: 0.7; }
  .rl .t { overflow: hidden; text-overflow: ellipsis; }
  mark { background: color-mix(in srgb, var(--warn) 35%, transparent); color: inherit; border-radius: 2px; }
  :global(html.remote) input { font-size: 16px; }
  :global(html.remote) .rl { padding-block: 6px; }
</style>
