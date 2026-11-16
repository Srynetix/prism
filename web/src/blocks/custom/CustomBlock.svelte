<script lang="ts">
  import { hasCore, isDesktop } from '../../bridge'
  import BlockControls from '../../components/BlockControls.svelte'
  import Icon from '../../components/Icon.svelte'
  import { isCollapsed, isShort } from '../../layout.svelte'
  import { openSettings } from '../../settings.svelte'
  import { armer } from '../../armed.svelte'
  import ArmedLabel from '../../components/ArmedLabel.svelte'
  import State from '../../components/ui/State.svelte'
  import { toast, toastError } from '../../toast.svelte'
  import { ago } from '../../time'
  import { t } from '../../i18n.svelte'
  import { call } from '../block'
  import CustomOutput from './CustomOutput.svelte'
  import { readNow } from './actions'
  import { SOURCE, worst, type CustomBlockView } from './types'

  /**
   * One custom block, as a block of a page (`custom:<id>`) or among the others (`CustomBlocks`).
   * Waiting for trust: what changed and where it came from instead of its output, and the buttons
   * to trust it (two clicks, on the computer only), go back to the trusted version, or remove it.
   */
  let { block, pageId, embedded = false }: { block: CustomBlockView | undefined; pageId: string; embedded?: boolean } = $props()

  const short = $derived(isShort(pageId))
  const level = $derived(block ? worst(block) : null)
  let refreshing = $state(false)
  async function refresh() {
    if (!block) return
    refreshing = true
    try {
      await readNow(block.id)
    } catch (e) {
      toastError(String(e), refresh)
    } finally {
      setTimeout(() => (refreshing = false), 1200)
    }
  }

  /** Two clicks: the first arms the button a few seconds (no confirm() in the app's webview). */
  const armed = armer()
  async function act(what: 'trust' | 'revert' | 'delete') {
    if (!block || !armed.confirm(what)) return
    try {
      await call(SOURCE, what, what === 'trust' ? { id: block.id, hash: block.pending?.hash } : { id: block.id })
      toast(what === 'trust' ? t('{name} trusted: it runs now', { name: block.label }) : what === 'revert' ? t('{name} is back to its trusted version', { name: block.label }) : t('{name} removed from config.toml', { name: block.label }))
    } catch (e) {
      toastError(String(e))
    }
  }

  const REASON = { new: t('new'), modified: t('changed'), pack_updated: t('pack updated') }
</script>

