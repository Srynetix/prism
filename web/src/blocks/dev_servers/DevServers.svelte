<script lang="ts">
  import { hasCore } from '../../bridge'
  import { armer } from '../../armed.svelte'
  import { agentsView, openAgents, openAgentShell, runInShell } from '../../agents.svelte'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import { devPreview, openDevServer } from '../../dev-preview.svelte'
  import { starredTasks } from '../../task-stars.svelte'
  import { taskFilesChanged } from '../../task-guard'
  import { ago } from '../../time'
  import { toast } from '../../toast.svelte'
  import { t, tn } from '../../i18n.svelte'
  import { withoutTicket } from '../trail/trail'
  import type { Agent } from '../../types'
  import { call, type BlockProps } from '../block'
  import type { DevServer, DevServersState, Listener } from './types'

  /**
   * Dev servers, by agent: what runs in its shells and on which port, a stopped one with its last
   * lines, then the agents running nothing (▶ starts the tasks starred in their Tasks panel), then
   * the ports held outside Prism. Kill takes two clicks (Tauri has no confirm()).
   */
  let { state: dev, error, snap }: BlockProps<DevServersState> = $props()
  const enabled = $derived(dev !== undefined)

  const arming = armer()
  let busy = $state<string | null>(null)
  let failed = $state<string | null>(null)
  async function act(key: string, f: () => Promise<unknown>) {
    busy = key
    failed = null
    try {
      await f()
    } catch (e) {
      failed = String(e)
    } finally {
      busy = null
    }
  }
  const refresh = () => call('dev_servers', 'refresh').catch(() => {})

  const agents = $derived((snap.agents ?? []) as Agent[])
  const servers = $derived(dev?.servers ?? [])
  /** The agents with something to show, in the agents' order; then those running nothing. */
  const groups = $derived.by(() => {
    const by = new Map<string, DevServer[]>()
    for (const s of servers) by.set(s.agent, [...(by.get(s.agent) ?? []), s])
    return agents.filter((a) => by.has(a.id)).map((a) => ({ a, servers: by.get(a.id)! }))
  })
  const idle = $derived(agents.filter((a) => a.state === 'running' && !servers.some((s) => s.agent === a.id)))
  const running = $derived(servers.filter((s) => s.state === 'running').length)
  const broken = $derived(servers.filter((s) => s.state === 'stopped' && s.failed))
  /** The agent's first web port: what Open opens. */
  const firstPort = (list: DevServer[]) => list.find((s) => s.state === 'running' && !s.docker && s.ports.length)?.ports[0]

  const tone = (s: DevServer) => (s.state === 'running' ? 'ok' : s.state === 'starting' ? 'warn' : s.failed ? 'ko' : '')
  const where = (s: DevServer) => (s.docker ? t('container') : s.shell !== null ? t('shell {n}', { n: s.shell }) : t('started by the agent'))
  const stars = (a: Agent) => starredTasks[a.cwd] ?? []

  const show = (s: DevServer) => (s.shell !== null ? openAgentShell(s.agent, s.shell) : openAgents(s.agent))
  const stop = (s: DevServer) => act(`stop:${s.key}`, () => call('dev_servers', 'stop', { key: s.key }))
  const again = (s: DevServer) =>
    act(`again:${s.key}`, async () => {
      await runInShell(s.agent, s.shell, s.command)
      await refresh()
    })
  const dismiss = (s: DevServer) => act(`dismiss:${s.key}`, () => call('dev_servers', 'dismiss', { key: s.key }))
  /**
   * The starred tasks, one new shell each; without a star, the Tasks panel to star some. When the
   * agent's branch changed what defines them (package.json, a Makefile, scripts/), a second click.
   */
  async function start(a: Agent) {
    const list = stars(a)
    if (!list.length) {
      agentsView.side = 'tasks'
      openAgents(a.id)
      return
    }
    if (!arming.is(`start:${a.id}`)) {
      const changed = await taskFilesChanged(a.id)
      if (changed.length) {
        arming.confirm(`start:${a.id}`)
        toast(t('{agent} changed {files}: check what its tasks run, then Start again', { agent: a.title, files: changed.slice(0, 3).join(', ') }), 6000)
        return
      }
    }
    // Armed and clicked again: your go, whatever changed.
    if (arming.is(`start:${a.id}`)) arming.confirm(`start:${a.id}`)
    void act(`start:${a.id}`, async () => {
      for (const task of list) await runInShell(a.id, null, task.command)
      await refresh()
    })
  }
  const kill = (l: Listener) => arming.confirm(`kill:${l.pid}`) && void act(`kill:${l.pid}`, () => call('dev_servers', 'kill', { pid: l.pid }))
  /** At the desk, its local address; on a phone, the remote's preview. */
  const open = (port: number) => openDevServer(port)
  const openTitle = (port: number) => (devPreview.phone ? t('Open :{port} on this phone (the remote’s preview)', { port }) : t('Open http://localhost:{port}', { port }))
</script>

