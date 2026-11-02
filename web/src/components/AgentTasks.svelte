<script lang="ts">
  import Icon from './Icon.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { agentShells, agentTasks, runInShell, shellNumber, shellTabs, type AgentTask, type TaskSource } from '../agents.svelte'
  import { isStarred, toggleStar } from '../task-stars.svelte'
  import type { Agent } from '../types'
  import { t } from '../i18n.svelte'

  /**
   * The Tasks tab of an agent's side panel: what its folder declares — `just` recipes, `task`
   * tasks, `package.json` scripts — in the folder and its first-level subfolders. A click runs
   * one in the shell shown in the Shell tab (otherwise the first one at its prompt, otherwise a
   * new one) and shows it; a task that needs arguments is only typed there, left to finish. A busy
   * shell (a dev server) is not interrupted without asking: Ctrl-C it, or run in a new shell.
   * A file's tasks fold under its header (a click; ⌥-click: every file), remembered by its path
   * in the folder, so for every agent of the same repository; a filter shows them all. A star
   * marks a task as part of the repository's stack: the Dev servers block starts the starred ones.
   */
  let { agent, onrun }: { agent: Agent; onrun: () => void } = $props()

  let sources = $state<TaskSource[] | null>(null)
  let error = $state('')
  let busy = $state(false)
  let query = $state('')
  async function load() {
    busy = true
    error = ''
    try {
      sources = await agentTasks(agent.id)
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  $effect(() => {
    if (!sources && !error && !busy) void load()
  })

  let folded = $state(new Set(pref<string[]>('agents.tasks.folded', [])))
  function fold(file: string, all: boolean) {
    const closing = !folded.has(file)
    const next = new Set(folded)
    for (const f of all ? (sources ?? []).map((s) => s.file) : [file]) {
      if (closing) next.add(f)
      else next.delete(f)
    }
    folded = next
    setPref('agents.tasks.folded', [...next])
  }
  const isFolded = (file: string) => !query.trim() && folded.has(file)

  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase()
    return (sources ?? []).map((s) => ({ ...s, tasks: s.tasks.filter((tk) => !q || tk.name.toLowerCase().includes(q) || tk.desc?.toLowerCase().includes(q)) })).filter((s) => !q || s.tasks.length)
  })

  /** Where a task runs: the shown shell, otherwise the first at its prompt, otherwise a new one (`null`). */
  async function target(): Promise<number | null> {
    const shells = await agentShells(agent.id)
    return shells.find((s) => s.n === shellTabs[agent.id])?.n ?? shells.find((s) => !s.busy)?.n ?? null
  }
  /** The task refused because its shell runs something: offered to Ctrl-C it, or to use a new shell. */
  let held = $state<{ task: AgentTask; n: number; reason: string } | null>(null)
  async function run(task: AgentTask, how: { n?: number | null; interrupt?: boolean } = {}) {
    error = ''
    held = null
    let n: number | null = null
    try {
      n = how.n !== undefined ? how.n : await target()
      shellTabs[agent.id] = shellNumber(await runInShell(agent.id, n, task.command, { enter: !task.args, interrupt: how.interrupt }))
      onrun()
    } catch (e) {
      const reason = String(e)
      if (n !== null && !how.interrupt && /busy with/.test(reason)) held = { task, n, reason }
      else error = reason
    }
  }
  const KIND: Record<TaskSource['kind'], string> = { just: 'justfile', task: 'Taskfile', npm: 'scripts' }
</script>

<div class="bar">
  <input bind:value={query} placeholder={t('Filter tasks')} spellcheck="false" />
  <button onclick={load} disabled={busy} title={t('Read the task files again')}><Icon name="refresh" spin={busy} /></button>
