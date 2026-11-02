import { live } from './live.svelte'
import type { DevServer, DevServersState } from './blocks/dev_servers/types'
import { t } from './i18n.svelte'

/**
 * An agent's open ports, for the Agents view: the servers the Dev servers block finds for it (in
 * its shells, started by the agent itself, a Compose container of its folder started since it was
 * launched), running or starting.
 * Nothing while the block is off (`[dev_servers] enabled`): it alone reads the processes.
 */
const state = () => live.snapshot?.blocks?.dev_servers as DevServersState | undefined

export const devServersOn = () => state() !== undefined

/** Its servers listening (or about to), each port once, in the order found. */
export function agentServers(id: string): DevServer[] {
  return (state()?.servers ?? []).filter((s) => s.agent === id && s.state !== 'stopped')
}

/** One chip per port: the server holding it, for its title. */
export function agentPorts(id: string): { port: number; server: DevServer }[] {
  const seen = new Set<number>()
  return agentServers(id).flatMap((server) => server.ports.filter((p) => !seen.has(p) && seen.add(p)).map((port) => ({ port, server })))
}

/** Where it runs and what started it: a chip's title. */
export const serverTitle = (s: DevServer) => `${s.command} · ${s.docker ? t('container') : s.shell !== null ? t('shell {n}', { n: s.shell }) : t('started by the agent')}`
