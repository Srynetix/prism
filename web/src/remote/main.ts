import { mount } from 'svelte'
import { useTransport } from '../bridge'
import '../app.css'
import { api } from './passkey'
import { openSocket } from './socket.svelte'

/**
 * Prism's remote, on a phone: its own page (`remote.html`), served by the computer through the tunnel.
 * Before anything, a session: this phone paired, then unlocked with its passkey (`Gate`); then
 * the socket answers the window's calls (`bridge`) and the views mount (`RemoteApp`).
 */
document.documentElement.dataset.theme = matchMedia('(prefers-color-scheme: dark)').matches ? 'graphite' : 'paper'
// The phone's page: its own scrolling rules (app.css).
document.documentElement.classList.add('remote')
if ('serviceWorker' in navigator) navigator.serviceWorker.register('/remote/sw.js', { scope: '/' }).catch(() => {})

// Links leave the app for the phone's browser.
document.addEventListener('click', (e) => {
  const a = (e.target as HTMLElement | null)?.closest?.('a[href]') as HTMLAnchorElement | null
  if (e.defaultPrevented || !a) return
  const href = a.getAttribute('href') ?? ''
  if (/^https?:/i.test(href)) {
    e.preventDefault()
    window.open(href, '_blank', 'noopener')
  } else if (href === '#') {
    e.preventDefault()
  }
})

const target = document.getElementById('app')!
let current: Record<string, unknown> | null = null

async function start() {
  // The language's words (the last one the computer said, kept on the phone) before any view.
  await (await import('../i18n.svelte')).loadLanguage()
  const { unmount } = await import('svelte')
  if (current) unmount(current)
  // Development only: `remote.html?preview` shows the views on the browser preview's snapshot
  // (dev-snapshot.json), without a computer nor a session: to look at them at a phone's width.
  // `remote.html?gate=login|unpaired|name|passkey|confirm`: the lock screen, at one of its steps.
  const gate = import.meta.env.DEV ? new URLSearchParams(location.search).get('gate') : null
  if (gate) {
    const { default: Gate } = await import('./Gate.svelte')
    const preview = gate === 'unpaired' ? 'login' : (gate as 'name' | 'passkey' | 'confirm' | 'login')
    current = mount(Gate, { target, props: { paired: gate !== 'unpaired', ready: () => {}, preview } })
    return
  }
  if (import.meta.env.DEV && new URLSearchParams(location.search).has('preview')) {
    await (await import('../modules/modules.svelte')).loadModules()
    const { connect, live } = await import('../live.svelte')
    await connect()
    // `remote.html?preview&profiles`: the switcher and All, on made-up profiles.
    if (new URLSearchParams(location.search).has('profiles')) (await import('./phone-profiles.svelte')).previewProfiles(live.snapshot)
    const { default: RemoteApp } = await import('./RemoteApp.svelte')
    current = mount(RemoteApp, { target })
    return
  }
  const session = await api<{ device: string | null; paired: boolean }>('/api/session').catch(() => ({ device: null, paired: true }))
  if (!session.device || location.pathname === '/pair') {
    const { default: Gate } = await import('./Gate.svelte')
    current = mount(Gate, { target, props: { paired: session.paired, ready: () => location.replace('/') } })
    return
  }
  // The profiles of this computer, and the one this phone shows (`phone-profiles`): its socket at its path.
  const profiles = await import('./phone-profiles.svelte')
  await profiles.loadProfiles()
  // The modules' blocks of the profile it shows, before the views read the blocks.
  await (await import('../modules/modules.svelte')).loadModules(profiles.remoteBase())
  useTransport(profiles.routed(openSocket(() => location.replace('/'), { base: profiles.remoteBase() })))
  profiles.openSides()
  const { connect } = await import('../live.svelte')
  await connect()
  const { initTheme, watchTheme } = await import('../theme.svelte')
  initTheme()
  // A theme saved from the phone's Settings, or on the computer: config.toml re-read, the snapshot says so.
  watchTheme()
  const { default: RemoteApp } = await import('./RemoteApp.svelte')
  current = mount(RemoteApp, { target })
}

start()
