<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../Icon.svelte'
  import { hasCore, invoke } from '../../bridge'
  import { t } from '../../i18n.svelte'
  import { copy } from '../../toast.svelte'
  import type { CliCommand } from '../../bindings/CliCommand'

  /**
   * Settings › General › Command line: `prism` on the PATH (a script running this Prism's binary,
   * cli/command.rs), its state, and the subcommands it opens to (`prism --help`).
   */
  const SAMPLE: CliCommand = { state: 'absent', found: null, runs: null, binary: '/Applications/Prism.app/Contents/MacOS/prism', install_at: '~/.local/bin/prism', dir_in_path: true, ours: false }
  let status = $state<CliCommand | null>(hasCore() ? null : SAMPLE)
  let busy = $state(false)
  let error = $state('')

  async function call(cmd: 'cli_command' | 'cli_command_install' | 'cli_command_remove') {
    if (!hasCore()) return
    busy = true
    error = ''
    try {
      status = await invoke<CliCommand>(cmd)
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
  onMount(() => void call('cli_command'))

  const dir = $derived(status?.install_at.replace(/\/prism$/, '') ?? '')
  const pathLine = $derived(`export PATH="${dir.replace(/^\/Users\/[^/]+|^\/home\/[^/]+/, '$HOME')}:$PATH"`)
</script>

<div class="card">
  <div class="head">
    <b>{t('The prism command')}</b>
    {#if status?.state === 'installed'}<span class="pill ok"><Icon name="check" /> {t('Installed')}</span>
    {:else if status?.state === 'elsewhere'}<span class="pill warn"><Icon name="warning" /> {t('Another Prism')}</span>
    {:else if status}<span class="pill">{t('Not installed')}</span>{/if}
    <span class="grow"></span>
    {#if status?.ours}<button class="btn" onclick={() => call('cli_command_remove')} disabled={busy}>{t('Remove')}</button>{/if}
    {#if status && status.state !== 'installed'}
      <button class="btn primary" onclick={() => call('cli_command_install')} disabled={busy}>{status.ours ? t('Point it to this Prism') : t('Install')}</button>
    {/if}
  </div>
  {#if error}<p class="warn pad"><Icon name="warning" /> {error}</p>{/if}
  {#if status}
    <div class="rows">
      {#if status.state === 'installed'}
        <span class="k">{t('Runs')}</span><code title={status.binary}>{status.found}{status.runs ? ` → ${status.binary}` : ''}</code>
      {:else if status.state === 'elsewhere'}
        <span class="k">{t('Found')}</span><code title={status.runs ?? status.found ?? ''}>{status.found}{status.runs ? ` → ${status.runs}` : ''}</code>
        <span class="k">{t('This Prism')}</span><code title={status.binary}>{status.binary}</code>
      {:else}
        <span class="k">{t('Installed in')}</span><code title={status.install_at}>{status.install_at}</code>
        <span class="k">{t('Runs')}</span><code title={status.binary}>{status.binary}</code>
      {/if}
    </div>
    {#if !status.dir_in_path && status.state !== 'installed'}
      <p class="warn pad">
        <Icon name="warning" /> {t('{dir} is not in your PATH: add it to your shell’s profile.', { dir })}
        <button class="btn small" onclick={() => copy(pathLine, t('Line copied'))}><code>{pathLine}</code></button>
      </p>
    {/if}
  {:else}
    <p class="muted pad">{t('Reading…')}</p>
  {/if}
  <p class="muted pad foot">
    {t('A short script that runs this Prism’s binary, as VS Code installs code. In a terminal: prism --help, prism token clickup, prism restart, prism completions zsh. Linux packages install it themselves.')}
  </p>
</div>

<style>
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .head { display: flex; align-items: center; gap: 10px; padding: 10px 14px; border-bottom: 1px solid var(--border); flex-wrap: wrap; }
  .head b { font-weight: 550; }
  .grow { flex: 1; }
  .rows { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 6px 12px; padding: 10px 14px; font-size: var(--fs-body); }
  .k { color: var(--text-muted); font-size: var(--fs-meta); }
  code { font-family: var(--mono); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .muted { color: var(--text-muted); font-size: var(--fs-meta); }
  .pad { margin: 0; padding: 10px 14px; }
  .foot { border-top: 1px solid var(--border); }
  .warn { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; color: var(--warn); font-size: var(--fs-meta); }
  .pill { display: inline-flex; align-items: center; gap: 4px; font-size: var(--fs-tag); padding: 1px 8px; border-radius: 9px; background: var(--surface-muted); color: var(--text-muted); white-space: nowrap; }
  .pill.ok { background: var(--ok-bg, var(--surface-muted)); color: var(--ok); }
  .pill.warn { background: var(--warn-bg); color: var(--warn); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 4px 12px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  .btn.small { padding: 1px 8px; min-width: 0; max-width: 100%; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