<section class="panel" style="--src: var(--src-claude)" class:collapsed={isCollapsed('dev_servers')}>
  <h2>
    <span class="title"><Icon name="terminal" />{t('Dev servers')}</span>
    {#if dev}<small>{t('{n} running', { n: running })}{broken.length ? ` · ${tn(broken.length, '{n} stopped on an error', '{n} stopped on an error')}` : ''}</small>{/if}
    <BlockControls id="dev_servers" />
  </h2>
  {#if !enabled}
    <div class="empty">{t('The dev servers are not read. Turn on Settings › Agents › Dev servers, then restart Prism.')}</div>
  {:else if !dev?.read_at}
    <div class="empty">{error ? t('Cannot read the processes: {error}', { error }) : t('Reading the processes…')}</div>
  {:else if isShort('dev_servers')}
    <div class="short-body">
      {#each broken as s (s.key)}
        <div class="srow"><span class="t mono">{s.command}</span><span class="pill ko">{t('stopped')}</span></div>
      {:else}
        <div class="srow more">{t('{n} running', { n: running })}{dev.outside.length ? ` · ${tn(dev.outside.length, '{n} port outside Prism', '{n} ports outside Prism')}` : ''}</div>
      {/each}
    </div>
  {:else}
    {#if failed}<p class="err" title={failed}>{failed}</p>{/if}
    {#if error}<p class="err" title={error}>{error}</p>{/if}
    <div class="scroll">
      {#each groups as g (g.a.id)}
        {@const port = firstPort(g.servers)}
        <div class="agent" data-nav data-thread={`agent:${g.a.id}`}>
          <span class="dot {g.servers.some((s) => s.state === 'stopped' && s.failed) ? 'ko' : g.servers.some((s) => s.state === 'running') ? 'ok' : ''}"></span>
          <button class="who" onclick={() => openAgents(g.a.id)} title={t('Open the agent')}>
            {#if g.a.ticket}<span class="tk mono">{g.a.ticket}</span>{/if}
            <span class="tt">{g.a.ticket ? withoutTicket(g.a.title, g.a.ticket) : g.a.title}</span>
            {#if g.a.branch}<span class="br mono muted">{g.a.branch}</span>{/if}
          </button>
          {#if port}<button class="act" onclick={() => open(port)} title={openTitle(port)}>:{port} ↗</button>{/if}
        </div>
        {#each g.servers as s (s.key)}
          <div class="srv" class:stopped={s.state === 'stopped'}>
            <span class="dot {tone(s)}"></span>
            <span class="main">
              <span class="mono name" title={s.command}>{s.command}</span>
              <span class="sub muted">{where(s)}{s.state === 'starting' ? ` · ${t('no port yet')}` : ''}{s.state === 'stopped' ? ` · ${s.failed ? t('stopped on an error') : t('stopped')}` : ''}{s.since && s.state !== 'stopped' ? ` · ${ago(s.since)}` : ''}</span>
            </span>
            <span class="ports">
              {#each s.ports as p (p)}{#if s.state === 'running' || !devPreview.phone}<button class="port mono" onclick={() => open(p)} title={openTitle(p)}>:{p}</button>{:else}<span class="port mono">:{p}</span>{/if}{/each}
            </span>
            <span class="acts">
              {#if hasCore()}
                {#if !s.docker}<button class="icon" onclick={() => show(s)} title={s.shell !== null ? t('Show shell {n} in the agent’s panel', { n: s.shell }) : t('Open the agent')} aria-label={t('Show')}><Icon name="terminal" /></button>{/if}
                {#if s.state === 'stopped'}
                  {#if s.shell !== null && s.typed}<button class="icon" disabled={busy !== null} onclick={() => again(s)} title={t('Run {command} again in shell {n}', { command: s.command, n: s.shell })} aria-label={t('Run again')}><Icon name="retry" /></button>{/if}
                  <button class="icon" disabled={busy !== null} onclick={() => dismiss(s)} title={t('Forget this line')} aria-label={t('Dismiss')}><Icon name="close" /></button>
                {:else}
                  <button class="icon" disabled={busy !== null} onclick={() => stop(s)} title={s.docker ? 'docker stop' : s.shell !== null ? t('Stop: Ctrl-C in its shell') : t('Stop: SIGTERM')} aria-label={t('Stop')}><Icon name="stop" /></button>
                {/if}
              {/if}
            </span>
          </div>
          {#if s.state === 'stopped' && s.tail}<pre class="tail" class:ko={s.failed}>{s.tail}</pre>{/if}
        {/each}
      {/each}

      {#if idle.length}
        <div class="section">{t('Nothing running')} <span class="muted">{idle.length}</span></div>
        {#each idle as a (a.id)}
          {@const list = stars(a)}
          <div class="srv">
            <span class="dot"></span>
            <span class="main">
              <span class="name who-line">{#if a.ticket}<span class="tk mono">{a.ticket}</span><span class="tt">{withoutTicket(a.title, a.ticket)}</span>{:else}<span class="tt">{a.title}</span>{/if}</span>
              <span class="sub muted" title={list.map((task) => task.command).join('\n')}>{list.length ? list.map((task) => `★ ${task.name}`).join('  ') : t('No task starred: star the stack in its Tasks panel')}</span>
            </span>
            <span></span>
            <span class="acts">
              {#if hasCore()}
                <button class="act" class:primary={list.length > 0} class:armed={arming.is(`start:${a.id}`)} disabled={busy !== null} onclick={() => start(a)} title={list.length ? t('Start {tasks}, one shell each', { tasks: list.map((task) => task.name).join(', ') }) : t('Open its Tasks panel')}>
                  {busy === `start:${a.id}` ? '…' : list.length ? `▶ ${t('Start')}` : t('Tasks…')}
                </button>
              {/if}
            </span>
          </div>
        {/each}
      {/if}

      {#if !groups.length && !idle.length}
        <div class="empty">{t('No agent runs a server. Launch an agent, then run its dev task from its Tasks panel (★ to start it from here next time).')}</div>
      {/if}

      {#if dev.scan_outside && dev.outside.length}
        <div class="section">{t('Outside Prism')} <span class="muted">{dev.outside.length}</span></div>
        {#each dev.outside as l (`${l.pid}:${l.port}`)}
          <div class="srv">
            <span class="dot"></span>
            <span class="main">
              <span class="name">{l.program}</span>
              <span class="sub muted mono" title={l.command}>{l.docker ? `${t('container')} · ${l.command}` : `pid ${l.pid} · ${l.command}`}</span>
            </span>
            <span class="ports">{#if devPreview.phone}<span class="port mono" title={t('Outside Prism: not previewed from a phone')}>:{l.port}</span>{:else}<button class="port mono" onclick={() => open(l.port)} title={t('Open http://localhost:{port}', { port: l.port })}>:{l.port}</button>{/if}</span>
            <span class="acts">
              {#if hasCore()}
                <button class="act" class:armed={arming.is(`kill:${l.pid}`)} disabled={busy !== null} onclick={() => kill(l)} title={arming.is(`kill:${l.pid}`) ? t('Click again to stop it') : l.docker ? 'docker stop' : 'SIGTERM'}>
                  {arming.is(`kill:${l.pid}`) ? t('Sure?') : l.docker ? t('Stop') : t('Kill')}
                </button>
              {/if}
            </span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .err { margin: 0; padding: 4px 12px; color: var(--ko); font-size: var(--fs-meta); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .agent { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 6px 12px 3px; border-top: 1px solid var(--border); }
  .agent:first-child { border-top: none; }
  .who { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 0; border: none; background: none; color: var(--text); font: inherit; font-weight: 600; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .who:hover .tt { text-decoration: underline; }
  .tt, .br { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .br { flex-shrink: 1; min-width: 0; font-weight: 400; font-size: var(--fs-tag); }
  .br::before { content: '·'; margin-right: 8px; color: var(--text-muted); }
  /* The ticket as a chip, apart from the agent's title. */
  .tk { flex-shrink: 0; padding: 0 6px; border-radius: var(--r4); background: color-mix(in srgb, var(--info) 14%, transparent); color: var(--info); font-size: var(--fs-tag); font-weight: 500; line-height: 18px; white-space: nowrap; }
  .who-line { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .srv { display: grid; grid-template-columns: 8px minmax(0, 1fr) auto auto; gap: 8px; align-items: center; padding: 3px 12px 3px 26px; }
  .section + .srv, .section ~ .srv { padding-left: 12px; border-top: 1px solid var(--border); }
  .srv:hover { background: var(--surface-muted); }
  .main { display: flex; flex-direction: column; min-width: 0; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-mono); }
  .sub { font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ports { display: flex; gap: 4px; }
  .port { padding: 0 4px; border: none; border-radius: var(--r4); background: none; color: var(--info); cursor: pointer; }
  .port:hover { background: color-mix(in srgb, var(--info) 14%, transparent); }
  .tail { margin: 0 12px 6px 26px; padding: 5px 8px; max-height: 120px; overflow: auto; border-left: 3px solid var(--border); border-radius: var(--r4); background: var(--surface-muted); color: var(--text-muted); font: var(--fs-tag) / 1.45 var(--mono); white-space: pre-wrap; word-break: break-word; }
  .tail.ko { border-left-color: var(--ko); color: var(--text); }
  .section { padding: 4px 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); border-top: 1px solid var(--border); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); opacity: 0.5; }
  .dot.ok { background: var(--ok); opacity: 1; }
  .dot.warn { background: var(--warn); opacity: 1; }
  .dot.ko { background: var(--ko); opacity: 1; }
  .acts { display: flex; gap: 2px; justify-content: flex-end; }
  .act { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-tag); cursor: pointer; white-space: nowrap; }
  .act.primary { border-color: color-mix(in srgb, var(--src) 55%, var(--border)); color: var(--src); }
  .act.armed { border-color: var(--ko); color: var(--ko); }
  .act:disabled, .icon:disabled { opacity: 0.5; cursor: default; }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; padding: 0; border: 1px solid transparent; border-radius: var(--r5); background: none; color: var(--text-muted); cursor: pointer; }
  .icon:hover:not(:disabled) { color: var(--text); border-color: var(--border); }
</style>
