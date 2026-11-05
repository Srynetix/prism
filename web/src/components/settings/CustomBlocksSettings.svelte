<script lang="ts">
  import { call } from '../../blocks/block'
  import CustomOutput from '../../blocks/custom/CustomOutput.svelte'
  import { customState, SOURCE, type CustomAction, type CustomBlockDef, type CustomBlockView, type CustomRead } from '../../blocks/custom/types'
  import { openLauncher } from '../../agents.svelte'
  import { live } from '../../live.svelte'
  import { toast } from '../../toast.svelte'
  import { ago } from '../../time'
  import { t } from '../../i18n.svelte'
  import ScriptEditor from './ScriptEditor.svelte'

  /**
   * Settings › Interface › Custom blocks: one line per `[[custom_blocks]]` entry (its state, Run
   * now, its last output), and its editor — Command | Script, the script in CodeMirror with
   * starting templates, Run now on the text being edited, Write with an agent. Saved here, a block
   * is trusted as saved; changed anywhere else, it waits for trust.
   */
  const custom = $derived(customState(live.snapshot))
  const blocks = $derived(custom?.blocks ?? [])

  interface ActionRow { id: string; label: string; kind: 'command' | 'script' | 'agent'; value: string; when: string; critical: boolean }
  interface Draft {
    former: string
    id: string
    label: string
    mode: 'command' | 'script'
    command: string
    script: string
    args: string
    pace: 'every' | 'demand'
    every: number
    timeout: number
    cwd: string
    env: { name: string; value: string }[]
    remote: boolean
    enabled: boolean
    actions: ActionRow[]
  }

  const TEMPLATES: { id: string; label: string; text: string }[] = [
    { id: 'bash', label: 'bash', text: '#!/usr/bin/env bash\n# A custom block prints JSON: kind list, gauge, stats, markdown or table.\nset -euo pipefail\nused=$(df -P "$HOME" | awk \'NR==2 {gsub("%",""); print $5}\')\nprintf \'{"kind":"gauge","label":"Disk","value":%s,"max":100,"unit":"%%","warn":80,"ko":90}\\n\' "$used"\n' },
    { id: 'python', label: 'python', text: '#!/usr/bin/env python3\n# A custom block prints JSON: kind list, gauge, stats, markdown or table.\nimport json, shutil\n\ntotal, used, free = shutil.disk_usage("/")\nprint(json.dumps({"kind": "stats", "stats": [\n    {"label": "Disk used", "value": round(used / total * 100), "max": 100, "unit": "%", "warn": 80, "ko": 90},\n    {"label": "Free (GB)", "value": round(free / 1e9)},\n]}))\n' },
    { id: 'uv', label: 'uv', text: '#!/usr/bin/env -S uv run --script\n# /// script\n# dependencies = ["httpx"]\n# ///\n# A custom block prints JSON: kind list, gauge, stats, markdown or table.\nimport json, httpx\n\nsummary = httpx.get("https://www.githubstatus.com/api/v2/summary.json", timeout=10).json()\nitems = [{"title": c["name"], "sub": c["status"], "state": "ok" if c["status"] == "operational" else "warn"} for c in summary["components"][:8]]\nalerts = [{"severity": "warn", "text": f"GitHub: {i[\'title\']} {i[\'sub\']}", "key": i["title"]} for i in items if i["state"] != "ok"]\nprint(json.dumps({"kind": "list", "items": items, "alerts": alerts, "status": summary["status"]["description"]}))\n' },
    { id: 'node', label: 'node', text: "#!/usr/bin/env node\n// A custom block prints JSON: kind list, gauge, stats, markdown or table.\nconst os = require('os')\nconst [one, five, fifteen] = os.loadavg().map((v) => Math.round(v * 100) / 100)\nconsole.log(JSON.stringify({ kind: 'table', columns: ['window', { key: 'load', label: 'Load' }], rows: [['1 min', one], ['5 min', five], ['15 min', fifteen]] }))\n" },
  ]

  const blank = (): Draft => ({ former: '', id: '', label: '', mode: 'script', command: '', script: TEMPLATES[1].text, args: '', pace: 'every', every: 300, timeout: 20, cwd: '', env: [], remote: true, enabled: true, actions: [] })

  function fromDef(d: CustomBlockDef): Draft {
    return {
      former: d.id, id: d.id, label: d.label ?? '', mode: d.script != null ? 'script' : 'command', command: (d.command ?? []).join('\n'), script: d.script ?? '', args: (d.args ?? []).join('\n'),
      pace: d.on_demand ? 'demand' : 'every', every: d.every_seconds ?? 300, timeout: d.timeout_seconds, cwd: d.cwd ?? '',
      env: Object.entries(d.env ?? {}).map(([name, value]) => ({ name, value })), remote: d.remote, enabled: d.enabled,
      actions: (d.actions ?? []).map((a) => ({ id: a.id, label: a.label ?? '', kind: a.agent != null ? 'agent' : a.script != null ? 'script' : 'command', value: a.agent ?? a.script ?? (a.command ?? []).join('\n'), when: a.when ?? '', critical: !!a.critical })),
    }
  }

  const lines = (s: string) => s.split('\n').map((l) => l.trim()).filter(Boolean)

  function toDef(d: Draft): CustomBlockDef {
    const actions: CustomAction[] = d.actions.filter((a) => a.id.trim()).map((a) => ({
      id: a.id.trim(), ...(a.label.trim() ? { label: a.label.trim() } : {}), ...(a.when.trim() ? { when: a.when.trim() } : {}), ...(a.critical ? { critical: true } : {}),
      ...(a.kind === 'agent' ? { agent: a.value } : a.kind === 'script' ? { script: a.value.endsWith('\n') ? a.value : `${a.value}\n` } : { command: lines(a.value) }),
    }))
    const env = Object.fromEntries(d.env.filter((e) => e.name.trim()).map((e) => [e.name.trim(), e.value]))
    return {
      id: d.id.trim(),
      ...(d.label.trim() ? { label: d.label.trim() } : {}),
      ...(d.mode === 'script' ? { script: d.script.endsWith('\n') ? d.script : `${d.script}\n` } : { command: lines(d.command) }),
      ...(lines(d.args).length ? { args: lines(d.args) } : {}),
      ...(d.pace === 'demand' ? { on_demand: true } : { every_seconds: Math.max(5, Math.round(d.every)) }),
      timeout_seconds: Math.max(1, Math.round(d.timeout)),
      ...(d.cwd.trim() ? { cwd: d.cwd.trim() } : {}),
      ...(Object.keys(env).length ? { env } : {}),
      remote: d.remote,
      enabled: d.enabled,
      ...(actions.length ? { actions } : {}),
    }
  }

  let draft = $state<Draft | null>(null)
  let selected = $state<CustomBlockView | null>(null)
  let tried = $state<CustomRead | null>(null)
  let trying = $state(false)
  let saving = $state(false)
  let armed = $state<string | null>(null)
  let describe = $state('')
  let agentDraft = $state<{ path: string; text: string | null } | null>(null)

  function edit(b: CustomBlockView) {
    selected = b
    draft = b.def ? fromDef(b.def) : null
    tried = null
    agentDraft = null
  }
  function create() {
    selected = null
    draft = blank()
    tried = null
    agentDraft = null
  }

  /** A secret written in clear in `env` (the block refuses it): said before saving. */
  const SECRET = /TOKEN|KEY|PASSWORD|PASSWD|SECRET/i
  const clearSecret = $derived(draft?.env.find((e) => SECRET.test(e.name) && e.value && !e.value.startsWith('keychain:')))

  async function tryIt() {
    if (!draft) return
    trying = true
    try {
      tried = await call<CustomRead>(SOURCE, 'try', { def: toDef({ ...draft, id: draft.id.trim() || 'try' }) })
    } catch (e) {
      toast(String(e), 8000)
    } finally {
      trying = false
    }
  }

  async function save() {
    if (!draft) return
    saving = true
    try {
      await call(SOURCE, 'save', { def: toDef(draft), former: draft.former })
      toast(t('{name} saved and trusted: it runs now', { name: draft.label || draft.id }))
      draft.former = draft.id.trim()
    } catch (e) {
      toast(String(e), 8000)
    } finally {
      saving = false
    }
  }

  async function remove() {
    if (!draft?.former) return
    if (armed !== 'delete') {
      armed = 'delete'
      setTimeout(() => (armed = null), 4000)
      return
    }
    armed = null
    try {
      await call(SOURCE, 'delete', { id: draft.former })
      toast(t('{name} removed from config.toml', { name: draft.label || draft.former }))
      draft = null
      selected = null
    } catch (e) {
      toast(String(e), 8000)
    }
  }

  async function runSaved(b: CustomBlockView) {
    try {
      await call(SOURCE, 'run', { id: b.id })
    } catch (e) {
      toast(String(e))
    }
  }

  /** Write with an agent: the contract and my description; it writes the script where Load finds it. */
  async function withAgent() {
    if (!draft) return
    const id = draft.id.trim() || 'draft'
    try {
      const d = await call<{ path: string; text: string | null }>(SOURCE, 'draft', { id, clear: true })
      agentDraft = d
      const prompt = [
        `Write a script for a Prism custom block. What it should show: ${describe.trim() || '(I will describe it next)'}`,
        '',
        'The script runs on its own (no arguments, no input), with a shebang choosing its interpreter (bash, python3, node, or `#!/usr/bin/env -S uv run --script` with PEP 723 dependencies). It prints ONE JSON object on stdout:',
        '- {"kind": "list", "items": [{"id", "title", "sub", "state": "ok|warn|ko", "badge", "url"}]}',
        '- {"kind": "gauge", "label", "value", "max", "unit", "warn", "ko"} or {"kind": "stats", "stats": [{"key", "label", "value", "max", "unit", "warn", "ko"}]}',
        '- {"kind": "markdown", "text"} or {"kind": "table", "columns": ["name" | {"key", "label"}], "rows": [[...] | {...}]}',
        'Optional at the top level: "alerts": [{"severity": "ok|warn|ko", "text", "key"}] and "status": {"text", "state"}. Links are http(s) only. Secrets come from environment variables, never written in the script.',
        '',
        `Write only the script, whole, to the file ${d.path} (create its folder if needed). Do not edit Prism's config.toml: I review the script in Settings before saving it.`,
      ].join('\n')
      openLauncher({ title: t('Custom block {name}', { name: draft.label || id }), prompt, worktree: {} }, null, null, 'custom-blocks')
    } catch (e) {
      toast(String(e))
    }
  }
  async function loadDraft() {
    if (!draft) return
    try {
      const d = await call<{ path: string; text: string | null }>(SOURCE, 'draft', { id: draft.id.trim() || 'draft' })
      agentDraft = d
      if (d.text) {
        draft.mode = 'script'
        draft.script = d.text
        toast(t('The agent’s script is in the editor: Run now, then Save'))
      } else toast(t('The agent has not written it yet'))
    } catch (e) {
      toast(String(e))
    }
  }

  const tryView = $derived<CustomBlockView | null>(tried ? { id: draft?.id || 'try', label: draft?.label || '', origin: 'config', enabled: true, on_demand: false, every_seconds: 0, remote: true, source: '', running: false, output: tried.output, history: {}, actions: [] } : null)
