<script lang="ts">
  import { invoke, isDesktop } from '../../bridge'
  import Field from './Field.svelte'
  import { t } from '../../i18n.svelte'
  import { FILE_MANAGER } from '../../platform'
  import { copy } from '../../toast.svelte'

  /**
   * Prism's log file, for troubleshooting: where it is, open it, or copy its last lines into a
   * report. The file holds what the collectors log, never a token nor a message's content.
   */
  const LINES = 300
  let path = $state('')
  let size = $state<number | null>(null)
  let tail = $state('')
  let error = $state('')

  async function load() {
    if (!isDesktop()) return
    try {
      ;[path, size] = await invoke<[string, number | null]>('logs_info')
    } catch (e) {
      error = String(e)
    }
  }
  $effect(() => {
    load()
  })

  const run = (f: () => Promise<unknown>) => f().catch((e) => (error = String(e)))
  const open = (with_: 'finder' | 'app') => run(() => invoke('open_logs', { with: with_ }))
  const lastLines = () => invoke<string>('logs_tail', { lines: LINES })
  const copyTail = () => run(async () => copy(await lastLines(), t('Last {n} log lines copied', { n: LINES })))
  const show = () => run(async () => {
    tail = await lastLines()
    await load()
  })
  const kb = (n: number) => (n < 1024 * 1024 ? `${Math.max(1, Math.round(n / 1024))} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`)
</script>

<Field label={t('Log file')} hint={t('What Prism logs, for troubleshooting: the sources read, their errors, the agents started. Rotated at 5 MB, the previous one kept as prism.log.1. Never a token nor a message’s content.')}>
  {#if !isDesktop()}
    <small>{t('The log file is the app’s.')}</small>
  {:else}
    <span class="row">
      <code class="grow">{path}</code>
      {#if size !== null}<small>{kb(size)}</small>{/if}
    </span>
    <span class="row">
      <button class="btn" onclick={() => open('finder')}>{t('Reveal in {app}', { app: FILE_MANAGER })}</button>
      <button class="btn" onclick={() => open('app')}>{t('Open')}</button>
      <button class="btn" onclick={copyTail}>{t('Copy last {n} lines', { n: LINES })}</button>
      <button class="btn" onclick={show}>{tail ? t('Refresh last lines') : t('Show last lines')}</button>
    </span>
    {#if tail}<pre class="tail">{tail}</pre>{/if}
  {/if}
  {#if error}<small class="ko">{error}</small>{/if}
</Field>

<style>
  .row { display: flex; gap: 8px; align-items: center; width: 100%; flex-wrap: wrap; }
  .grow { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  code { font: var(--fs-meta) var(--mono); }
  .btn { flex-shrink: 0; padding: 0 10px; height: 28px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .btn:hover { border-color: var(--accent); }
  .tail { width: 100%; max-height: 320px; margin: 0; padding: 8px 10px; overflow: auto; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); font: var(--fs-tag) var(--mono); white-space: pre; }
  small { color: var(--text-muted); font-size: var(--fs-meta); }
  small.ko { color: var(--ko); }
</style>
