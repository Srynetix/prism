<script lang="ts">
  import { localInvoke as invoke, machine } from '../bridge'
  import { machineState } from '../machine.svelte'
  import { t } from '../i18n.svelte'

  /**
   * A machine's window before its Prism answers: reaching it, why not (out of reach, refused), and
   * its token asked for when it has none (`prism peer add` there), kept in this computer's keychain.
   * The page starts by itself once the machine's snapshot comes (`main.ts`).
   */
  const name = $derived(machineState.name || machine() || '')
  const noToken = $derived(!!machineState.error && /no token|does not know this token|revoked/.test(machineState.error))
  let token = $state('')
  let busy = $state(false)
  let error = $state('')

  async function save() {
    if (!token.trim() || busy) return
    busy = true
    error = ''
    try {
      await invoke('machine_token', { name, token })
      location.reload()
    } catch (e) {
      error = String(e)
      busy = false
    }
  }
</script>

<div class="gate">
  <div class="card">
    <h1><span class="dot" class:off={!!machineState.error}></span>{name}</h1>
    {#if noToken}
      <p>{t('Its Prism makes a token for this computer: run prism peer add <name> there, and paste what it prints.')}</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="password" autocomplete="off" spellcheck="false" placeholder={t('paste it')} bind:value={token} onkeydown={(e) => e.key === 'Enter' && save()} autofocus />
      <button class="btn" onclick={save} disabled={busy || !token.trim()}>{t('Connect')}</button>
      <p class="hint">{t('Kept in this computer’s keychain. It opens that Prism as its account would: keep it as an SSH key.')}</p>
    {:else if machineState.error}
      <p>{t('Out of reach, trying again…')}</p>
    {:else}
      <p>{t('Connecting…')}</p>
    {/if}
    {#if machineState.error}<p class="error">{machineState.error}</p>{/if}
    {#if error}<p class="error">{error}</p>{/if}
  </div>
</div>

<style>
  .gate { position: fixed; inset: 0; display: flex; align-items: center; justify-content: center; background: var(--bg); color: var(--text); font-family: var(--sans); font-size: var(--fs-body); }
  .card { width: 380px; max-width: calc(100vw - 32px); padding: 20px 22px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); display: flex; flex-direction: column; gap: 10px; }
  h1 { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 16px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; background: var(--info); }
  .dot.off { background: var(--warn); }
  p { margin: 0; color: var(--text-muted); }
  input { padding: 6px 8px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: 12px var(--mono); }
  .btn { align-self: flex-start; padding: 4px 12px; border: none; border-radius: var(--r6); background: var(--accent); color: var(--on-accent); font: inherit; font-weight: 600; cursor: pointer; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .hint { font-size: var(--fs-meta); }
  .error { color: var(--ko); font-size: var(--fs-meta); word-break: break-word; }
</style>