<section class="panel" class:embedded style="--src: var(--violet)" class:collapsed={!embedded && isCollapsed(pageId)}>
  <h2>
    <span class="title"><Icon name="code" />{block?.label ?? pageId.replace(/^custom:/, '')}</span>
    {#if block?.pending}<span class="pend"><Icon name="warning" /> {REASON[block.pending.reason]}</span>
    {:else if block}
      <small class:ko={level === 'ko'} class:warn={level === 'warn'} title={[block.source, block.ran_at && t('read {time} ago', { time: ago(block.ran_at) }), block.took_ms != null && `${block.took_ms} ms`].filter(Boolean).join(' · ')}>
        {block.running ? t('reading…') : block.output?.status?.text ?? (block.ran_at ? t('{time} ago', { time: ago(block.ran_at) }) : '')}
      </small>
      {#if hasCore() && block.enabled}<button class="btn sm icon ghost" onclick={refresh} title={t('Run it now')} aria-label={t('Refresh')}><Icon name="refresh" spin={refreshing || block.running} /></button>{/if}
    {/if}
    {#if !embedded}<BlockControls id={pageId} />{/if}
  </h2>
  {#if !block}
    <State kind="off" title={t('No custom block {id} in config.toml ([[custom_blocks]]).', { id: pageId.replace(/^custom:/, '') })} action={isDesktop() ? { label: t('Settings › Custom blocks'), run: () => openSettings('custom-blocks') } : undefined} />
  {:else if block.pending}
    {@const p = block.pending}
    <div class="pending">
      <p class="why">
        {#if p.reason === 'new'}{t('Added outside Settings {time} ago: it does not run before you trust it.', { time: ago(p.since) })}
        {:else if p.reason === 'modified'}{t('Changed outside Settings {time} ago: it no longer runs, its former output is hidden.', { time: ago(p.since) })}
        {:else}{t('Its pack changed it {time} ago: the trusted version runs on until you trust this one.', { time: ago(p.since) })}{/if}
        {#if block.origin === 'pack'}<span class="tag">{t('from a pack')}</span>{/if}
      </p>
      {#if p.agents.length}<p class="agent"><Icon name="warning" />{t('An agent was running in a folder holding the config: {agents}', { agents: p.agents.join(', ') })}</p>{/if}
      {#if p.diff?.length}
        <pre class="diff">{#each p.diff as l, i (i)}<span class="l {l.op === '+' ? 'add' : l.op === '-' ? 'del' : ''}">{l.op} {l.text}
</span>{/each}</pre>
      {/if}
      {#if isDesktop()}
        <div class="btns">
          <button class="btn primary" class:armed={armed.is('trust')} onclick={() => act('trust')}><ArmedLabel label={t('Trust and run')} armedLabel={t('Click again to confirm')} armed={armed.is('trust')} /></button>
          {#if p.reason !== 'new'}<button class="btn" class:armed={armed.is('revert')} onclick={() => act('revert')}><ArmedLabel label={t('Back to the trusted version')} armedLabel={t('Click again to confirm')} armed={armed.is('revert')} /></button>{/if}
          {#if block.origin === 'config'}<button class="btn danger" class:armed={armed.is('delete')} onclick={() => act('delete')}><ArmedLabel label={t('Remove')} armedLabel={t('Click again to confirm')} armed={armed.is('delete')} /></button>{/if}
        </div>
      {:else}
        <p class="phone">{t('Trust is given on the computer only.')}</p>
        {#if block.origin === 'config'}<div class="btns"><button class="btn danger" class:armed={armed.is('delete')} onclick={() => act('delete')}><ArmedLabel label={t('Remove')} armedLabel={t('Click again to confirm')} armed={armed.is('delete')} /></button></div>{/if}
      {/if}
    </div>
    {#if p.runs_trusted && block.output}<div class="scroll"><CustomOutput {block} {short} /></div>{/if}
  {:else if !block.enabled}
    <State kind="off" title={t('Turned off (enabled = false).')} />
  {:else}
    {#if block.error}
      <div class="fail" title={block.stderr ?? ''}><State kind="error" inline title={block.error.split('\n')[0]} /></div>
    {/if}
    {#if block.output}
      <div class="scroll"><CustomOutput {block} {short} /></div>
    {:else if !block.error}
      {#if block.running || !block.ran_at}<State kind="loading" title={t('Reading…')} />{:else}<State kind="empty" title={t('Nothing printed.')} />{/if}
    {/if}
  {/if}
</section>

<style>
  .panel.embedded { border: none; box-shadow: none; border-top: 1px solid var(--border); border-radius: 0; }
  small.ko { color: var(--ko); }
  small.warn { color: var(--warn); }
  .pend { padding: 0 7px; border-radius: var(--r9); background: var(--warn-bg); color: var(--warn); font-size: var(--fs-tag); font-weight: 500; }
  .pending { display: flex; flex-direction: column; gap: 8px; padding: 10px 12px; font-size: var(--fs-body); }
  .why { margin: 0; }
  .tag { margin-left: 6px; padding: 0 6px; border: 1px solid var(--border); border-radius: var(--r9); font-size: var(--fs-tag); color: var(--text-muted); }
  .agent { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--ko); font-size: var(--fs-meta); }
  .agent :global(svg) { width: 12px; height: 12px; flex-shrink: 0; }
  .diff { margin: 0; max-height: 220px; overflow: auto; padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface-muted); font-family: var(--mono); font-size: var(--fs-meta); line-height: 1.45; }
  .l { display: block; white-space: pre; }
  .l.add { background: color-mix(in srgb, var(--ok) 14%, transparent); }
  .l.del { background: color-mix(in srgb, var(--ko) 14%, transparent); }
  .btns { display: flex; flex-wrap: wrap; gap: 6px; }
  .phone { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .fail { padding: 6px 12px; background: color-mix(in srgb, var(--ko-bg) 50%, transparent); font-size: var(--fs-meta); }
</style>
