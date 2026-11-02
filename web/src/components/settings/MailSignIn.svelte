<script lang="ts">
  import { armer } from '../../armed.svelte'
  import { invoke, isDesktop } from '../../bridge'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import Field from './Field.svelte'
  import { t } from '../../i18n.svelte'
  import { live } from '../../live.svelte'
  import { copy } from '../../toast.svelte'

  /**
   * The Mail block's Microsoft 365 account: signing in through the device code flow (a code to
   * type at Microsoft's address, the Mail.ReadBasic permission to accept), signing out. The
   * refresh token goes straight into the keychain; nothing is shown back but the state.
   */
  const mail = $derived(live.snapshot?.mail)
  let code = $state<{ user_code: string; verification_uri: string } | null>(null)
  let busy = $state(false)
  let error = $state('')
  const arm = armer(4000)

  async function signIn() {
    busy = true
    error = ''
    try {
      code = await invoke<{ user_code: string; verification_uri: string; expires_in: number }>('mail_sign_in_start')
      await openUrl(code.verification_uri).catch(() => {})
      await invoke('mail_sign_in_wait')
      code = null
    } catch (e) {
      error = String(e)
      code = null
    } finally {
      busy = false
    }
  }

  /** Two clicks: the first arms the button (confirm() does not work in the webview). */
  async function signOut() {
    if (!arm.confirm()) return
    error = ''
    await invoke('mail_sign_out').catch((e) => (error = String(e)))
  }
</script>

<Field label="Microsoft 365" hint={t('Signs in with your work account, once: Prism only gets to read your messages’ sender, subject, date and states (Mail.ReadBasic). The refresh token stays in the system keychain, this profile’s entry.')}>
  {#if !isDesktop()}
    <small>{t('Signing in happens in the app.')}</small>
  {:else if !mail}
    <small>{t('Turn on Mail above, then save to sign in.')}</small>
  {:else if code}
    <span class="row">
      <code class="code">{code.user_code}</code>
      <button class="btn" onclick={() => copy(code!.user_code, t('Code copied'))}>{t('Copy')}</button>
      <button class="btn" onclick={() => openUrl(code!.verification_uri)}>{t('Open {address}', { address: code.verification_uri.replace(/^https?:\/\//, '') })}</button>
    </span>
    <small>{t('Type this code at Microsoft’s address, sign in and accept. Waiting…')}</small>
  {:else if mail.signed_in}
    <span class="row"><small class="ok grow">{t('Signed in.')}</small><button class="btn" class:armed={arm.is()} onclick={signOut}>{arm.is() ? t('Click again to sign out') : t('Sign out')}</button></span>
  {:else}
    <span class="row"><small class="grow">{t('Not signed in.')}</small><button class="btn primary" onclick={signIn} disabled={busy}>{t('Sign in')}</button></span>
  {/if}
  {#if error}<small class="ko">{error}</small>{/if}
</Field>

<style>
  .row { display: flex; gap: 8px; align-items: center; width: 100%; }
  .grow { flex: 1; }
  .code { padding: 2px 10px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); font: 600 15px var(--mono); letter-spacing: 1px; }
  .btn { flex-shrink: 0; padding: 0 10px; height: 28px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .btn.armed { border-color: var(--ko); color: var(--ko); }
  small { color: var(--text-muted); font-size: var(--fs-meta); }
  small.ok { color: var(--ok); }
  small.ko { color: var(--ko); }
</style>