</div>
{#if error}<p class="err">{error}</p>{/if}
{#if held}
  <p class="ask">
    {t('Shell {n} is busy ({what}).', { n: held.n, what: held.reason.replace(/^.*busy with /, '') })}
    <button class="link" onclick={() => run(held!.task, { n: null })}>{t('Run {task} in a new shell', { task: held.task.name })}</button>
    · <button class="link" onclick={() => run(held!.task, { n: held!.n, interrupt: true })}>{t('Ctrl-C it and run')}</button>
    · <button class="link" onclick={() => { shellTabs[agent.id] = held!.n; onrun() }}>{t('See it')}</button>
  </p>
{/if}
<div class="list">
  {#each shown as s (s.file)}
    {@const shut = isFolded(s.file)}
    <button class="grp" onclick={(e) => fold(s.file, e.altKey)} aria-expanded={!shut} title="{s.file} · {shut ? t('Show its tasks (⌥-click: every file)') : t('Hide its tasks (⌥-click: every file)')}">
      <span class="chev"><Icon name={shut ? 'chevronRight' : 'chevronDown'} size={11} /></span>
      <span class="k">{KIND[s.kind]}</span>
      <span class="f">{s.file}</span>
      {#if shut}<span class="c">{s.tasks.length}</span>{/if}
      <span class="r">{s.runner}</span>
    </button>
    {#if s.note && !shut}<p class="note" title={s.note}>{s.note}{s.tasks.length ? ` · ${t('read by Prism')}` : ''}</p>{/if}
    {#each shut ? [] : s.tasks as tk (tk.command)}
      {@const starred = isStarred(agent.cwd, s.file, tk.name)}
      <div class="trow">
        <button class="task" onclick={() => run(tk)} title="{tk.command}{tk.args ? ` … ${t('(needs arguments: typed into the shell, to finish there)')}` : ''}">
          <Icon name={tk.args ? 'edit' : 'play'} size={12} />
          <span class="n">{tk.name}</span>
          {#if tk.desc}<span class="d">{tk.desc}</span>{/if}
        </button>
        {#if !tk.args}
          <button class="star" class:on={starred} onclick={() => toggleStar(agent.cwd, { file: s.file, name: tk.name, command: tk.command })} aria-pressed={starred} title={starred ? t('Part of the stack: the Dev servers block starts it. Click to unstar') : t('Star: part of this repository’s stack, started by the Dev servers block')}>{starred ? '★' : '☆'}</button>
        {/if}
      </div>
    {/each}
  {:else}
    {#if sources}
      <p class="muted empty">{query.trim() ? t('No task matches.') : t('No justfile, Taskfile nor package.json scripts in this folder or its first-level subfolders.')}</p>
    {/if}
  {/each}
</div>

<style>
  .bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); }
  .bar input { flex: 1; min-width: 0; padding: 3px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: var(--fs-meta) var(--mono); outline: none; }
  .bar input:focus { border-color: var(--src-claude); }
  .bar button { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .err { margin: 0; padding: 6px 10px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .ask { margin: 0; padding: 6px 10px; background: var(--warn-bg); color: var(--warn); font-size: var(--fs-meta); }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .empty { padding: 12px; font-size: var(--fs-meta); }
  .list { flex: 1; min-height: 0; overflow: auto; padding: 4px; display: flex; flex-direction: column; }
  .grp { display: flex; align-items: center; gap: 6px; margin: 6px 0 1px; padding: 2px 6px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .grp:hover { background: var(--surface-muted); }
  .grp .chev { display: inline-flex; color: var(--text-muted); }
  .grp .c { padding: 0 5px; border-radius: var(--r8); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }
  .grp .k { font-size: 10px; text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .grp .f { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); }
  .grp .r { margin-left: auto; color: var(--text-muted); font-family: var(--mono); }
  .note { margin: 0 6px 3px; color: var(--warn); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .trow { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
  .star { padding: 0 6px; border: none; background: none; color: var(--text-muted); opacity: 0.45; font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .trow:hover .star, .star.on { opacity: 1; }
  .star.on { color: var(--warn); }
  .task { display: grid; grid-template-columns: 14px auto minmax(0, 1fr); align-items: center; gap: 6px; padding: 2px 6px; border: none; border-radius: var(--r4); background: none; color: var(--text); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .task:hover { background: var(--surface-muted); }
  .task :global(svg) { color: var(--src-claude); }
  .task .n { font-family: var(--mono); font-weight: 600; white-space: nowrap; }
  .task .d { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
</style>
