import type { BlockAlert, BlockDef } from '../block'
import type { DevServersState } from './types'
import { t, tn } from '../../i18n.svelte'

/** A server whose shell went back to its prompt on an error: one alert each. */
function alerts(d: DevServersState): BlockAlert[] {
  return d.servers
    .filter((s) => s.state === 'stopped' && s.failed)
    .map((s) => ({
      severity: 'ko' as const,
      text: s.shell !== null ? t('Dev server {command} stopped on an error (shell {shell})', { command: s.command, shell: s.shell }) : t('Dev server {command} stopped on an error', { command: s.command }),
      notification: { key: `dev_servers:${s.key}`, title: t('Dev server') },
      fixes: s.tail ? [{ kind: 'dev-server' as const, agent: s.agent, command: s.command, where: s.shell !== null ? `shell ${s.shell}` : 'started by the agent', tail: s.tail }] : [],
    }))
}

/** Dev servers: what your agents' folders run on which port, and what to start, from their Tasks. */
export const devServers = {
  id: 'dev_servers' as const,
  label: t('Dev servers'),
  category: 'Code & agents',
  help: {
    purpose: t('Which local servers run for which agent, on which port, and why did one stop?'),
    points: [
      t('By agent: each server running in one of its shells (or started by the agent itself, or a Compose container from its folder started since the agent was launched), its ports as links, since when. Show opens the shell in the agent’s panel; Stop sends it Ctrl-C.'),
      t('A server whose shell went back to its prompt stays, with its last lines: red when they read as an error (a port already in use, a traceback). Run again, or dismiss it.'),
      t('Nothing running: ▶ Start runs the tasks starred in the agent’s Tasks panel (★), one shell each. Without a star, the Tasks panel opens.'),
      t('Outside Prism: the ports held by programs Prism did not start (a forgotten node, a Homebrew Postgres, a container, one from an agent’s folder already running before it); system and application programs aside. Kill takes two clicks.'),
      t('In Alerts: a dev server stopped on an error.'),
    ],
    source: t('Prism’s tmux panes, the process tree (ps), the listening TCP ports (lsof, or ss on Linux) and docker ps, every [dev_servers] poll_seconds. Nothing to declare: the stack comes from the Tasks.'),
  },
  settings: {
    parent: 'agents-root',
    node: { id: 'dev_servers', label: t('Dev servers'), sections: ['dev_servers'], toggle: 'dev_servers' },
    sections: [
      {
        id: 'dev_servers', source: 'dev_servers', label: t('Dev servers'), table: '[dev_servers]', path: ['dev_servers'],
        intro: t('The Dev servers block: the servers your agents’ shells run and their ports, found in the process tree; what to start comes from the stars of the agents’ Tasks panel. Read at launch.'),
        fields: [
          { key: 'enabled', label: t('Dev servers'), hint: t('Off by default. On: reads the processes and their listening ports; add the Dev servers block to a page.'), kind: 'toggle' },
          { key: 'poll_seconds', label: t('Read every'), hint: t('The processes, ports and containers.'), kind: 'number', unit: t('seconds') },
          { key: 'scan_outside', label: t('Outside Prism'), hint: t('Also list the ports held by programs Prism did not start (system and application programs aside).'), kind: 'toggle' },
        ],
      },
    ],
  },
  badge: (d) => {
    const n = d.servers.filter((s) => s.state === 'stopped' && s.failed).length
    return { count: n, title: tn(n, '{n} dev server stopped on an error', '{n} dev servers stopped on an error') }
  },
  alerts,
} satisfies BlockDef<DevServersState>
