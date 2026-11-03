<script lang="ts">
  import { localInvoke as invoke, machine } from '../bridge'
  import { machineState } from '../machine.svelte'
  import { t } from '../i18n.svelte'

  /**
   * A machine's window before its Prism answers: reaching it, why not (out of reach, refused), and
   * when it has no token, its pairing: the code its Prism shows (`prism peer pair` there), this
   * computer then accepted there by name; or a token pasted (`prism peer add` there). The token
   * goes to this computer's keychain. The page starts by itself once the machine's snapshot comes
   * (`main.ts`).
   */
  const name = $derived(machineState.name || machine() || '')
  const noToken = $derived(!!machineState.error && /no token|does not know this token|revoked/.test(machineState.error))
  let token = $state('')
  let code = $state('')
  /** A token pasted rather than a code typed. */
  let pasting = $state(false)
  /** The code given: waiting for the machine to accept this computer. */
  let asking = $state(false)
  let busy = $state(false)
  let error = $state('')
  const digits = $derived(code.replace(/\D/g, ''))

  async function run(f: () => Promise<unknown>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await f()
      location.reload()
    } catch (e) {
      error = String(e)
      busy = false
      asking = false
    }
  }
  const save = () => token.trim() && run(() => invoke('machine_token', { name, token }))
  const pair = () => digits.length === 6 && run(() => ((asking = true), invoke('machine_pair', { name, code: digits })))
</script>

<div class="gate">
  <div class="card">
    <h1><span class="dot" class:off={!!machineState.error}></span>{name}</h1>
    {#if noToken && asking}
      <p>{t('Accept this computer on {name}: where prism peer pair runs, or in its Settings › Remote › Devices.', { name })}</p>
      <p class="hint">{t('Waiting for it… (five minutes at most)')}</p>
    {:else if noToken && !pasting}
      <p>{t('Pair this computer: on {name}, run prism peer pair (or Settings › Remote › Devices › Pair a computer there), then type the code it shows.', { name })}</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="code" inputmode="numeric" autocomplete="off" spellcheck="false" maxlength="7" placeholder="123 456" bind:value={code} onkeydown={(e) => e.key === 'Enter' && pair()} autofocus />
      <span class="acts">
        <button class="btn" onclick={pair} disabled={busy || digits.length !== 6}>{t('Pair')}</button>
        <button class="link" onclick={() => (pasting = true)}>{t('Paste a token instead')}</button>
      </span>
      <p class="hint">{t('Its token is kept in this computer’s keychain. It opens that Prism as its account would: keep it as an SSH key.')}</p>
    {:else if noToken}
      <p>{t('Its Prism makes a token for this computer: run prism peer add <name> there, and paste what it prints.')}</p>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="password" autocomplete="off" spellcheck="false" placeholder={t('paste it')} bind:value={token} onkeydown={(e) => e.key === 'Enter' && save()} autofocus />
      <span class="acts">
        <button class="btn" onclick={save} disabled={busy || !token.trim()}>{t('Connect')}</button>
        <button class="link" onclick={() => (pasting = false)}>{t('Pair with a code instead')}</button>
      </span>
      <p class="hint">{t('Kept in this computer’s keychain. It opens that Prism as its account would: keep it as an SSH key.')}</p>
    {:else if machineState.error}
      <p>{t('Out of reach, trying again…')}</p>
    {:else}
      <p>{t('Connecting…')}</p>
    {/if}
    {#if machineState.error && !noToken}<p class="error">{machineState.error}</p>{/if}
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
  input.code { font-size: 20px; letter-spacing: 4px; text-align: center; }
  .acts { display: flex; align-items: center; gap: 12px; }
  .link { padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-meta); cursor: pointer; text-decoration: underline; }
  .error { color: var(--ko); font-size: var(--fs-meta); word-break: break-word; }
</style>
