<script lang="ts">
  import Icon from '../Icon.svelte'
  import ArmedLabel from '../ArmedLabel.svelte'
  import { blockLabel } from '../../views'
  import { moduleStates } from '../../modules/modules.svelte'
  import { armer } from '../../armed.svelte'
  import { invoke, hasCore, machine } from '../../bridge'
  import { toast } from '../../toast.svelte'
  import { t } from '../../i18n.svelte'

  /**
   * Settings › Packs › Modules: a team's blocks loaded at run time (`modules/<id>/` next to
   * config.toml, a pack's or put there by hand), as this window loaded them: their blocks, or why
   * not. Nothing of a module runs before I trust it: what it may reach is shown, what a new
   * version changes too. A module added, replaced or trusted shows once the window reloads.
   */
  const list = $derived(moduleStates())
  const armed = armer()
  /** Trusted from here: what is left to do (reload; restart Prism for a back end installed after launch). */
  let trusted = $state<Record<string, 'reload' | 'restart'>>({})
  let busy = $state('')

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
</script>

<div class="card">
  <div class="ch"><b>{t('Modules')}</b><small>{t('a team’s blocks loaded without rebuilding Prism, from modules/ next to config.toml')}</small></div>
  {#each list as m (m.id)}
    <div class="module">
      <div class="minfo">
        <b>{m.id}{#if m.version}<span class="ver mono">v{m.version}</span>{/if}</b>
        {#if m.description}<small>{m.description}</small>{/if}
        {#if m.info?.pack}<small>{t('From the pack {pack}.', { pack: m.info.pack })}</small>{/if}
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
        {#each m.clashes as block (block)}<span class="hint warn"><Icon name="warning" /> {t('Its block {block} is not shown: that id is already taken.', { block })}</span>{/each}
      </div>
      {#if m.state === 'waiting' && m.info && !trusted[m.id] && hasCore() && !machine()}
        <button class="btn" class:armed={armed.is(m.id)} disabled={busy === m.id} onclick={() => trust(m.id, m.info!.fingerprint)} title={t('Run this module’s web part and back end on this computer (click twice)')}><ArmedLabel label={t('Trust')} armedLabel={t('Trust it?')} armed={armed.is(m.id)} /></button>
      {:else}
        <span class="state" class:on={m.state === 'on' && !m.clashes.length} class:wait={m.state === 'waiting'}>{m.state === 'on' ? t('Loaded') : m.state === 'waiting' ? (trusted[m.id] ? t('Trusted') : t('Waiting')) : m.state === 'refused' ? t('Refused') : t('Failed')}</span>
      {/if}
    </div>
  {:else}
    <p class="hint pad">{t('No module: a pack brings them ([[modules]]), or put a module’s folder in modules/, next to config.toml, then reload the window.')}</p>
  {/each}
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
  .reload { display: flex; gap: 12px; align-items: center; justify-content: space-between; padding: 10px 14px; }
</style>
