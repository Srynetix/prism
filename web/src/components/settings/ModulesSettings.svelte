<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../Icon.svelte'
  import ArmedLabel from '../ArmedLabel.svelte'
  import Field from './Field.svelte'
  import Control from './Control.svelte'
  import { blockLabel } from '../../views'
  import { moduleStates } from '../../modules/modules.svelte'
  import { armer } from '../../armed.svelte'
  import { invoke, hasCore, machine } from '../../bridge'
  import { setValue, valueOf } from '../../settings.svelte'
  import { checkModules, moduleUpdates } from '../../updates.svelte'
  import { toast } from '../../toast.svelte'
  import { t, tn } from '../../i18n.svelte'
  import type { ModuleRun } from '../../bindings/ModuleRun'
  import type { RemovedModule } from '../../bindings/RemovedModule'

  /**
   * Settings › Packs › Modules: a team's blocks loaded at run time (`modules/<id>/` next to
   * config.toml, a pack's, installed by hand or put there), as this window loaded them: their
   * blocks, or why not. Nothing of a module runs before I trust it: what it may reach is shown,
   * what a new version changes too. A module that crashed is started again from here, read again
   * from where it came from, or removed; one is installed without a pack. A module added,
   * replaced or trusted shows once the window reloads.
   */
  const list = $derived(moduleStates())
  const armed = armer()
  /** Trusted from here: what is left to do (reload; restart Prism for a back end installed after launch). */
  let trusted = $state<Record<string, 'reload' | 'restart'>>({})
  /** Done from here, by module: what happened and what is left to do. */
  let done = $state<Record<string, string>>({})
  let busy = $state('')
  /** How the back ends run in this Prism, and the pack modules I removed. */
  let runs = $state<ModuleRun[]>([])
  let removed = $state<RemovedModule[]>([])
  let source = $state('')
  let installed = $state('')
  /** This computer's Prism: a phone or another computer's window changes none of its modules. */
  const local = () => hasCore() && !machine()
  const runOf = (id: string) => runs.find((r) => r.id === id)
  const updateOf = (id: string) => moduleUpdates.list.find((c) => c.id === id && c.changed && c.installed && !c.error)

  async function refresh() {
    if (!local()) return
    try {
      ;[runs, removed] = await Promise.all([invoke<ModuleRun[]>('modules_runs'), invoke<RemovedModule[]>('modules_removed')])
    } catch (e) {
      console.error('modules:', e)
    }
  }
  onMount(() => {
    void refresh()
    void checkModules()
  })

  /** What a capability lets a module's back end reach. */
  function capability(c: string): string {
    if (c === 'git') return t('git, reading, in your clones')
    if (c === 'gitlab') return t('GitLab’s API, as you (glab)')
    if (c.startsWith('http:')) return t('HTTP requests to {host}', { host: c.slice(5) })
    if (c.startsWith('exec:')) return t('runs the program {program}', { program: c.slice(5) })
    return c
  }

  async function trust(id: string, fingerprint: string) {
    if (!armed.confirm(id)) return
    busy = id
    try {
      const running = await invoke<boolean>('modules_trust', { id, fingerprint })
      trusted[id] = running ? 'reload' : 'restart'
      toast(t('Module {id} trusted', { id }))
    } catch (e) {
      toast(String(e))
    } finally {
      busy = ''
    }
  }

  /** Started again, read again from its source, removed or put back: two clicks each. */
  async function act(kind: 'reset' | 'reinstall' | 'remove' | 'restore', id: string) {
    if (!armed.confirm(`${kind}:${id}`)) return
    busy = id
    try {
      const out = await invoke<boolean | null>(`modules_${kind}`, { id })
      done[id] =
        kind === 'reset' ? (out ? t('Started again.') : t('Runs again once Prism restarts.'))
        : kind === 'remove' ? t('Removed: reload the window; its back end stops for good once Prism restarts.')
        : out ? t('Installed again, trusted (a pack’s new version reaching nothing more): reload the window.')
        : t('Installed again: reload the window, then trust it.')
      toast(kind === 'reset' ? t('Module {id} started again', { id }) : kind === 'remove' ? t('Module {id} removed', { id }) : t('Module {id} installed', { id }))
      await refresh()
      void checkModules()
    } catch (e) {
      toast(String(e))
    } finally {
      busy = ''
    }
  }

  async function install() {
    const s = source.trim()
    if (!s || busy) return
    busy = '+install'
    try {
      const id = await invoke<string>('modules_install', { source: s })
      source = ''
      installed = t('Module {id} installed: reload the window to look at what it reaches and trust it; its back end runs once Prism restarts.', { id })
      toast(t('Module {id} installed', { id }))
      void checkModules()
    } catch (e) {
      toast(String(e))
    } finally {
      busy = ''
    }
  }

  const when = (at: number) => new Date(at).toLocaleString()
