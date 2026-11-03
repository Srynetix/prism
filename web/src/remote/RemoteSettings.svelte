<script lang="ts">
  import { hasCore, invoke } from '../bridge'
  import { live } from '../live.svelte'
  import { SCHEMA, type FieldSpec, type SectionSpec } from '../settings-schema'
  import { TREE, NODES, descendants, type TreeNode } from '../settings-tree'
  import { THEMES } from '../theme.svelte'
  import { LANGUAGES, t, tn, tc } from '../i18n.svelte'
  import Icon from '../components/Icon.svelte'

  /**
   * More › Settings on a phone (`[remote] settings`): the computer's Settings tree, every value read,
   * and the few keys the computer lets a phone change (`PHONE_SETTINGS`, listed in `editable`) edited
   * in place, then saved together after a summary and the passkey. The computer checks each key again:
   * the page only offers what it would accept. Sections drawn by Settings itself (profiles, packs,
   * console…) are shown on the computer only.
   */
  let { node, open }: { node: string | null; open: (id: string) => void } = $props()

  interface PhoneSettings {
    values: Record<string, unknown>
    effective: Record<string, unknown>
    defaults: Record<string, unknown>
    inherited: Record<string, unknown>
    error: string | null
    editable: string[]
  }

  /** `[appearance]` is drawn by Settings itself on the computer: its plain keys, for the phone. */
  const APPEARANCE: SectionSpec = {
    id: 'appearance', label: t('Appearance'), table: '[appearance]', path: ['appearance'],
    fields: [
      { key: 'theme', label: t('Theme'), kind: 'select', options: ['system', ...THEMES.map((th) => th.id)] },
      { key: 'language', label: t('Language'), hint: t('Prism’s words. System: each device’s own language.'), kind: 'select', optional: true, placeholder: t('System'), options: LANGUAGES.map((l) => l.id) },
      { key: 'density', label: t('Density'), hint: t('Comfortable: larger text, airier rows.'), kind: 'select', options: ['compact', 'comfortable'] },
      { key: 'animations', label: t('Animations'), kind: 'toggle' },
      { key: 'tooltips', label: t('Tooltips'), kind: 'toggle' },
      { key: 'screensaver', label: t('Screensaver'), kind: 'toggle' },
      { key: 'screensaver_seconds', label: t('Screensaver after'), kind: 'number', unit: 'seconds' },
      { key: 'locale', label: t('Dates and numbers'), kind: 'text' },
      { key: 'friday_guard', label: t('Friday guard'), hint: t('One more click to tag prod on a Friday afternoon.'), kind: 'toggle' },
    ],
  }
  const SPECS = new Map<string, SectionSpec>([...SCHEMA.map((s) => [s.id, s] as const), ['appearance', APPEARANCE]])

  let data = $state<PhoneSettings | null>(null)
  let loadError = $state<string | null>(null)
  /** The preview (no computer): the snapshot's few settings, saved nowhere. */
  const preview = !hasCore()

  async function load() {
    loadError = null
    try {
      if (preview) {
        const s = live.snapshot as unknown as Record<string, unknown> | null
        const effective = { alerts: s?.alerts, appearance: s?.appearance, palette: s?.palette, console: s?.console }
        data = { values: {}, effective, defaults: {}, inherited: {}, error: null, editable: [...EDITABLE_PREVIEW] }
      } else {
        data = await invoke<PhoneSettings>('remote_settings_read')
      }
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e)
    }
  }
  /** The preview's stand-in for the computer's `PHONE_SETTINGS` (src-tauri, config/remote.rs), which decides. */
  const EDITABLE_PREVIEW = [
    ...['digest', 'digest_times', 'digest_urgent', 'quiet_hours', 'focus_events', 'behind_commits', 'pending_job_minutes', 'review_wait_hours', 'doing_idle_days', 'waiting_session_hours', 'argo_out_of_sync_minutes', 'meeting_lead_minutes', 'worktree_in_use_hours', 'large_mr_lines'].map((k) => `alerts.${k}`),
    ...['theme', 'language', 'density', 'animations', 'tooltips', 'screensaver', 'screensaver_seconds'].map((k) => `appearance.${k}`),
  ]
  load()

  const dotted = (path: string[]) => path.join('.')
  const at = (obj: unknown, path: string[]): unknown => path.reduce<unknown>((o, k) => (o && typeof o === 'object' ? (o as Record<string, unknown>)[k] : undefined), obj)
  const pathOf = (s: SectionSpec, f: FieldSpec) => [...s.path, f.key]
  const editable = (path: string[]) => !!data?.editable.includes(dotted(path))
  const sectionsOf = (n: TreeNode) => (n.sections ?? []).map((id) => ({ id, spec: SPECS.get(id) }))
  const nodeEditable = (n: TreeNode) => sectionsOf(n).some(({ spec }) => spec?.fields.some((f) => editable(pathOf(spec, f))))

  /** The changes made here, by key, not yet saved. */
  let edits = $state<Record<string, { path: string[]; label: string; value: unknown }>>({})
  const changes = $derived(Object.values(edits).filter((e) => JSON.stringify(e.value) !== JSON.stringify(at(data?.effective, e.path))))
  const valueOf = (path: string[]) => (dotted(path) in edits ? edits[dotted(path)].value : at(data?.effective, path))
  function set(path: string[], label: string, value: unknown) {
    edits[dotted(path)] = { path, label, value }
    saved = null
  }

  let confirming = $state(false)
  let saving = $state(false)
  let saveError = $state<string | null>(null)
  let saved = $state<string | null>(null)
  /** An empty text goes back to the default: the key leaves config.toml. */
  const sent = (v: unknown) => (v === '' ? null : v)
  async function save() {
    saving = true
    saveError = null
    try {
      const list = changes.map((c) => ({ path: c.path, value: sent(c.value) }))
      if (!preview) data = await invoke<PhoneSettings>('remote_settings_save', { edits: list })
      else for (const c of list) c.path.reduce<Record<string, unknown>>((o, k, i) => (i === c.path.length - 1 ? ((o[k] = c.value), o) : (o[k] ??= {}) as Record<string, unknown>), data!.effective)
      saved = tn(list.length, '{n} change saved on the computer', '{n} changes saved on the computer')
      edits = {}
      confirming = false
    } catch (e) {
      saveError = e instanceof Error ? e.message : String(e)
    } finally {
      saving = false
    }
  }

  const show = (v: unknown): string => {
    if (v === undefined || v === null || v === '') return '—'
    if (Array.isArray(v)) return v.length ? v.map(show).join(', ') : '—'
    if (typeof v === 'object') return Object.entries(v as Record<string, unknown>).map(([k, x]) => `${k}: ${show(x)}`).join(' · ')
    return String(v)
  }
  /** A list's entry being typed, by key. */
  let adding = $state<Record<string, string>>({})
  function addTo(path: string[], label: string) {
    const k = dotted(path)
    const v = (adding[k] ?? '').trim()
    if (!v) return
    set(path, label, [...((valueOf(path) as string[] | undefined) ?? []), v])
    adding[k] = ''
  }

  const page = $derived(node ? NODES.get(node) : undefined)
  const roots = TREE
  const pagesOf = (root: TreeNode) => descendants(root).filter((n) => n !== root && n.sections?.length)
