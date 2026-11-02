<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { t } from '../i18n.svelte'
  import AgentWeb from './AgentWeb.svelte'
  import { agentShell, agentShells, agentTasks, closeShell, runInShell, type AgentShellInfo, type TaskSource } from '../agents.svelte'
  import { socketState } from './socket.svelte'
  import { openShellView } from './shell-view.svelte'

  /**
   * An agent's tools on a phone, beside its terminal: its shells (watched, taken as its terminal
   * is: `[remote] typing` and the passkey), its folder's tasks (run as detected, never a line of
   * the phone's: a critical action, the passkey each time), its pages and artifacts (AgentWeb).
   */
  let { id, tab, onask }: { id: string; tab: 'shell' | 'tasks' | 'web'; onask: (text: string) => void } = $props()

  const acts = $derived(socketState.hello?.classes.includes('act') ?? false)
  const critical = $derived(socketState.hello?.classes.includes('critical') ?? false)
  let error = $state<string | null>(null)
  let busy = $state(false)
  async function act<T>(f: () => Promise<T>): Promise<T | undefined> {
    busy = true
    error = null
    try {
      return await f()
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      busy = false
    }
  }

  /* Shells */
  let shells = $state<AgentShellInfo[] | null>(null)
  async function loadShells() {
    shells = (await agentShells(id).catch((e) => ((error = String(e)), []))) ?? []
  }
  $effect(() => {
    if (tab !== 'shell') return
    void loadShells()
    const timer = setInterval(loadShells, 5000)
    return () => clearInterval(timer)
  })
  async function openShell(n: number | null) {
    // Reopened (after an `exit`) when the phone may act; otherwise watched as it is.
    const key = acts ? await act(() => agentShell(id, n)) : n !== null ? `shell:${id}:${n}` : undefined
    if (key) openShellView(key, t('Shell {n}', { n: key.split(':').pop() ?? '' }))
  }
  async function close(n: number) {
    await act(() => closeShell(id, n))
    await loadShells()
  }

  /* Tasks */
  let sources = $state<TaskSource[] | null>(null)
  $effect(() => {
    if (tab !== 'tasks' || sources) return
    agentTasks(id).then((s) => (sources = s)).catch((e) => ((error = String(e)), (sources = [])))
  })
  async function run(name: string, command: string) {
    const key = await act(() => runInShell(id, null, command))
    if (key) openShellView(key, name)
  }
</script>

<div class="tools">
  {#if error}<p class="error">{error}</p>{/if}
  {#if tab === 'shell'}
    {#each shells ?? [] as s (s.key)}
      <div class="row">
        <button class="main" onclick={() => openShell(s.n)}><Icon name="terminal" /><span>{t('Shell {n}', { n: s.n })}<small>{s.command}{s.busy ? ` · ${t('running')}` : ''}</small></span></button>
        {#if critical}<button class="x" onclick={() => close(s.n)} disabled={busy} title={t('Close this shell, whatever it runs (passkey)')} aria-label={t('Close shell {n}', { n: s.n })}>×</button>{/if}
      </div>
    {:else}
      <p class="muted">{shells ? t('No shell open for this agent.') : t('Loading…')}</p>
    {/each}
    {#if acts}<button class="add" onclick={() => openShell(null)} disabled={busy}><Icon name="add" />{t('New shell')}</button>{/if}
    <p class="muted">{t('Watched read-only;')} <b>{t('Take')}</b> {t('types in it (Settings › Remote › typing, passkey).')}</p>
  {:else if tab === 'tasks'}
    {#each sources ?? [] as s (s.file)}
      <p class="src">{s.file}<small>{s.runner}</small></p>
      {#each s.tasks as task (task.command)}
        <button class="task" onclick={() => run(task.name, task.command)} disabled={busy || !critical} title={task.command}>
          <Icon name="play" /><span>{task.name}<small>{task.desc ?? task.command}{task.args ? ` · ${t('needs arguments: typed, to finish in the shell')}` : ''}</small></span>
        </button>
      {/each}
    {:else}
      <p class="muted">{sources ? t('No task declared in this folder (justfile, Taskfile, package.json).') : t('Loading…')}</p>
    {/each}
    {#if sources?.length && !critical}<p class="muted">{t('Running a task from a phone is a critical action: Settings › Remote › critical.')}</p>{/if}
  {:else}
    <AgentWeb {id} {onask} />
  {/if}
</div>

<style>
  .tools { flex: 1; min-height: 0; overflow: auto; padding: 10px 12px calc(12px + env(safe-area-inset-bottom)); background: var(--bg); color: var(--text); display: flex; flex-direction: column; gap: 6px; }
  .error { margin: 0; color: var(--ko); font-size: 13px; }
  .muted { margin: 4px 0; color: var(--text-muted); font-size: 12.5px; }
  .row { display: flex; gap: 6px; }
  .main, .task, .add { display: flex; align-items: center; gap: 10px; width: 100%; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); color: var(--text); font: inherit; font-size: 14px; text-align: left; }
  .main span, .task span { display: flex; flex-direction: column; min-width: 0; }
  small { color: var(--text-muted); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { flex: none; width: 42px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); color: var(--ko); font-size: 18px; }
  .add { justify-content: center; color: var(--info); }
  .src { margin: 8px 0 2px; font: 12px ui-monospace, monospace; color: var(--text-muted); display: flex; gap: 8px; }
  button:disabled { opacity: 0.5; }
</style>
