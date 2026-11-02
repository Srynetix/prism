import type { CheckRequest } from './settings-checks.svelte'
import { t } from './i18n.svelte'

/**
 * Where each tool's access comes from, for the Settings and the pages wizard: the page that
 * creates a tracker's token (a click away), the forge CLI's login command, and the check that
 * asks the service who that access is (checks.rs).
 */
export const TOKEN_PAGES = {
  clickup: { url: 'https://app.clickup.com/settings/apps', label: t('Create it in ClickUp › Settings › Apps › API Token') },
  jira: { url: 'https://id.atlassian.com/manage-profile/security/api-tokens', label: t('Create it at id.atlassian.com › Security › API tokens') },
} as const

export const TOKEN_HINTS = {
  clickup: t('Generate (or copy) your personal token, starting with pk_, and paste it here. Kept in the system keychain, this profile’s entry; never in config.toml.'),
  jira: t('Create an API token for the account below, and paste it here. Kept in the system keychain, this profile’s entry; never in config.toml.'),
} as const

export const tokenCheck = (tracker: 'clickup' | 'jira', site = '', email = ''): CheckRequest =>
  tracker === 'clickup' ? { kind: 'clickup_token' } : { kind: 'jira_token', site, email }

/** The forge CLI's login, on a self-hosted GitLab its host. */
export function forgeLogin(forge: 'gitlab' | 'github', host = '') {
  const h = host.trim().replace(/^https?:\/\//, '').replace(/\/$/, '')
  return forge === 'github'
    ? { tool: 'gh', command: 'gh auth login', check: { kind: 'forge_login', forge } as CheckRequest }
    : { tool: 'glab', command: h && h !== 'gitlab.com' ? `glab auth login --hostname ${h}` : 'glab auth login', check: { kind: 'forge_login', forge } as CheckRequest }
}