</script>

<div class="settings">
  {#if loadError}
    <p class="note ko">{loadError}</p>
    <button class="retry" onclick={load}>{t('Try again')}</button>
  {:else if !data}
    <p class="note">{t('Reading the computer’s settings…')}</p>
  {:else if !page}
    <p class="banner"><Icon name="lock" /> {t('Read on the computer. What is marked')} <span class="ed">{t('editable')}</span> {t('changes from here, each save confirmed by your passkey.')}</p>
    {#if data.error}<p class="note ko">{t('config.toml does not load: {error}', { error: data.error })}</p>{/if}
    {#each roots as root (root.id)}
      {@const pages = pagesOf(root)}
      {#if pages.length}
        <p class="sect">{root.label}</p>
        <ul class="list">
          {#each pages as n (n.id)}
            {@const own = sectionsOf(n).some(({ spec }) => spec)}
            <li><button onclick={() => open(n.id)}>
              <span class="dot" style="background: {root.color ?? 'var(--accent)'}"></span>
              <span class="t">{n.label}</span>
              {#if nodeEditable(n)}<span class="ed">{t('editable')}</span>{:else if own}<span class="ro">{t('read')}</span>{:else}<span class="ro">{t('on the computer')}</span>{/if}
              <Icon name="next" />
            </button></li>
          {/each}
        </ul>
      {/if}
    {/each}
  {:else}
    {@const any = nodeEditable(page)}
    <p class="banner" class:ok={any}>
      {#if any}<Icon name="edit" /> {t('The keys marked editable change from here; the others are read.')}
      {:else}<Icon name="lock" /> {t('Read-only on the phone: change it in Prism on the computer.')}{/if}
    </p>
    {#each sectionsOf(page) as { id, spec } (id)}
      {#if !spec}
        <p class="note">{id === 'file' ? t('config.toml is shown in Settings on the computer only.') : t('This part is shown in Settings on the computer only.')}</p>
      {:else}
        {#if sectionsOf(page).length > 1}<p class="sect">{spec.label}</p>{/if}
        <div class="box">
          {#each spec.fields as f (f.key)}
            {@const path = pathOf(spec, f)}
            {@const v = valueOf(path)}
            {@const can = editable(path)}
            {@const changed = dotted(path) in edits && JSON.stringify(edits[dotted(path)].value) !== JSON.stringify(at(data.effective, path))}
            <div class="field" class:changed>
              <div class="l">
                <b>{f.label}</b>
                {#if f.kind === 'toggle'}
                  <button class="sw" class:on={!!v} disabled={!can} role="switch" aria-checked={!!v} aria-label={f.label} onclick={() => set(path, `${spec.table} ${f.key}`, !v)}></button>
                {/if}
              </div>
              {#if f.kind === 'toggle'}
                <!-- the switch is in the label's line -->
              {:else if can && f.kind === 'select'}
                <select value={v ?? ''} onchange={(e) => set(path, `${spec.table} ${f.key}`, e.currentTarget.value)}>
                  {#if f.optional}<option value="">{f.placeholder ?? '—'}</option>{/if}
                  {#each Array.isArray(f.options) ? f.options : [] as o (o)}<option value={o}>{o}</option>{/each}
                </select>
              {:else if can && f.kind === 'number'}
                <span class="num"><input type="number" inputmode="numeric" step={f.step ?? 1} value={v ?? ''} onchange={(e) => set(path, `${spec.table} ${f.key}`, e.currentTarget.value === '' ? '' : Number(e.currentTarget.value))} />{#if f.unit}<small>{t(f.unit)}</small>{/if}</span>
              {:else if can && f.kind === 'text'}
                <input type="text" value={v ?? ''} placeholder={f.placeholder ?? ''} onchange={(e) => set(path, `${spec.table} ${f.key}`, e.currentTarget.value)} />
              {:else if can && f.kind === 'checks'}
                {@const on = (v as string[] | undefined) ?? []}
                <span class="chips">
                  {#each Array.isArray(f.options) ? f.options : [] as o (o)}
                    <button class="chip check" class:on={on.includes(o)} aria-pressed={on.includes(o)} onclick={() => set(path, `${spec.table} ${f.key}`, on.includes(o) ? on.filter((x) => x !== o) : [...on, o])}>{f.labels?.[o] ?? o}</button>
                  {/each}
                </span>
              {:else if can && f.kind === 'list'}
                <span class="chips">
                  {#each (v as string[] | undefined) ?? [] as item, i (i)}
                    <button class="chip" onclick={() => set(path, `${spec.table} ${f.key}`, ((v as string[]) ?? []).filter((_, j) => j !== i))} aria-label={t('Remove {name}', { name: item })}>{item} <Icon name="close" /></button>
                  {/each}
                  <input class="add" type="text" placeholder={`+ ${t('add')}`} bind:value={adding[dotted(path)]} onkeydown={(e) => e.key === 'Enter' && addTo(path, `${spec.table} ${f.key}`)} onblur={() => addTo(path, `${spec.table} ${f.key}`)} />
                </span>
              {:else}
                <span class="v">{show(v)}{#if f.unit && v !== undefined && v !== null && v !== ''} {t(f.unit)}{/if}</span>
              {/if}
              {#if f.hint && can}<span class="h">{f.hint}</span>{/if}
            </div>
          {/each}
        </div>
      {/if}
    {/each}
  {/if}
  {#if saved}<p class="note ok">{saved}</p>{/if}
</div>

{#if changes.length}
  <div class="save">
    <span>{tn(changes.length, '{n} change', '{n} changes')}</span>
    <button onclick={() => (edits = {})}>{t('Undo')}</button>
    <button class="pri" onclick={() => { saveError = null; confirming = true }}>{t('Save')}</button>
  </div>
{/if}

{#if confirming}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={() => !saving && (confirming = false)}></div>
  <div class="sheet" role="dialog" aria-label={t('Save on the computer')}>
    <b>{t('Save on the computer?')}</b>
    <div class="diff">
      {#each changes as c (dotted(c.path))}
        <div><code>{c.label}</code><span><code class="o">{show(at(data?.effective, c.path))}</code> → <code class="n">{show(sent(c.value) ?? tc('settings', 'default'))}</code></span></div>
      {/each}
    </div>
    <p class="fp">{preview ? t('Preview: nothing is saved.') : t('Confirmed by your passkey.')}</p>
    {#if saveError}<p class="note ko">{saveError}</p>{/if}
    <div class="btns">
      <button onclick={() => (confirming = false)} disabled={saving}>{t('Cancel')}</button>
      <button class="pri" onclick={save} disabled={saving}>{saving ? t('Saving…') : t('Save')}</button>
    </div>
  </div>
{/if}

<style>
  .settings { display: flex; flex-direction: column; gap: 10px; padding-bottom: 56px; }
  .banner { display: block; margin: 0; padding: 9px 12px; border-radius: 10px; background: var(--surface-muted); color: var(--text-muted); font-size: 12.5px; }
  .banner.ok { background: color-mix(in srgb, var(--ok-bg) 55%, transparent); color: var(--ok); }
  .banner :global(svg) { vertical-align: -2px; margin-right: 4px; }
  .sect { margin: 6px 4px -4px; color: var(--text-muted); font-size: 11px; letter-spacing: 0.05em; text-transform: uppercase; }
  .note { margin: 12px 4px; color: var(--text-muted); font-size: 13px; text-align: center; }
  .note.ko { color: var(--ko); }
  .note.ok { color: var(--ok); }
  .retry { align-self: center; padding: 8px 16px; border: 1px solid var(--border); border-radius: 18px; background: var(--surface); color: var(--text); font: inherit; }
  .ed { padding: 1px 7px; border-radius: 9px; background: var(--ok-bg); color: var(--ok); font-size: 11px; white-space: nowrap; }
  .ro { color: var(--text-muted); font-size: 11.5px; white-space: nowrap; }
  ul.list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; overflow: hidden; background: var(--surface); }
  ul.list li + li { border-top: 1px solid var(--border); }
  ul.list button { width: 100%; display: flex; align-items: center; gap: 10px; padding: 13px 14px; border: none; background: none; color: var(--text); font: inherit; font-size: 15px; text-align: left; }
  ul.list .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  ul.list :global(svg) { color: var(--text-muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }

  .box { border: 1px solid var(--border); border-radius: 12px; background: var(--surface); overflow: hidden; }
  .field { display: flex; flex-direction: column; gap: 5px; padding: 11px 14px; }
  .field + .field { border-top: 1px solid var(--border); }
  .field.changed { background: color-mix(in srgb, var(--accent) 8%, transparent); box-shadow: inset 3px 0 var(--accent); }
  .l { display: flex; align-items: center; gap: 10px; }
  .l b { flex: 1; font-weight: 500; font-size: 14.5px; }
  .v { font-family: var(--mono); font-size: 13px; color: var(--text-muted); overflow-wrap: anywhere; }
  .h { color: var(--text-muted); font-size: 12px; }
  input[type='text'], input[type='number'], select { width: 100%; padding: 9px 10px; border: 1px solid var(--border); border-radius: 8px; background: var(--bg); color: var(--text); font: 14px var(--mono); }
  input:focus, select:focus { outline: none; border-color: var(--accent); }
  .num { display: flex; align-items: center; gap: 8px; }
  .num input { flex: 1; }
  .num small { color: var(--text-muted); font-size: 12px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 4px 8px 4px 11px; border: 1px solid var(--border); border-radius: 14px; background: none; color: var(--text); font: 13px var(--mono); }
  .chip :global(svg) { color: var(--text-muted); }
  .chip.check { padding-right: 11px; color: var(--text-muted); }
  .chip.check.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--text); }
  .chips input.add { width: 96px; padding: 4px 10px; border-style: dashed; border-radius: 14px; font: 13px var(--sans); }
  .sw { flex: none; width: 42px; height: 24px; padding: 0; border-radius: 12px; background: var(--surface-muted); border: 1px solid var(--border); position: relative; }
  .sw::after { content: ''; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: var(--text-muted); transition: left 0.12s; }
  .sw.on { background: var(--ok); border-color: var(--ok); }
  .sw.on::after { left: 20px; background: var(--surface); }
  .sw:disabled { opacity: 0.45; }

  .save { position: fixed; left: 0; right: 0; bottom: calc(58px + env(safe-area-inset-bottom)); z-index: 6; display: flex; align-items: center; gap: 10px; padding: 10px 14px; background: var(--surface); border-top: 1px solid var(--border); box-shadow: 0 -6px 16px rgba(0, 0, 0, 0.18); }
  .save span { flex: 1; color: var(--text-muted); font-size: 13px; }
  .save button, .btns button { padding: 8px 16px; border: 1px solid var(--border); border-radius: 18px; background: var(--surface); color: var(--text); font: inherit; font-size: 14px; }
  button.pri { background: var(--accent); border-color: var(--accent); color: #fff; }
  .scrim { position: fixed; inset: 0; z-index: 40; background: rgba(0, 0, 0, 0.45); }
  .sheet { position: fixed; left: 0; right: 0; bottom: 0; z-index: 41; display: flex; flex-direction: column; gap: 10px; padding: 16px 16px calc(20px + env(safe-area-inset-bottom)); border-radius: 18px 18px 0 0; background: var(--surface); box-shadow: 0 -10px 30px rgba(0, 0, 0, 0.35); }
  .sheet b { font-size: 16px; }
  .diff { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; font-size: 12.5px; }
  .diff div { display: flex; flex-direction: column; gap: 2px; padding: 8px 10px; }
  .diff div + div { border-top: 1px solid var(--border); }
  .diff code { font-family: var(--mono); overflow-wrap: anywhere; }
  .diff .o { color: var(--ko); text-decoration: line-through; }
  .diff .n { color: var(--ok); }
  .fp { margin: 0; color: var(--text-muted); font-size: 12.5px; text-align: center; }
  .btns { display: flex; gap: 10px; justify-content: flex-end; }
</style>
