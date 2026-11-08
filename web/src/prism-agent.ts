import { agentsSettings, openLauncher } from './agents.svelte'
import { cfg } from './settings.svelte'
import { dirName } from './platform'

/**
 * "Configure Prism": an agent in the config's folder, given Prism's MCP server and its skill
 * whatever `[agents] prism_mcp` says, its prompt waiting for the user's request in the launcher.
 * From Settings › Agents › Prism MCP, the palette, or the ✦ next to a section (its page and table
 * in the prompt).
 */
export function configurePrism(section?: { page: string; table: string }) {
  const fr = agentsSettings().prompt_language === 'fr'
  const path = cfg.file?.path ?? ''
  const dir = dirName(path) || undefined
  const about = section ? (fr ? `\nLa question porte sur Settings › ${section.page} (${section.table}).\n` : `\nThis is about Settings › ${section.page} (${section.table}).\n`) : ''
  const prompt = fr
    ? `Aide-moi à configurer Prism. Utilise les outils du serveur MCP prism (prism_search_docs, prism_explain_key, prism_validate) et n'écris jamais config.toml toi-même : propose les changements avec prism_propose_config.\n${about}\nMa demande : `
    : `Help me configure Prism. Use the prism MCP server's tools (prism_search_docs, prism_explain_key, prism_validate) and never write config.toml yourself: propose changes with prism_propose_config.\n${about}\nMy request: `
  openLauncher({ title: fr ? 'Configurer Prism' : 'Configure Prism', cwd: dir, prompt, prism_mcp: true, from: 'Configure Prism' }, null, null, 'settings')
}
