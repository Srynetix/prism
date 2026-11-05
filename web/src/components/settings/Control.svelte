<script lang="ts">
  import { suggest } from '../../suggest'
  import { stageEnvs } from '../../stages'
  import ListInput from './ListInput.svelte'
  import { choices, effortsKey, loadEfforts, loadModels } from '../../settings.svelte'
  import { FIXED, type AgentCli, type Kind, type Source } from '../../settings-schema'
  import { canPick, pickPath } from '../../pick'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { agentsSettings } from '../../agents.svelte'

  /**
   * An input according to the key's type: text, path (a folder, typed or chosen in the system's
   * picker), long text (a prompt: several lines, the box resized by its corner), number, switch,
   * choice, checkboxes, list. Text with options: typed, those suggested
   * (an agent CLI's models).
   * `optional`: when empty, the key is removed (`onchange(null)`). Text is only saved when
   * leaving the field (or Enter), for a TOML preview that does not move with every letter.
   */
  let {
    kind, value, onchange, options, labels, optional = false, placeholder, unit, step = 1, compact = false,
  }: {
    kind: Exclude<Kind, 'table'>
    value: unknown
    onchange: (v: unknown) => void
    options?: Source | string[]
    /** `checks`: an option's words, by value. */
    labels?: Record<string, string>
    optional?: boolean
    placeholder?: string
    unit?: string
    /** A number's step: 1 by default (integer), 0.25 for a price or a factor. */
    step?: number
    compact?: boolean
  } = $props()

  const list = $derived.by(() => {
    if (Array.isArray(options)) return options
    if (!options) return []
    if (options === 'envs') return stageEnvs()
    if (options === 'kinds') return agentsSettings().kinds.map((k) => k.name)
    if (options.startsWith('models:')) return choices.models[options.slice(7) as AgentCli] ?? []
    if (options.startsWith('efforts:')) return choices.efforts[effortsKey(options.slice(8) as AgentCli)] ?? []
    return FIXED[options] ?? (options === 'kube' ? choices.kube : options === 'apps' ? choices.apps : options === 'calendars' ? choices.calendars : [])
  })
  const text = (v: string) => (optional && !v.trim() ? null : v.trim())
  $effect(() => {
    if (typeof options === 'string' && options.startsWith('models:')) loadModels(options.slice(7) as AgentCli)
    if (typeof options === 'string' && options.startsWith('efforts:')) loadEfforts(options.slice(8) as AgentCli)
  })

  async function pick() {
    const p = await pickPath((value as string | null) ?? '')
    if (p) onchange(p)
  }
</script>

{#if kind === 'toggle'}
  <button class="toggle" class:on={!!value} role="switch" aria-checked={!!value} aria-label={t('Toggle')} onclick={() => onchange(!value)}><i></i></button>
{:else if kind === 'number'}
  <span class="row"><input class="t num" type="number" min="0" {step} value={value ?? ''} onchange={(e) => onchange(Math.max(0, Number((Math.round((Number(e.currentTarget.value) || 0) / step) * step).toFixed(4))))} />{#if unit}<span class="unit">{t(unit)}</span>{/if}</span>
{:else if kind === 'select'}
  {@const current = (value as string | null) ?? ''}
  <select class="t" class:compact value={current} onchange={(e) => onchange(text(e.currentTarget.value))}>
    {#if optional || !current}<option value="">{placeholder ?? '—'}</option>{/if}
    {#if current && !list.includes(current)}<option value={current}>{current}</option>{/if}
    {#each list as o (o)}<option value={o}>{o}</option>{/each}
  </select>
{:else if kind === 'checks' && !list.length}
  <!-- Nothing to pick from (no access to its source, the browser preview): the names, typed. -->
  <ListInput value={(value as string[] | null) ?? []} onchange={(v) => onchange(optional && !v.length ? null : v)} {compact} />
{:else if kind === 'checks'}
  {@const picked = (value as string[] | null) ?? []}
  <!-- A picked value the source no longer offers (a calendar renamed) stays, to uncheck. -->
  {@const all = [...list, ...picked.filter((p) => !list.includes(p))]}
  <span class="row">
    {#each all as o (o)}
      <label class="check" class:gone={!list.includes(o)} title={list.includes(o) ? undefined : t('Not found any more')}><input type="checkbox" checked={picked.includes(o)} onchange={(e) => onchange(e.currentTarget.checked ? all.filter((x) => x === o || picked.includes(x)) : picked.filter((x) => x !== o))} />{labels?.[o] ?? o}</label>
    {/each}
  </span>
{:else if kind === 'list'}
  <ListInput value={(value as string[] | null) ?? []} onchange={(v) => onchange(optional && !v.length ? null : v)} options={list} {placeholder} {compact} />
{:else if kind === 'longtext'}
  <!-- ⌘↵ leaves it, as Enter leaves a line. -->
  <textarea class="t long" class:compact value={(value as string | null) ?? ''} {placeholder} rows="2" onchange={(e) => onchange(text(e.currentTarget.value))} onkeydown={(e) => e.key === 'Enter' && (e.metaKey || e.ctrlKey) && e.currentTarget.blur()}></textarea>
{:else if kind === 'path' && canPick()}
  <span class="row nowrap">
    <input class="t mono" class:compact value={(value as string | null) ?? ''} {placeholder} spellcheck="false" onchange={(e) => onchange(text(e.currentTarget.value))} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
    <button class="pick" class:compact onclick={pick} title={t('Choose a folder')} aria-label={t('Choose a folder')}><Icon name="folder" /></button>
  </span>
{:else}
  <input class="t" class:mono={kind === 'path' || kind === 'text'} class:compact value={(value as string | null) ?? ''} {placeholder} spellcheck="false" {@attach suggest(() => list)} onchange={(e) => onchange(text(e.currentTarget.value))} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
{/if}

<style>
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
  .row.nowrap { flex-wrap: nowrap; gap: 6px; }
  .pick { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 30px; height: 28px; padding: 0; border: 1px solid var(--border); border-radius: var(--r7); background: var(--surface); color: var(--text-muted); cursor: pointer; }
  .pick:hover { border-color: var(--accent); color: var(--accent); }
  .pick.compact { height: 26px; }
  .t { width: 100%; height: 28px; min-width: 0; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); outline: none; }
  .t:focus { border-color: var(--accent); }
  .t.mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .t.compact { height: 26px; }
  .t.num { width: 90px; }
  .t.long { display: block; height: auto; min-height: 28px; padding: 5px 9px; line-height: 1.4; resize: vertical; field-sizing: content; max-height: 320px; }
  select.t { max-width: 420px; }
  .unit { color: var(--text-muted); font-size: var(--fs-meta); }
  .check { display: inline-flex; align-items: center; gap: 5px; font-size: var(--fs-body); cursor: pointer; }
  .check input { margin: 0; --tick: var(--accent); }
  .check.gone { color: var(--text-muted); text-decoration: line-through; }
  .toggle { position: relative; flex-shrink: 0; width: 30px; height: 17px; padding: 0; border: 1px solid var(--border); border-radius: var(--r9); background: var(--surface-muted); cursor: pointer; }
  .toggle i { position: absolute; top: 2px; left: 2px; width: 11px; height: 11px; border-radius: 50%; background: var(--text-muted); transition: left 0.15s; }
  .toggle.on { background: color-mix(in srgb, var(--accent) 40%, var(--surface)); border-color: var(--accent); }
  .toggle.on i { left: 15px; background: var(--accent); }
</style>
