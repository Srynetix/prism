import type { StatusLevel as Level } from '../../bindings/StatusLevel'
import type { StatusState } from '../../bindings/StatusState'
import type { StatusService as Service } from '../../bindings/StatusService'
import type { StatusIncident as Incident } from '../../bindings/StatusIncident'

/** The status pages read, as the Status pages block publishes them (`Snapshot.blocks.status`): Rust's types (`crates/blocks/src/status`), generated in `bindings/`. */
export type { Level, StatusState, Service, Incident }

/** A level's tone: green, orange, red, or muted (maintenance, not answering). */
export const toneOf = (l: Level): 'ok' | 'warn' | 'ko' | '' => (l === 'operational' ? 'ok' : l === 'minor' ? 'warn' : l === 'major' || l === 'critical' ? 'ko' : '')

/** Services with something under way (an incident, a component down), worst first. */
export function troubled(s: StatusState | undefined): Service[] {
  const rank: Record<Level, number> = { critical: 5, major: 4, minor: 3, maintenance: 2, unknown: 1, operational: 0 }
  return (s?.services ?? []).filter((x) => x.level !== 'operational' && x.level !== 'unknown').sort((a, b) => rank[b.level] - rank[a.level])
}
