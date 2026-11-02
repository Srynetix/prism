<script lang="ts">
  import { armer } from '../../armed.svelte'
  import { invoke, isDesktop } from '../../bridge'
  import Field from './Field.svelte'
  import { t } from '../../i18n.svelte'

  /**
   * The Google account Gmail and Google Calendar read: signing in through the browser (Google's
   * page opens, Prism waits for its answer on the loopback), signing out. The refresh token goes
   * straight into the keychain; nothing is shown back but the state and the permissions asked.
   */
  const NAMES: Record<string, string> = {
    'https://www.googleapis.com/auth/gmail.metadata': t('Gmail: labels and headers'),
    'https://www.googleapis.com/auth/calendar.calendarlist.readonly': t('your calendars’ list'),
    'https://www.googleapis.com/auth/calendar.events.readonly': t('their events, read-only'),
  }
  let google = $state<{ signed_in: boolean; scopes: string[] } | null>(null)
  let busy = $state(false)
  let error = $state('')
  const arm = armer(4000)

  async function load() {
    if (!isDesktop()) return
    google = await invoke<{ signed_in: boolean; scopes: string[] }>('google_state').catch((e) => {
      error = String(e)
      return null
    })
  }
  $effect(() => {
    load()
  })

  async function signIn() {
    busy = true
    error = ''
    try {
      await invoke('google_sign_in')
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
      await load()
    }
  }

  /** Two clicks: the first arms the button (confirm() does not work in the webview). */
  async function signOut() {
    if (!arm.confirm()) return
    error = ''
    await invoke('google_sign_out').catch((e) => (error = String(e)))
    await load()
  }
</script>

<Field label="Google" hint={t('Signs in once, in your browser. Prism asks only for what the sources turned on read: {scopes}. The refresh token stays in the system keychain, this profile’s entry.', { scopes: google?.scopes.length ? google.scopes.map((s) => NAMES[s] ?? s).join(', ') : t('nothing yet') })}>
  {#if !isDesktop()}
    <small>{t('Signing in happens in the app.')}</small>
  {:else if !google}
    <small>…</small>
  {:else if !google.scopes.length}
    <small>{t('Choose Gmail as the mail provider, or Google Calendar as the calendar, then save to sign in.')}</small>
  {:else if busy}
    <small>{t('Waiting for Google in your browser: sign in and allow every permission…')}</small>
  {:else if google.signed_in}
    <span class="row"><small class="ok grow">{t('Signed in.')}</small><button class="btn" onclick={signIn}>{t('Sign in again')}</button><button class="btn" class:armed={arm.is()} onclick={signOut}>{arm.is() ? t('Click again to sign out') : t('Sign out')}</button></span>
  {:else}
    <span class="row"><small class="grow">{t('Not signed in.')}</small><button class="btn primary" onclick={signIn}>{t('Sign in with Google')}</button></span>
  {/if}
  {#if error}<small class="ko">{error}</small>{/if}
</Field>

<style>
  .row { display: flex; gap: 8px; align-items: center; width: 100%; }
  .grow { flex: 1; }
  .btn { flex-shrink: 0; padding: 0 10px; height: 28px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .btn.armed { border-color: var(--ko); color: var(--ko); }
  small { color: var(--text-muted); font-size: var(--fs-meta); }
  small.ok { color: var(--ok); }
  small.ko { color: var(--ko); }
</style>
