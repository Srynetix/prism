<script lang="ts">
  import { armer } from '../armed.svelte'
  import { onMount, untrack } from 'svelte'
  import Icon from './Icon.svelte'
  import Terminal from './Terminal.svelte'
  import { shellNumber, shellTabs, type AgentShellInfo, type ShellOwner } from '../agents.svelte'
  import { pollsShown } from '../polls.svelte'
  import { t } from '../i18n.svelte'

  /**
   * Login shells in a folder, for the commands of the moment (tests, a dev server, git), one tab
   * each: an agent's (its side panel's Shell tab) or a worktree's (the Worktrees view). tmux keeps
   * them, like an agent: switching tab or closing the pane only detaches the terminal, and they are
   * still there on return. `exit` or × closes one (a second click while it runs a program); +
   * opens another.
   */
  let { owner }: { owner: ShellOwner } = $props()

  let shells = $state<AgentShellInfo[]>([])
  let error = $state('')
  let loaded = $state(false)
  const active = $derived(shells.find((s) => s.n === shellTabs[owner.id]) ?? shells[0] ?? null)
  const select = (n: number) => (shellTabs[owner.id] = n)

  async function list() {
    try {
      const next = await owner.list()
      // Unchanged: left as is, not to redraw the tabs every few seconds.
      if (JSON.stringify(next) !== JSON.stringify(shells)) shells = next
      error = ''
    } catch (e) {
      error = String(e)
    }
  }
  /** A new shell (`null`), or shell `n` again; it becomes the shown one. */
  async function open(n: number | null = null) {
    error = ''
    try {
      select(shellNumber(await owner.open(n)))
      await list()
    } catch (e) {
      error = String(e)
    }
  }
  /** The tabs follow what each shell runs; the first opening starts a shell if there is none. */
  onMount(() => {
    void list().then(() => {
      loaded = true
      if (!shells.length && !error) void open()
    })
  })
  // Hidden (another view, the window): read again once shown.
  const shown = pollsShown()
  $effect(() => {
    if (!shown()) return
    // Shown again: at once (the first read is onMount's).
    if (untrack(() => loaded)) void list()
    const timer = setInterval(() => void list(), 3000)
    return () => clearInterval(timer)
  })

  /** × on a shell running a program arms first: a second click within 3 s closes it. */
  const armed = armer()
  async function close(s: AgentShellInfo) {
    if (s.busy && !armed.confirm(String(s.n))) return
    armed.disarm()
    try {
      await owner.close(s.n)
    } catch (e) {
      error = String(e)
    }
    await list()
  }
  /** `exit` typed in the shown shell: its session is gone. */
  function ended(n: number) {
    shells = shells.filter((s) => s.n !== n)
    void list()
  }
  const folder = $derived(owner.folder)
</script>

<div class="bar" role="tablist" aria-label={t('Shells of {name}', { name: owner.title })}>
  {#each shells as s (s.n)}
    <span class="tab" class:on={s.n === active?.n} class:busy={s.busy}>
      <button class="pick" role="tab" aria-selected={s.n === active?.n} onclick={() => select(s.n)} title="{s.busy ? t('Runs {command}', { command: s.command }) : t('At its prompt')} · {folder}">
        <span class="dot"></span>{s.n}<span class="cmd">{s.command}</span>
      </button>
      <button class="x" class:armed={armed.is(String(s.n))} onclick={() => close(s)} title={armed.is(String(s.n)) ? t('Click again: {command} stops with its shell', { command: s.command }) : t('Close shell {n}', { n: s.n })} aria-label={t('Close shell {n}', { n: s.n })}><Icon name="close" size={10} /></button>
    </span>
  {/each}
  <button class="plus" onclick={() => open()} title={t('A new shell in {folder}', { folder })} aria-label={t('New shell')}><Icon name="plus" /></button>
</div>
{#if error}<p class="err">{error}</p>{/if}
{#if active}
  {#key active.key}<div class="tw"><Terminal id={active.key} onend={() => ended(active.n)} /></div>{/key}
{:else if loaded}
  <p class="muted empty">{t('No shell open.')} <button class="link" onclick={() => open()}>{t('Open one in {folder}', { folder })}</button></p>
{/if}

<style>
  .bar { display: flex; align-items: center; gap: 4px; padding: 5px 8px; border-bottom: 1px solid var(--border); overflow-x: auto; }
  .tab { display: inline-flex; align-items: center; flex-shrink: 0; border: 1px solid var(--border); border-radius: var(--r6); }
  .tab.on { border-color: var(--src-claude); background: color-mix(in srgb, var(--src-claude) 10%, transparent); }
  .pick, .x, .plus { display: inline-flex; align-items: center; gap: 5px; padding: 2px 6px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .tab.on .pick { color: var(--text); }
  .cmd { max-width: 110px; overflow: hidden; text-overflow: ellipsis; font-family: var(--mono); color: var(--text-muted); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--text-muted); opacity: 0.5; }
  .busy .dot { background: var(--ok); opacity: 1; }
  .x { padding: 2px 4px; }
  .x:hover { color: var(--text); }
  .x.armed { background: var(--ko-bg); color: var(--ko); }
  .plus { flex-shrink: 0; border: 1px dashed var(--border); }
  .plus:hover { border-color: var(--src-claude); color: var(--src-claude); }
  .err { margin: 0; padding: 6px 10px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); white-space: pre-wrap; }
  .empty { padding: 12px; font-size: var(--fs-meta); }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; }
  .tw { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