</script>

<div class="card">
  <div class="ch"><b>{t('Modules')}</b><small>{t('a team’s blocks loaded without rebuilding Prism, from modules/ next to config.toml')}</small></div>
  {#each list as m (m.id)}
    {@const run = runOf(m.id)}
    {@const update = updateOf(m.id)}
    {@const from = !!(m.info?.pack || m.info?.source)}
    <div class="module">
      <div class="minfo">
        <b>{m.id}{#if m.version}<span class="ver mono">v{m.version}</span>{/if}</b>
        {#if m.description}<small>{m.description}</small>{/if}
        {#if m.info?.pack}<small>{t('From the pack {pack}.', { pack: m.info.pack })}</small>
        {:else if m.info?.source}<small>{t('Installed by hand from')} <span class="mono">{m.info.source}</span>.</small>{/if}
        {#if m.state === 'on'}
          <small>{m.blocks.length ? t('Its blocks: {names}', { names: m.blocks.filter((b) => !m.clashes.includes(b)).map(blockLabel).join(', ') || '—' }) : t('No block.')}</small>
        {:else if m.state === 'waiting' && m.info}
          {#if trusted[m.id]}
            <span class="hint new"><Icon name="check" /> {trusted[m.id] === 'reload' ? t('Trusted: reload the window to load it.') : t('Trusted: reload the window, and restart Prism for its back end (installed since Prism started).')}</span>
          {:else}
            <span class="hint warn"><Icon name="lock" /> {m.info.trusted_version ? t('A new version waits for your trust (v{version} was trusted): nothing of it runs until then.', { version: m.info.trusted_version }) : t('Waits for your trust: nothing of it runs until then.')}</span>
            {#if m.info.capabilities.length}
              <small>{t('Its back end may reach:')}</small>
              <ul class="caps">{#each m.info.capabilities as c (c)}<li>{capability(c)}</li>{/each}</ul>
            {:else if m.info.backend}
              <small>{t('Its back end reaches nothing besides its own blocks.')}</small>
            {/if}
            {#if m.info.trusted_version && m.info.changes.length}
              <small>{t('What this version changes:')}</small>
              <pre class="changes mono">{#each m.info.changes as line, i (i)}<span class:add={line.startsWith('+')} class:del={line.startsWith('-')}>{line}</span>{'\n'}{/each}</pre>
            {/if}
          {/if}
        {:else}
          <span class="hint warn"><Icon name="warning" /> {m.state === 'refused' ? t('Not loaded: {reason}', { reason: m.reason ?? '' }) : t('Failed to load: {reason}', { reason: m.reason ?? '' })}</span>
        {/if}
        {#if !done[m.id]}
          {#if run?.held}<span class="hint warn"><Icon name="warning" /> {run.held}</span>
          {:else if m.info?.stopped}<span class="hint warn"><Icon name="warning" /> {t('Held: its back end stopped Prism the last time it ran. Start it again once it is fixed, or reinstall it.')}</span>{/if}
          {#if run?.crashes && run.last_crash}<small class="crash">{tn(run.crashes, 'Crashed once in the last minutes, at {time}: {message}', 'Crashed {n} times in the last minutes, the last at {time}: {message}', { time: when(run.last_crash_at ?? 0), message: run.last_crash })}</small>{/if}
        {/if}
        {#if update}<span class="hint new"><Icon name="up" /> {update.latest && update.latest !== update.version ? t('Version {version} at its source: Reinstall gets it.', { version: update.latest }) : t('Changed at its source: Reinstall gets it.')}</span>{/if}
        {#if done[m.id]}<span class="hint new"><Icon name="check" /> {done[m.id]}</span>{/if}
        {#each m.clashes as block (block)}<span class="hint warn"><Icon name="warning" /> {t('Its block {block} is not shown: that id is already taken.', { block })}</span>{/each}
      </div>
      <div class="side">
        {#if m.state === 'waiting' && m.info && !trusted[m.id] && local()}
          <button class="btn" class:armed={armed.is(m.id)} disabled={busy === m.id} onclick={() => trust(m.id, m.info!.fingerprint)} title={t('Run this module’s web part and back end on this computer (click twice)')}><ArmedLabel label={t('Trust')} armedLabel={t('Trust it?')} armed={armed.is(m.id)} /></button>
        {:else}
          <span class="state" class:on={m.state === 'on' && !m.clashes.length && !run?.held} class:wait={m.state === 'waiting'}>{run?.held || m.info?.stopped ? t('Held') : m.state === 'on' ? t('Loaded') : m.state === 'waiting' ? (trusted[m.id] ? t('Trusted') : t('Waiting')) : m.state === 'refused' ? t('Refused') : t('Failed')}</span>
        {/if}
        {#if local() && m.info && !done[m.id]}
          <span class="acts">
            {#if m.info.backend && m.info.trusted && (run?.held || run?.crashes || m.info.stopped)}
              <button class="btn" class:armed={armed.is(`reset:${m.id}`)} disabled={busy === m.id} onclick={() => act('reset', m.id)} title={t('Start its back end again from nothing: its crashes forgotten, its blocks read again (click twice)')}><ArmedLabel label={t('Start again')} armedLabel={t('Start it?')} armed={armed.is(`reset:${m.id}`)} /></button>
            {/if}
            {#if from}
              <button class="btn" class:primary={!!update} class:armed={armed.is(`reinstall:${m.id}`)} disabled={busy === m.id} onclick={() => act('reinstall', m.id)} title={m.info.pack ? t('Read it again from the pack {pack}’s source and put it back whole (click twice)', { pack: m.info.pack }) : t('Read it again from {source} and put it back whole (click twice)', { source: m.info.source ?? '' })}><ArmedLabel label={t('Reinstall')} armedLabel={t('Reinstall?')} armed={armed.is(`reinstall:${m.id}`)} /></button>
            {/if}
            <button class="btn" class:armed={armed.is(`remove:${m.id}`)} disabled={busy === m.id} onclick={() => act('remove', m.id)} title={m.info.pack ? t('Delete it from modules/ and forget its trust; the pack {pack} leaves it out from now on, until you put it back (click twice)', { pack: m.info.pack }) : t('Delete it from modules/ and forget its trust (click twice)')}><ArmedLabel label={t('Remove')} armedLabel={t('Remove?')} armed={armed.is(`remove:${m.id}`)} /></button>
          </span>
        {/if}
      </div>
    </div>
  {:else}
    <p class="hint pad">{t('No module: a pack brings them ([[modules]]), or install one below, then reload the window.')}</p>
  {/each}
  {#each removed as r (r.id)}
    <div class="module">
      <div class="minfo">
        <b>{r.id}</b>
        <small>{r.pack ? t('Removed: the pack {pack} brings it, its updates leave it out ([modules] removed).', { pack: r.pack }) : t('Removed, and no pack brings it any more ([modules] removed).')}</small>
        {#if done[r.id]}<span class="hint new"><Icon name="check" /> {done[r.id]}</span>{/if}
      </div>
      {#if r.pack && local() && !done[r.id]}
        <div class="side">
          <button class="btn" class:armed={armed.is(`restore:${r.id}`)} disabled={busy === r.id} onclick={() => act('restore', r.id)} title={t('Install it again from the pack {pack} (click twice)', { pack: r.pack })}><ArmedLabel label={t('Put back')} armedLabel={t('Put it back?')} armed={armed.is(`restore:${r.id}`)} /></button>
        </div>
      {/if}
    </div>
  {/each}
  {#if local()}
    <div class="filerow">
      <input class="t mono grow" bind:value={source} placeholder={t('~/code/hello/dist/hello, gh:owner/repo:dist/hello@v1 or glab:group/project:modules/hello.zip')} spellcheck="false" onkeydown={(e) => e.key === 'Enter' && install()} aria-label={t('Module source')} />
      <button class="btn primary" onclick={install} disabled={!source.trim() || !!busy}>{busy === '+install' ? t('Installing…') : t('Install a module')}</button>
    </div>
    <p class="hint pad">{t('Without a pack: a module’s folder or .zip on this computer, or a repository’s folder at a ref (glab or gh). Nothing of it runs before you trust it. Its source is kept: Reinstall reads it again, and a change there shows in the status bar.')}</p>
    {#if installed}<p class="hint new pad"><Icon name="check" /> {installed}</p>{/if}
    <Field label={t('Trust a pack’s updates')} hint={t('A new version of a module a pack installed, that you trusted before, runs without asking again when it reaches nothing more (the same capabilities and config tables, or fewer). Anything more, and it waits for your trust. A module installed by hand always asks.')} path={['modules', 'trust_pack_updates']}>
      <Control kind="toggle" value={valueOf(['modules', 'trust_pack_updates'], true)} onchange={(v) => setValue(['modules', 'trust_pack_updates'], v, true)} />
    </Field>
  {/if}
  <div class="reload">
    <span class="hint">{t('A module added, replaced, removed or trusted shows once the window reloads.')}</span>
    <button class="btn" onclick={() => location.reload()} title={t('Load the window again, its modules with it')}>{t('Reload the window')}</button>
  </div>
</div>

<style>
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); flex-shrink: 0; }
  .ch { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .ch b { font-size: var(--fs-title); }
  .ch small { color: var(--text-muted); font-size: var(--fs-meta); }
  .module { display: flex; align-items: flex-start; gap: 12px; padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .minfo { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .minfo b { display: flex; align-items: baseline; gap: 8px; font-weight: 550; font-size: var(--fs-body); }
  .minfo small { color: var(--text-muted); font-size: var(--fs-meta); overflow-wrap: anywhere; }
  .minfo small.crash { color: var(--ko); }
  .side { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; flex-shrink: 0; }
  .acts { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
  .ver { color: var(--text-muted); font-weight: 400; }
  .hint { font-size: var(--fs-meta); color: var(--text-muted); overflow-wrap: anywhere; }
  .hint.warn { color: var(--warn); }
  .hint.new { color: var(--ok); }
  .hint.pad { margin: 0; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .caps { margin: 2px 0 4px; padding-left: 18px; font-size: var(--fs-meta); }
  .changes { margin: 2px 0 4px; padding: 6px 8px; border-radius: var(--r6, 6px); background: var(--bg); font-size: var(--fs-meta); white-space: pre-wrap; overflow-wrap: anywhere; }
  .changes .add { color: var(--ok); }
  .changes .del { color: var(--ko); }
  .state { font-size: var(--fs-meta); color: var(--ko); white-space: nowrap; }
  .state.on { color: var(--ok); }
  .state.wait { color: var(--warn); }
  .btn.armed { border-color: var(--warn); color: var(--warn); }
  .filerow { display: flex; gap: 8px; align-items: center; padding: 10px 14px 0; }
  .grow { flex: 1; min-width: 0; }
  .t { height: 28px; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; outline: none; }
  .t:focus { border-color: var(--accent); }
  .t.mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .reload { display: flex; gap: 12px; align-items: center; justify-content: space-between; padding: 10px 14px; }
</style>
