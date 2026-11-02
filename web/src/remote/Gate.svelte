<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { t } from '../i18n.svelte'
  import { locale } from '../locale'
  import { api, create, get, supported } from './passkey'

  /**
   * Before the remote: pairing this phone (the address of the Mac's QR code, `/pair#<token>`),
   * or unlocking it with its passkey. `ready` once a session is open.
   * Locked, a phone's lock screen: the time and the date, Prism's mark, Unlock at the thumb.
   * Pairing, its three steps shown, the code the Mac checks in large digits.
   * `preview` (development only, `remote.html?gate=<step>`): a step shown without a Mac.
   */
  type Step = 'name' | 'passkey' | 'confirm' | 'login'
  let { paired, ready, preview }: { paired: boolean; ready: () => void; preview?: Step } = $props()

  const token = location.pathname === '/pair' ? location.hash.slice(1) : ''
  const guess = /android/i.test(navigator.userAgent) ? t('Android phone') : /iphone/i.test(navigator.userAgent) ? 'iPhone' : /ipad/i.test(navigator.userAgent) ? 'iPad' : t('Phone')

  // svelte-ignore state_referenced_locally
  let step = $state<Step>(preview ?? (token ? 'name' : 'login'))
  let name = $state(guess)
  // svelte-ignore state_referenced_locally
  let code = $state(preview ? '482913' : '')
  let busy = $state(false)
  let error = $state<string | null>(supported() ? null : t('This browser has no passkeys: open the page in Chrome (Android) or Safari (iOS).'))
  let claim: { challenge: string; rpId: string; user: string; name: string } | null = null

  /** The passkey prompt dismissed (or timed out) is no failure: one more tap. */
  const why = (e: unknown) => {
    if (e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError')) return t('Cancelled: tap to try again.')
    return e instanceof Error ? e.message : String(e)
  }
  const run = async (f: () => Promise<void>) => {
    busy = true
    error = null
    try {
      await f()
    } catch (e) {
      error = why(e)
    } finally {
      busy = false
    }
  }

  const begin = () =>
    run(async () => {
      const c = await api<{ code: string; challenge: string; rpId: string; user: string; name: string }>('/api/pair/claim', { token, name })
      claim = c
      code = c.code
      step = 'passkey'
    })

  const makePasskey = () =>
    run(async () => {
      const registration = await create(claim!)
      await api('/api/pair/finish', { token, registration })
      step = 'confirm'
      wait()
    })

  /** Until the Mac confirms the code: then the session's cookie arrives. */
  async function wait() {
    for (;;) {
      await new Promise((r) => setTimeout(r, 1000))
      try {
        // With its claim's challenge: the session is this phone's, not any that saw the QR code.
        const s = await api<{ state: string }>('/api/pair/state', { token, claim: claim!.challenge })
        if (s.state === 'paired') {
          history.replaceState(null, '', '/')
          return ready()
        }
        if (s.state === 'gone') {
          error = t('The pairing ended on the Mac (cancelled or expired): open a new one there.')
          return
        }
      } catch {
        // Out of reach for a moment: try again.
      }
    }
  }

  const unlock = () =>
    run(async () => {
      const options = await api<{ challenge: string; rpId: string; allow: string[] }>('/api/login/begin', {})
      await api('/api/login/finish', await get(options))
      ready()
    })

  let now = $state(new Date())
  $effect(() => {
    const t = setInterval(() => (now = new Date()), 10_000)
    return () => clearInterval(t)
  })
  /** The hour large, AM / PM (a 12-hour locale's) small beside it. */
  const parts = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }).formatToParts(now))
  const time = $derived(parts.filter((p) => p.type !== 'dayPeriod').map((p) => p.value).join('').trim())
  const period = $derived(parts.find((p) => p.type === 'dayPeriod')?.value ?? '')
  const date = $derived(now.toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long' }))

  const STEPS: [Step, string][] = [['name', t('Name')], ['passkey', t('Passkey')], ['confirm', t('On the Mac')]]
  const at = $derived(STEPS.findIndex(([s]) => s === step))
</script>

<main class:lock={step === 'login'}>
  <div class="glow" aria-hidden="true"></div>
  {#if step === 'login'}
    <div class="top">
      <svg class="mark small" viewBox="0 0 32 32" aria-hidden="true"><path d="M16 4 28 26H4Z" /></svg>
      <span>Prism</span>
    </div>
    <div class="clock">
      <time>{time}{#if period}<small>{period}</small>{/if}</time>
      <p class="date">{date}</p>
    </div>
    <div class="foot">
      {#if paired}
        <button class="unlock" onclick={unlock} disabled={busy || !supported()} aria-label={t('Unlock with this phone’s passkey')}>
          <Icon name={busy ? 'unlock' : 'lock'} size="28px" />
        </button>
        <p>{busy ? t('Waiting for the passkey…') : t('Tap to unlock')}</p>
        <p class="muted">{t('With this phone’s passkey: biometrics or the screen lock.')}</p>
      {:else}
        <div class="card">
          <b>{t('No phone paired with this Prism yet')}</b>
          <ol>
            <li>{t('On the Mac:')} <em>{t('Settings › Remote › Devices')}</em></li>
            <li><em>{t('Pair a phone')}</em>{t(', a QR code shows')}</li>
            <li>{t('Scan it with this phone’s camera')}</li>
          </ol>
        </div>
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
    </div>
  {:else}
    <div class="pair">
      <svg class="mark" viewBox="0 0 32 32" aria-hidden="true"><path d="M16 4 28 26H4Z" /></svg>
      <h1>{t('Pair this phone')}</h1>
      <ol class="steps" aria-label={t('Steps')}>
        {#each STEPS as [s, label], i (s)}
          <li class:done={i < at} class:on={i === at}><span>{i < at ? '✓' : i + 1}</span>{label}</li>
        {/each}
      </ol>

      {#if step === 'name'}
        <p>{t('Its name shows on the Mac, in Settings › Remote › Devices.')}</p>
        <input bind:value={name} maxlength="40" aria-label={t('This phone’s name')} />
        <button class="primary" onclick={begin} disabled={busy || !name.trim()}>{t('Continue')}</button>
      {:else}
        <p>{step === 'passkey' ? t('The Mac will ask you to check this code:') : t('Confirm this code on the Mac, in Settings › Remote › Devices.')}</p>
        <div class="code" aria-label={t('Code {code}', { code })}>
          {#each code.split('') as d, i (i)}<span>{d}</span>{/each}
        </div>
        {#if step === 'passkey'}
          <p class="muted">{t('A passkey for this Prism is created on this phone; biometrics or the screen lock unlock it.')}</p>
          <button class="primary" onclick={makePasskey} disabled={busy}><Icon name="lock" /> {t('Create the passkey')}</button>
        {:else}
          <p class="wait"><Icon name="refresh" spin /> {t('Waiting for the Mac…')}</p>
          <p class="muted">{t('Then install the app: Chrome’s menu › Install app (Safari: Share › Add to Home Screen).')}</p>
        {/if}
      {/if}
      {#if error}<p class="error">{error}</p>{/if}
    </div>
  {/if}
</main>

<style>
  main {
    position: relative;
    overflow: hidden;
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: calc(20px + env(safe-area-inset-top)) 20px calc(28px + env(safe-area-inset-bottom));
    text-align: center;
    background: var(--bg);
    color: var(--text);
  }
  /* The ray of Prism's mark, faint behind. */
  .glow {
    position: absolute;
    inset: -20% -40% auto;
    height: 70%;
    pointer-events: none;
    background: radial-gradient(closest-side, color-mix(in srgb, var(--accent) 22%, transparent), transparent);
  }
  main > :not(.glow) { position: relative; }
  .mark { width: 56px; height: 56px; fill: color-mix(in srgb, var(--accent) 25%, transparent); stroke: var(--accent); stroke-width: 1.6; stroke-linejoin: round; }
  .mark.small { width: 18px; height: 18px; stroke-width: 2.4; }
  p { margin: 0; max-width: 340px; line-height: 1.45; }
  .muted { color: var(--text-muted); font-size: 13px; }
  .error { color: var(--ko); font-size: 13px; }

  /* Locked. */
  .top { display: flex; align-items: center; gap: 7px; color: var(--text-muted); font-size: 13px; font-weight: 600; letter-spacing: 0.04em; }
  .clock { margin-top: 9vh; }
  time { display: block; white-space: nowrap; font-size: min(84px, 24vw); font-weight: 250; line-height: 1; letter-spacing: -0.02em; font-variant-numeric: tabular-nums; }
  time small { margin-left: 6px; font-size: 0.3em; font-weight: 500; letter-spacing: 0.02em; color: var(--text-muted); }
  .date { margin-top: 8px; color: var(--text-muted); font-size: 17px; text-transform: capitalize; }
  .foot { margin-top: auto; width: 100%; display: flex; flex-direction: column; align-items: center; gap: 8px; }
  .unlock {
    width: 76px;
    height: 76px;
    margin-bottom: 6px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--accent) 50%, var(--border));
    background: color-mix(in srgb, var(--accent) 16%, var(--surface));
    color: var(--accent);
    box-shadow: 0 0 0 8px color-mix(in srgb, var(--accent) 7%, transparent);
    transition: transform 0.12s;
  }
  .unlock:active { transform: scale(0.94); }
  .unlock:disabled { opacity: 0.7; }
  .card { width: min(340px, 100%); padding: 16px; border: 1px solid var(--border); border-radius: 14px; background: var(--surface); text-align: left; }
  .card ol { margin: 10px 0 0; padding-left: 20px; color: var(--text-muted); font-size: 14px; line-height: 1.7; }
  .card em { font-style: normal; color: var(--text); }

  /* Pairing. */
  .pair { width: 100%; flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 14px; }
  h1 { margin: 0; font-size: 22px; }
  .steps { list-style: none; margin: 0 0 6px; padding: 0; display: flex; gap: 6px; font-size: 12px; color: var(--text-muted); }
  .steps li { display: flex; align-items: center; gap: 5px; padding: 4px 10px 4px 4px; border-radius: 14px; background: var(--surface-muted); }
  .steps span { display: grid; place-items: center; width: 18px; height: 18px; border-radius: 50%; background: var(--surface); font-size: 11px; font-weight: 600; }
  .steps li.on { color: var(--text); background: color-mix(in srgb, var(--accent) 18%, var(--surface)); }
  .steps li.on span { background: var(--accent); color: var(--on-accent); }
  .steps li.done span { color: var(--ok); }
  .code { display: flex; gap: 6px; }
  .code span { width: 42px; height: 54px; display: grid; place-items: center; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); font: 600 28px ui-monospace, 'SF Mono', Menlo, monospace; color: var(--accent); }
  .wait { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 14px; }
  input {
    width: min(320px, 100%);
    padding: 11px 13px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font-size: 16px;
  }
  button.primary {
    width: min(320px, 100%);
    padding: 12px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    border: none;
    border-radius: 12px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 16px;
    font-weight: 600;
  }
  button:disabled { opacity: 0.55; }
</style>