</script>

<p class="intro">{t('Your own blocks: a command, or a script written here, that prints JSON (list, gauge, stats, markdown or table). Each one is a page block, custom:<id> in + Block. Saved here, a block is trusted; added or changed outside Settings (an editor, an agent, a pack), it waits for your trust before it runs.')}</p>

<div class="card list">
  {#each custom?.errors ?? [] as e (e)}<div class="cfgerr">{e}</div>{/each}
  {#each blocks as b (b.id)}
    <div class="blk" class:sel={selected?.id === b.id}>
      <span class="dot" class:ko={!!b.error} class:warn={!!b.pending} class:off={!b.enabled}></span>
      <button class="name" onclick={() => edit(b)}><b>{b.label}</b> <code>custom:{b.id}</code></button>
      <span class="meta">{b.pending ? t('waits for your trust') : !b.enabled ? t('off') : b.error ? b.error.split('\n')[0] : b.ran_at ? t('read {time} ago', { time: ago(b.ran_at) }) : t('not read yet')}{b.origin === 'pack' ? ` · ${t('from a pack')}` : ''}</span>
      {#if !b.pending && b.enabled}<button class="btn ghost" onclick={() => runSaved(b)}>{t('Run now')}</button>{/if}
    </div>
  {:else}
    <div class="none">{t('No custom block yet.')}</div>
  {/each}
  <div class="add"><button class="btn" onclick={create}>{t('+ New custom block')}</button></div>
</div>

{#if selected && !draft}
  <p class="note">{t('This block does not read: fix it in config.toml (Config file), its error is above.')}</p>
{/if}

{#if draft}
  {@const d = draft}
  <div class="card editor">
    {#if selected?.origin === 'pack'}<p class="note">{t('This block comes from a pack: saving writes your own version in config.toml, over the pack’s.')}</p>{/if}
    <div class="grid">
      <label>{t('Id')}<input bind:value={d.id} placeholder="backups" spellcheck="false" /></label>
      <label>{t('Name')}<input bind:value={d.label} placeholder={t('Backups')} /></label>
    </div>
    <div class="seg" role="radiogroup" aria-label={t('What it runs')}>
      <button role="radio" aria-checked={d.mode === 'command'} class:on={d.mode === 'command'} onclick={() => (d.mode = 'command')}>{t('Command')}</button>
      <button role="radio" aria-checked={d.mode === 'script'} class:on={d.mode === 'script'} onclick={() => (d.mode = 'script')}>{t('Script')}</button>
    </div>
    {#if d.mode === 'command'}
      <label>{t('Command')}<small>{t('The program, then one argument per line: run as is, never through a shell (~ allowed).')}</small>
        <textarea rows="3" bind:value={d.command} placeholder={'~/bin/check-backups.sh\n--json'} spellcheck="false"></textarea></label>
    {:else}
      <div class="tpl">
        <span>{t('Start from')}</span>
        {#each TEMPLATES as tpl (tpl.id)}<button class="btn ghost" onclick={() => (d.script = tpl.text)}>{tpl.label}</button>{/each}
      </div>
      <ScriptEditor bind:value={d.script} label={t('Script')} />
      <small class="hint">{t('Run by its shebang (bash, python3, node, uv run --script with its dependencies…); without one, /bin/sh. Prism writes it in its data folder, private, and runs it from there.')}</small>
      <div class="agent">
        <input bind:value={describe} placeholder={t('What should it show? (for Write with an agent)')} />
        <button class="btn" onclick={withAgent}>{t('Write with an agent')}</button>
        {#if agentDraft}<button class="btn ghost" onclick={loadDraft}>{t('Load the agent’s script')}</button>{/if}
      </div>
    {/if}
    <div class="grid">
      <label>{t('Arguments')}<small>{t('One per line, after the command’s or given to the script.')}</small><textarea rows="2" bind:value={d.args} spellcheck="false"></textarea></label>
      <label>{t('Folder')}<small>{t('Where it runs; empty: the home folder.')}</small><input bind:value={d.cwd} placeholder="~/infra" spellcheck="false" /></label>
    </div>
    <div class="grid">
      <label>{t('Read')}
        <select bind:value={d.pace}><option value="every">{t('every…')}</option><option value="demand">{t('on demand (↻) and at launch')}</option></select>
      </label>
      {#if d.pace === 'every'}<label>{t('Every (seconds)')}<input type="number" min="5" bind:value={d.every} /></label>{/if}
      <label>{t('Timeout (seconds)')}<input type="number" min="1" max="600" bind:value={d.timeout} /></label>
    </div>
    <div class="env">
      <span class="lbl">{t('Environment')}<small>{t('A secret by reference: keychain:<service> (never in clear).')}</small></span>
      {#each d.env as e, i (i)}
        <div class="row"><input bind:value={e.name} placeholder="TOKEN" spellcheck="false" /><input bind:value={e.value} placeholder="keychain:prism-backup" spellcheck="false" /><button class="btn ghost" onclick={() => d.env.splice(i, 1)} aria-label={t('Remove')}>×</button></div>
      {/each}
      <button class="btn ghost" onclick={() => d.env.push({ name: '', value: '' })}>{t('+ Variable')}</button>
      {#if clearSecret}<p class="warn">{t('{name} looks like a secret written in clear: the block refuses it. Store it in the keychain and write keychain:<service>.', { name: clearSecret.name })}</p>{/if}
    </div>
    <div class="actions">
      <span class="lbl">{t('Actions')}<small>{t('Buttons on the lines they apply to (when: state != \'ok\'). The line is given as PRISM_ITEM; {item.id} only as a whole argument. An agent opens in the launcher, the line as data.')}</small></span>
      {#each d.actions as a, i (i)}
        <div class="act">
          <div class="row">
            <input bind:value={a.id} placeholder="fix" spellcheck="false" />
            <input bind:value={a.label} placeholder={t('Label')} />
            <select bind:value={a.kind}><option value="command">{t('Command')}</option><option value="script">{t('Script')}</option><option value="agent">{t('Agent')}</option></select>
            <input bind:value={a.when} placeholder="state != 'ok'" spellcheck="false" />
            <label class="chk"><input type="checkbox" bind:checked={a.critical} />{t('Critical')}</label>
            <button class="btn ghost" onclick={() => d.actions.splice(i, 1)} aria-label={t('Remove')}>×</button>
          </div>
          <textarea rows={a.kind === 'command' ? 2 : 4} bind:value={a.value} spellcheck="false" placeholder={a.kind === 'agent' ? t('Fix the backup of {item.title}') : a.kind === 'script' ? '#!/bin/sh\n…' : '~/bin/backup.sh\n{item.id}'}></textarea>
        </div>
      {/each}
      <button class="btn ghost" onclick={() => d.actions.push({ id: '', label: '', kind: 'command', value: '', when: '', critical: false })}>{t('+ Action')}</button>
    </div>
    <div class="flags">
      <label class="chk"><input type="checkbox" bind:checked={d.enabled} />{t('On')}</label>
      <label class="chk"><input type="checkbox" bind:checked={d.remote} />{t('On the phone')}</label>
    </div>
    <div class="btns">
      <button class="btn" onclick={tryIt} disabled={trying}>{trying ? t('Running…') : t('Run now')}</button>
      <button class="btn primary" onclick={save} disabled={saving || !d.id.trim()}>{t('Save')}</button>
      {#if d.former}<button class="btn danger" class:armed={armed === 'delete'} onclick={remove}>{armed === 'delete' ? t('Click again to remove') : t('Remove')}</button>{/if}
      <button class="btn ghost" onclick={() => ((draft = null), (selected = null))}>{t('Close')}</button>
    </div>
    {#if tried}
      <div class="tried">
        <div class="head">{tried.error ? t('Failed') : t('Its output')} · {tried.took_ms} ms{#if tried.code != null} · {t('exit code {code}', { code: tried.code })}{/if}</div>
        {#if tried.error}<pre class="err">{tried.error}</pre>{/if}
        {#if tryView}<div class="preview"><CustomOutput block={tryView} acts={false} /></div>{/if}
        {#if tried.stderr}<details><summary>stderr</summary><pre>{tried.stderr}</pre></details>{/if}
      </div>
    {:else if selected?.output || selected?.stderr || selected?.error}
      <details class="tried">
        <summary>{t('Last output')}{selected.ran_at ? ` · ${ago(selected.ran_at)}` : ''}</summary>
        {#if selected.error}<pre class="err">{selected.error}</pre>{/if}
        {#if selected.output}<pre>{JSON.stringify(selected.output, null, 2)}</pre>{/if}
        {#if selected.stderr}<pre>{selected.stderr}</pre>{/if}
      </details>
    {/if}
  </div>
{/if}

<style>
  .list { display: flex; flex-direction: column; padding: 0; }
  .blk { display: flex; align-items: center; gap: 8px; padding: 7px 12px; border-top: 1px solid var(--border); }
  .blk:first-child { border-top: none; }
  .blk.sel { background: var(--surface-muted); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); flex-shrink: 0; }
  .dot.ko { background: var(--ko); }
  .dot.warn { background: var(--warn); }
  .dot.off { background: var(--text-muted); opacity: 0.5; }
  .name { padding: 0; border: none; background: none; color: var(--text); font: inherit; cursor: pointer; text-align: left; }
  .name code { color: var(--text-muted); font-size: var(--fs-meta); }
  .meta { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); font-size: var(--fs-meta); }
  .none { padding: 10px 12px; color: var(--text-muted); }
  .add { padding: 8px 12px; border-top: 1px solid var(--border); }
  .cfgerr { padding: 6px 12px; background: var(--ko-bg); color: var(--ko); font-size: var(--fs-meta); }
  .editor { display: flex; flex-direction: column; gap: 10px; margin-top: 12px; padding: 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 10px; }
  label { display: flex; flex-direction: column; gap: 3px; font-size: var(--fs-meta); color: var(--text-muted); }
  label small, .lbl small, .hint { color: var(--text-muted); font-size: var(--fs-tag); }
  .lbl { display: flex; flex-direction: column; gap: 2px; font-size: var(--fs-meta); color: var(--text-muted); }
  input, textarea, select { padding: 4px 8px; border: 1px solid var(--border); border-radius: var(--r5); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); }
  textarea { font-family: var(--font-mono); font-size: var(--fs-meta); resize: vertical; }
  .seg { display: inline-flex; align-self: flex-start; gap: 1px; padding: 2px; border-radius: var(--r6); background: var(--surface-muted); }
  .seg button { padding: 3px 12px; border: none; border-radius: var(--r5); background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12); }
  .tpl, .agent { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: var(--fs-meta); color: var(--text-muted); }
  .agent input { flex: 1; min-width: 200px; }
  .env, .actions { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; gap: 6px; align-items: center; }
  .row input { flex: 1; min-width: 0; }
  .act { display: flex; flex-direction: column; gap: 4px; padding: 6px; border: 1px solid var(--border); border-radius: var(--r6); }
  .chk { flex-direction: row; align-items: center; gap: 5px; white-space: nowrap; }
  .flags { display: flex; gap: 14px; }
  .btns { display: flex; flex-wrap: wrap; gap: 6px; }
  .btn.armed { outline: 2px solid var(--warn); }
  .btn.danger { color: var(--ko); }
  .warn { margin: 0; color: var(--warn); font-size: var(--fs-meta); }
  .tried { border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; }
  .tried .head, .tried summary { padding: 5px 10px; background: var(--surface-muted); font-size: var(--fs-meta); color: var(--text-muted); cursor: default; }
  .tried summary { cursor: pointer; }
  .tried pre { margin: 0; padding: 6px 10px; max-height: 240px; overflow: auto; font-family: var(--font-mono); font-size: var(--fs-meta); white-space: pre-wrap; word-break: break-word; }
  .tried pre.err { color: var(--ko); }
  .preview { max-height: 320px; overflow: auto; }
</style>
