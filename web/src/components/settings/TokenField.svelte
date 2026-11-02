<script lang="ts">
  import { invoke, isDesktop } from '../../bridge'
  import Field from './Field.svelte'
  import { t } from '../../i18n.svelte'
  import ConnectionCheck from './ConnectionCheck.svelte'
  import type { CheckRequest } from '../../settings-checks.svelte'

  /**
   * A tracker's API token (or Google's client secret), written straight into the system keychain
   * (the active profile's entry), never into config.toml nor shown back: only whether it is there,
   * and its length. `create`: the page where the token is made, a click away; `check`: the
   * service asked who the token is, when shown and after each save.
   */
  let {
    tracker, hint, label = t('API token'), create, check = null,
  }: { tracker: 'clickup' | 'jira' | 'google'; hint: string; label?: string; create?: { url: string; label: string }; check?: CheckRequest | null } = $props()
  let saves = $state(0)

  let service = $state('')
  let length = $state<number | null>(null)
  let value = $state('')
  let busy = $state(false)
  let error = $state('')
  let stored = $state(false)

  async function load() {
    if (!isDesktop()) return
    try {
      ;[service, length] = await invoke<[string, number | null]>('token_state', { tracker })
    } catch (e) {
      error = String(e)
    }
  }
  $effect(() => {
    void tracker
    load()
  })

  async function save() {
    if (!value.trim() || busy) return
    busy = true
    error = ''
    try {
      length = await invoke<number>('store_token', { tracker, token: value })
      value = ''
      stored = true
      saves++
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

<Field {label} {hint}>
  {#if create}<a class="create" href={create.url} target="_blank" rel="noreferrer">{create.label} ↗</a>{/if}
  <span class="row">
    <input class="t" type="password" autocomplete="off" spellcheck="false" placeholder={length ? t('replace it') : t('paste it')} bind:value onkeydown={(e) => e.key === 'Enter' && save()} disabled={!isDesktop()} />
    <button class="btn" onclick={save} disabled={busy || !value.trim()}>{busy ? t('Saving…') : t('Save to keychain')}</button>
  </span>
  <small class:ko={!!error} class:ok={stored && !error}>
    {#if error}{error}
    {:else if stored}{t('Stored ({n} characters) as {service}: read with it now.', { n: length ?? 0, service })}
    {:else if length}{t('In the keychain as {service} ({n} characters).', { n: length, service })}
    {:else if service}{t('Not in the keychain yet ({service}).', { service })}{/if}
  </small>
  {#if check && (length || stored)}<ConnectionCheck request={check} nonce={saves} />{/if}
</Field>

<style>
  .row { display: flex; gap: 8px; width: 100%; }
  .t { flex: 1; height: 28px; min-width: 0; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-family: var(--mono); font-size: var(--fs-mono); outline: none; }
  .t:focus { border-color: var(--accent); }
  .btn { flex-shrink: 0; padding: 0 10px; height: 28px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  small { color: var(--text-muted); font-size: var(--fs-meta); }
  small.ok { color: var(--ok); }
  small.ko { color: var(--ko); }
  .create { align-self: flex-start; font-size: var(--fs-meta); }
</style>
