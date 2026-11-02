import { invoke, isDesktop, listen } from './bridge'
import { agentsEnabled, agentsView, closeAgents, openAgents } from './agents.svelte'
import { guides } from './help-guides'
import { openHelp } from './help.svelte'
import { openSettings } from './settings.svelte'
import { openProfile, profilesView, switchProfileById } from './profiles.svelte'
import { toast } from './toast.svelte'
import { toggleZen, zen } from './layout.svelte'
import { keys } from './platform'
import { feedbackOn, openFeedback } from './feedback.svelte'
import { openPalette } from './palette.svelte'
import { lang, t } from './i18n.svelte'

/**
 * The macOS menu bar (src-tauri/src/menu.rs): its Help menu lists the guides, which the window
 * gives it; its items come back here as the `menu` event. Settings… (⌘,), New Agent (⌘N) and
 * Zen Mode (⌘⇧F) share their shortcut with the window's keys: when both see the key, the second is ignored.
 */
let keyed = { action: '', at: 0 }

/** The window's keys ran this action (`settings`, `new-agent`): the menu's echo is ignored. */
export const keyedAction = (action: string) => (keyed = { action, at: Date.now() })

export function run(action: string) {
  if (keyed.action === action && Date.now() - keyed.at < 500) return
  if (action === 'settings') {
    closeAgents()
    openSettings()
  } else if (action === 'new-agent') {
    if (!agentsEnabled()) return
    openAgents()
    agentsView.creating = true
    agentsView.desktopHref = null
  } else if (action === 'agents') {
    if (agentsEnabled()) openAgents()
  } else if (action === 'zen') {
    toggleZen()
    if (zen.on) toast(t('Zen mode · {keys} to leave it', { keys: keys('⌘⇧F') }))
  } else if (action === 'palette') {
    openPalette()
  } else if (action === 'feedback') {
    if (feedbackOn()) void openFeedback({ where: 'Help menu' })
    else openSettings('feedback')
  } else if (action === 'help') openHelp()
  else if (action.startsWith('help:')) openHelp(action.slice('help:'.length))
  else if (action.startsWith('profile-open:')) void openProfile({ id: action.slice('profile-open:'.length) }).catch((e) => toast(String(e), 6000))
  else if (action.startsWith('profile:')) void switchProfileById(action.slice('profile:'.length)).catch((e) => toast(String(e), 6000))
  else if (action === 'profile-new' || action === 'profiles') {
    closeAgents()
    profilesView.creating = action === 'profile-new'
    openSettings('profiles')
  }
}

/** At startup: the language goes to the menus, the guides to the Help menu, the menu's items come back to `run`. */
export function watchMenu() {
  if (!isDesktop()) return
  void invoke('set_help_guides', { guides: guides.map((g) => ({ id: g.id, title: g.title, group: t(g.group) })), lang: lang() }).catch(() => {})
  const off = listen<string>('menu', (e) => run(e.payload))
  return () => void off.then((f) => f())
}
