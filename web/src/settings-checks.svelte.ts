import { invoke, isDesktop } from './bridge'
import { untrack } from 'svelte'
import { t } from './i18n.svelte'
import { live } from './live.svelte'
import { keyedBy, mergedRows, valueOf } from './settings.svelte'
import type { CheckSpec, SectionSpec } from './settings-schema'

/**
 * Live checks in Settings (checks.rs): each checkable value (GitLab project,
 * kube context, folder…) is checked when the view opens and then on each edit, half a
 * second after the last keystroke. The same check is not redone within a minute:
 * ten lines on the same kube context only contact it once.
 */
export type CheckRequest = { kind: string } & Record<string, string>
export interface CheckResult {
  state: 'pending' | 'ok' | 'ko'
  message: string
  at: number
}

export const results = $state<Record<string, CheckResult>>({})
const FRESH_MS = 60_000
const DEBOUNCE_MS = 500
const timers = new Map<string, ReturnType<typeof setTimeout>>()

export const keyOf = (r: CheckRequest) => JSON.stringify(r)

/** The check of a value; `sibling` reads a neighboring key (a namespace's context). Empty: nothing to check. */
export function requestOf(spec: CheckSpec, value: unknown, sibling: (key: string) => unknown): CheckRequest | null {
  const v = typeof value === 'string' ? value.trim() : ''
  if (typeof spec === 'object') {
    // A CLI's own binary (`[agents.<cli>] bin`), its usual name when empty.
    if (spec.kind === 'agent_bin') return agentTools(spec.cli, v)
    const context = sibling(spec.context)
    return typeof context === 'string' && context && v ? { kind: 'kube_namespace', context, namespace: v } : null
  }
  // tmux and the chosen CLI, whatever the value of the Agents switch.
  if (spec === 'agent_tools') return agentTools(v || 'claude')
  // A repository's own CLI (`[[repos]] agent`), when it names one.
  if (spec === 'agent_cli') return v ? agentTools(v) : null
  if (!v) return null
  switch (spec) {
    case 'gitlab_project': {
      // On another forge than the profile's (`[[repos]] forge`): a link, not checked with this one.
      const forge = sibling('forge')
      return typeof forge === 'string' && forge && forge !== (live.snapshot?.forge ?? 'gitlab') ? null : { kind: spec, project: v }
    }
    case 'kube_context':
      return { kind: spec, context: v }
    case 'git_repo': {
      // In [[repos]], the clone must be one of the row's project.
      const project = sibling('project')
      return typeof project === 'string' && project.trim() ? { kind: spec, path: v, project: project.trim() } : { kind: spec, path: v }
    }
    case 'directory':
      return { kind: spec, path: v }
    case 'gcloud':
      return { kind: spec }
  }
}

/** tmux and a CLI, by its binary (`[agents.<cli>] bin`, being edited or not). */
function agentTools(provider: string, bin = String(valueOf<string | null>(['agents', provider, 'bin']) ?? '').trim()): CheckRequest {
  return bin ? { kind: 'agent_tools', provider, bin } : { kind: 'agent_tools', provider }
}

/** All the checks of a section, with the values being edited. */
export function sectionRequests(spec: SectionSpec): CheckRequest[] {
  const out: CheckRequest[] = []
  for (const f of spec.fields) {
    const path = [...spec.path, f.key]
    if (f.kind === 'table') {
      for (const row of keyedBy(path) ? mergedRows(path) : (valueOf<Record<string, unknown>[] | null>(path) ?? [])) {
        for (const c of f.columns ?? []) {
          const r = c.check ? requestOf(c.check, row[c.key], (k) => row[k]) : null
          if (r) out.push(r)
        }
      }
    } else if (f.check) {
      const r = requestOf(f.check, valueOf(path), (k) => valueOf([...spec.path, k]))
      if (r) out.push(r)
    }
  }
  return out
}

/** Runs the check if it is neither in progress nor recent. To call from an effect. */
export function ensure(r: CheckRequest) {
  if (!isDesktop()) return
  const key = keyOf(r)
  const known = untrack(() => results[key])
  if (known && (known.state === 'pending' || Date.now() - known.at < FRESH_MS)) return
  clearTimeout(timers.get(key))
  timers.set(
    key,
    setTimeout(async () => {
      results[key] = { state: 'pending', message: t('checking…'), at: Date.now() }
      try {
        const o = await invoke<{ ok: boolean; message: string }>('check_setting', { check: r })
        results[key] = { state: o.ok ? 'ok' : 'ko', message: o.message, at: Date.now() }
      } catch (e) {
        results[key] = { state: 'ko', message: String(e), at: Date.now() }
      }
    }, DEBOUNCE_MS),
  )
}

/** A check of the section failed. */
export const failing = (spec: SectionSpec) => sectionRequests(spec).some((r) => results[keyOf(r)]?.state === 'ko')

/** Counter of requested re-runs, followed by the effect that runs the checks. */
export const runs = $state({ n: 0 })
/** Re-check everything (button): forgets the recent results and re-runs. */
export function recheck() {
  for (const k of Object.keys(results)) if (results[k].state !== 'pending') delete results[k]
  runs.n++
}
