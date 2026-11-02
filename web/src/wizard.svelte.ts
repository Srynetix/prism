import { hasCore, invoke, isDesktop } from './bridge'
import { pref, setPref } from './layout.svelte'
import { live } from './live.svelte'
import type { SetupDetection } from './bindings/SetupDetection'

/**
 * Pages wizard: Forge → Agents → Repositories → Tickets → Your page, each step pre-filled from what
 * the machine has (`detect_setup`), then a template (those of the packs, or Prism's starter pack)
 * without the blocks whose source is missing, applied to config.toml. Offered once at the first
 * launch without `[[views]]`, then from Settings › Interface › Pages.
 */
export const wizard = $state({ open: false })

export const openWizard = () => (wizard.open = true)
/** Closed or skipped: the config keeps its pages, or none (one empty Dashboard, to fill with "+ Block"). */
export function closeWizard() {
  wizard.open = false
  setPref('wizard.seen', true)
}

/**
 * First launch: the app, without pages in the config, and the wizard never seen; once a config
 * exists (without one, FirstLaunch sets it up first).
 */
export const wizardDue = () => isDesktop() && !!live.snapshot && !live.snapshot.profile?.fresh && !live.snapshot.views?.length && !pref<boolean>('wizard.seen', false)

/** The browser preview's machine: fictitious, to see the steps without a core. */
const PREVIEW: SetupDetection = {
  forges: [
    { provider: 'gitlab', installed: true, host: 'gitlab.com', login: null },
    { provider: 'github', installed: true, host: 'github.com', login: 'octo-dev' },
  ],
  agents: [
    { provider: 'claude', path: '~/.local/bin/claude', version: '2.1.40 (Claude Code)' },
    { provider: 'command-code', path: null, version: null },
    { provider: 'opencode', path: null, version: null },
    { provider: 'codex', path: '/opt/homebrew/bin/codex', version: 'codex-cli 0.162.1' },
  ],
  tmux: 'tmux 3.5a',
  folders: ['~/Development'],
  repos: [
    { name: 'shop-api', path: '~/Development/shop-api', root: '~/Development', project: 'acme/shop-api', host: 'github.com', forge: 'github', touched: 1_790_000_000 },
    { name: 'shop-web', path: '~/Development/shop-web', root: '~/Development', project: 'acme/shop-web', host: 'github.com', forge: 'github', touched: 1_789_000_000 },
    { name: 'dotfiles', path: '~/Development/dotfiles', root: '~/Development', project: 'octo-dev/dotfiles', host: 'github.com', forge: 'github', touched: 1_780_000_000 },
    { name: 'infra', path: '~/Development/ops/infra', root: '~/Development', project: 'acme/infra', host: 'gitlab.com', forge: 'gitlab', touched: 1_770_000_000 },
  ],
  kubectl: false,
  kube_contexts: [],
  trackers: { clickup_env: false, jira_env: false, obsidian_vaults: ['~/Notes'] },
}

/** What the machine has: asked of the core, or the preview's sample without one. */
export const detectSetup = (): Promise<SetupDetection> => (hasCore() ? invoke<SetupDetection>('detect_setup') : Promise.resolve(PREVIEW))
