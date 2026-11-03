import { mount, unmount } from 'svelte'
import { hasCore, invoke, isDesktop, machine, useTransport } from './bridge'
import { openLink } from './links'
import './app.css'
import { preload } from './persist'

/**
 * In the webview, a link does not open on its own: external links (GitLab, ClickUp…)
 * go to the browser, and `claude://` to the Claude app.
 */
document.addEventListener('click', (e) => {
  // A component has already handled the click (ticket preview instead of ClickUp).
  if (e.defaultPrevented) return
  const a = (e.target as HTMLElement | null)?.closest?.('a[href]') as HTMLAnchorElement | null
  if (!a) return
  const href = a.getAttribute('href') ?? ''
  // Outside the app, a new tab rather than leaving the page (`claude://` stays with the browser).
  if (/^(https?|claude|obsidian):/i.test(href) && (isDesktop() || /^https?:/i.test(href))) {
    e.preventDefault()
    openLink(href)
  } else if (href === '#') {
    e.preventDefault()
  }
})

// First of all: the system theme, so as not to show Paper for an instant in dark mode.
document.documentElement.dataset.theme = matchMedia('(prefers-color-scheme: dark)').matches ? 'graphite' : 'paper'

// A machine's window: its calls go to that machine. Out of reach (or no token yet), a screen
// says so until it answers; then the page starts as the main window's.
if (machine()) {
  const m = await import('./machine.svelte')
  useTransport(m.openMachine())
  const ready = m.firstSnapshot()
  if (!(await Promise.race([ready.then(() => true), new Promise<boolean>((r) => setTimeout(() => r(false), 3000))]))) {
    await (await import('./i18n.svelte')).loadLanguage()
    const { default: MachineGate } = await import('./components/MachineGate.svelte')
    const gate = mount(MachineGate, { target: document.getElementById('app')! })
    await ready
    unmount(gate)
  }
}
// Notes and layout are read from disk before the components initialize.
await preload()
// The snapshot before mounting: theme and config.toml settings known from the first frame.
if (hasCore()) {
  const { live } = await import('./live.svelte')
  try {
    live.snapshot = await invoke('get_snapshot')
  } catch (e) {
    console.error('snapshot:', e)
  }
}
// The language's words before the app's modules, whose constants hold some.
await (await import('./i18n.svelte')).loadLanguage()
const { initTheme } = await import('./theme.svelte')
initTheme()
const { default: App } = await import('./App.svelte')
export default mount(App, { target: document.getElementById('app')! })
