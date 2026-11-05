<script lang="ts">
  import { live } from '../../live.svelte'
  import { agentsSettings } from '../../agents.svelte'
  import { openSettings, saveEdits } from '../../settings.svelte'
  import { toast } from '../../toast.svelte'
  import { ago } from '../../time'
  import { t, tn } from '../../i18n.svelte'
  import { BUILTINS, builtinLabel, cronWords, enableRoutine, everyWords, nextTime, removeRoutine, routinesOf, runRoutine, runTime, STATE_ICON, STATE_LABEL, trustRoutine, whenWords } from '../../routines.svelte'
  import type { Routine } from '../../bindings/Routine'
  import type { RoutineView } from '../../bindings/RoutineView'

  /**
   * Settings › Agents › Routines: one line per `[[agents.routines]]` (on, its action, when in
   * words, where its result goes, its last run, Run now), the routines waiting for trust with what
   * changed, the editor (Cron | Every…, the cron in words and its next times), and the last day's
   * runs. Saved here, a routine is trusted as saved; added or changed anywhere else (an editor, an
   * agent, a pack) — it or its quick action — it waits for trust before it runs.
   */
  const rs = $derived(routinesOf(live.snapshot))
  const list = $derived(agentsSettings().routines ?? [])
  const actions = $derived(agentsSettings().quick_actions ?? [])
  const repos = $derived((live.snapshot?.repos ?? []).map((r) => r.name))
  const viewOf = (name: string): RoutineView | undefined => rs.routines.find((r) => r.name === name)
  const lastRun = (name: string) => rs.runs.find((r) => r.routine === name && r.state !== 'skipped')
  const pending = $derived(rs.routines.filter((r) => r.pending))

  interface Draft {
    former: string
    name: string
    action: string
    repos: string[]
    pace: 'cron' | 'every'
    cron: string
    every: string
    time_zone: string
    deliver: 'review' | 'digest'
    missed: 'skip' | 'once'
    max_minutes: number
    enabled: boolean
  }
  const blank = (): Draft => ({ former: '', name: '', action: actions[0]?.name ?? BUILTINS[0], repos: [], pace: 'cron', cron: '0 3 * * 1-5', every: '6h', time_zone: '', deliver: 'review', missed: 'once', max_minutes: 60, enabled: true })
  const fromRoutine = (r: Routine): Draft => ({
    former: r.name, name: r.name, action: r.action, repos: [...r.repos], pace: r.every?.trim() && !r.cron?.trim() ? 'every' : 'cron', cron: r.cron ?? '', every: r.every ?? '6h', time_zone: r.time_zone ?? '',
    deliver: r.deliver, missed: r.missed, max_minutes: r.max_minutes, enabled: r.enabled,
  })
  /** As config.toml holds it: the keys left empty out. */
  function toRow(d: Draft): Record<string, unknown> {
    return {
      name: d.name.trim(),
      action: d.action,
      ...(d.repos.length && !d.action.startsWith('builtin:') ? { repos: d.repos } : {}),
      ...(d.pace === 'cron' ? { cron: d.cron.trim() } : { every: d.every.trim() }),
      ...(d.time_zone.trim() && d.pace === 'cron' ? { time_zone: d.time_zone.trim() } : {}),
      deliver: d.deliver,
      missed: d.missed,
      max_minutes: Math.max(1, Math.round(d.max_minutes)),
      enabled: d.enabled,
    }
  }
  /** A routine as a row, its empty keys out. */
  const asRow = (r: Routine) => toRow(fromRoutine(r))

  let draft = $state<Draft | null>(null)
  let saving = $state(false)
  let armed = $state<string | null>(null)
  const arm = (what: string) => {
    armed = what
    setTimeout(() => armed === what && (armed = null), 4000)
  }

  const qa = $derived(draft ? actions.find((a) => a.name === draft!.action) : undefined)
  const builtin = $derived(!!draft?.action.startsWith('builtin:'))
  const savedView = $derived(draft?.former ? viewOf(draft.former) : undefined)
  /** Unchanged since saved: its next times are the core's. */
  const unchanged = $derived(!!draft && !!draft.former && JSON.stringify(toRow(draft)) === JSON.stringify(asRow(list.find((r) => r.name === draft!.former)!)))
  const words = $derived(draft ? (draft.pace === 'cron' ? cronWords(draft.cron) : everyWords(draft.every)) : '')
  const taken = $derived(!!draft && list.some((r) => r.name === draft!.name.trim() && r.name !== draft!.former))

  async function write(rows: Record<string, unknown>[], done: string) {
    saving = true
    try {
      await saveEdits([{ path: ['agents', 'routines'], value: rows }])
      toast(done)
      return true
    } catch (e) {
      toast(String(e), 8000)
      return false
    } finally {
      saving = false
    }
  }

  async function save() {
    if (!draft || !draft.name.trim() || taken) return
    const row = toRow(draft)
    const rows = draft.former ? list.map((r) => (r.name === draft!.former ? row : asRow(r))) : [...list.map(asRow), row]
    if (await write(rows, t('{name} saved and trusted', { name: draft.name.trim() }))) draft.former = draft.name.trim()
  }

  async function remove() {
    if (!draft?.former) return
    if (armed !== 'delete') return arm('delete')
    armed = null
    const name = draft.former
    if (await write(list.filter((r) => r.name !== name).map(asRow), t('{name} removed from config.toml', { name }))) draft = null
  }

  async function removePending(name: string) {
    if (armed !== `rm:${name}`) return arm(`rm:${name}`)
    armed = null
    await removeRoutine(name)
  }

  const REASON = { new: t('New routine, not trusted yet'), modified: t('Changed outside Settings: suspended'), action_modified: t('Its quick action changed outside Settings: suspended') }
  const since = $derived(Date.now() - 86_400_000)
  const history = $derived(rs.runs.filter((r) => Date.parse(r.ended ?? r.started ?? r.due) > since))
  const actionText = (r: Routine) => (r.action.startsWith('builtin:') ? builtinLabel(r.action) : r.action) + ((viewOf(r.name)?.repos.length ?? 0) ? ` · ${viewOf(r.name)!.repos.join(', ')}` : '')
</script>

<p class="intro">{t('Agents launched on their own, at fixed times: a quick action (or one of Prism’s) run by a cron or at a pace, without you. Its result waits in To review (the Agents view, the Routines block) or goes to the Digest; blocked or failed, it always waits in To review. Saved here, a routine is trusted; added or changed outside Settings (an editor, an agent, a pack), it or its quick action, it waits for your trust before it runs.')}</p>

{#if !rs.keep_running}
  <div class="banner">
    <b>{t('Prism closed = no routine.')}</b> {t('They run while Prism does, its window open or not. Turn on Keep running (Settings › Remote) so that Prism opens at login and again after a crash.')}
    <button class="lnk" onclick={() => openSettings('remote')}>{t('Settings › Remote')}</button>
  </div>
{:else}
  <p class="ok">{t('Keep running is on: the routines run with the window closed.')}</p>
{/if}

{#each pending as p (p.name)}
  {@const pe = p.pending!}
  <div class="card trust">
    <div class="th"><span class="warn">⚠</span> <b>{p.name}</b> <span class="muted">{REASON[pe.reason]}</span></div>
    <p class="muted">{t('Seen {time} ago.', { time: ago(pe.since) })}{#if pe.agents.length}{' '}{t('Prism agents were working near config.toml then: {agents}.', { agents: pe.agents.join(', ') })}{/if}</p>
    <pre class="diff">{#each pe.diff as l, i (i)}<span class:add={l.startsWith('+')} class:del={l.startsWith('-')}>{l}
</span>{/each}</pre>
    <div class="btns">
      <button class="btn primary" onclick={() => trustRoutine(p.name, pe.hash)}>{pe.reason === 'action_modified' ? t('Trust both') : t('Trust')}</button>
      {#if pe.reason === 'new'}
        <button class="btn danger" class:armed={armed === `rm:${p.name}`} onclick={() => removePending(p.name)}>{armed === `rm:${p.name}` ? t('Click again to remove') : t('Remove')}</button>
      {:else if p.enabled}
        <button class="btn ghost" onclick={() => enableRoutine(p.name, false)}>{t('Turn off')}</button>
      {/if}
    </div>
  </div>
{/each}

<div class="card list">
  <div class="head"><span>{t('On')}</span><span>{t('Name')}</span><span>{t('Action')}</span><span>{t('When')}</span><span>{t('Result')}</span><span>{t('Last')}</span><span></span></div>
  {#each list as r (r.name)}
    {@const v = viewOf(r.name)}
    {@const last = lastRun(r.name)}
    <div class="line" class:sel={draft?.former === r.name}>
      <input type="checkbox" checked={r.enabled} onchange={(e) => enableRoutine(r.name, e.currentTarget.checked)} aria-label={t('On')} />
      <button class="name" onclick={() => (draft = fromRoutine(r))}><b>{r.name}</b></button>
      <span class="act">{actionText(r)}</span>
      <span class="when">{whenWords(r)}<code>{r.cron?.trim() || (r.every ? `every ${r.every}` : '')}</code>{#if v?.next[0]}<small>{t('next: {time}', { time: nextTime(v.next[0]) })}</small>{/if}</span>
      <span>{r.deliver === 'digest' ? t('Digest') : t('To review')}</span>
      <span class="last">{#if v?.error}<span class="ko" title={v.error}>{t('in error')}</span>{:else if v?.pending}<span class="warn">{t('waits for your trust')}</span>{:else if last}<span class="st {last.state}">{STATE_LABEL[last.state]}</span> <small>{ago(last.ended ?? last.due)}</small>{:else}<small class="muted">{t('not run yet')}</small>{/if}</span>
      <button class="btn ghost" disabled={!!v?.pending || !!v?.error} onclick={() => runRoutine(r.name)} title={t('The routine as saved, now: its result arrives as at its time')}>{t('Run now')}</button>
    </div>
    {#if v?.error}<div class="cfgerr">{r.name}: {v.error}</div>{/if}
  {:else}
    <div class="none">{t('No routine yet.')}{#if !actions.length}{' '}{t('A routine runs a quick action: write one first (Settings › Quick actions), or pick one of Prism’s.')}{/if}</div>
  {/each}
  <div class="add"><button class="btn" onclick={() => (draft = blank())}>{t('+ New routine')}</button></div>
</div>

{#if draft}
  {@const d = draft}
  <div class="card editor">
    <div class="grid">
      <label>{t('Name')}<input bind:value={d.name} placeholder={t('Nightly deps')} />{#if taken}<small class="ko">{t('Another routine has this name.')}</small>{/if}</label>
      <label>{t('Action')}
        <select bind:value={d.action}>
          {#each actions as a (a.name)}<option value={a.name}>{a.name}</option>{/each}
          {#each BUILTINS as b (b)}<option value={b}>{builtinLabel(b)} ({b})</option>{/each}
          {#if d.action && !actions.some((a) => a.name === d.action) && !(BUILTINS as readonly string[]).includes(d.action)}<option value={d.action}>{d.action} ({t('unknown')})</option>{/if}
        </select>
      </label>
    </div>
    {#if qa}
      <p class="qa">« {qa.prompt.length > 160 ? `${qa.prompt.slice(0, 160)}…` : qa.prompt} » · {qa.worktree ? t('worktree') : t('its clone')} · {qa.mode ?? 'default'} · <button class="lnk" onclick={() => openSettings('quick-actions')}>{t('change the quick action')}</button></p>
      <div class="repos">
        <span class="lbl">{t('Repositories')}<small>{t('One run each. None ticked: the quick action’s ({repos}).', { repos: qa.repos.join(', ') || t('none') })}</small></span>
        {#each repos as name (name)}
          <label class="chk"><input type="checkbox" checked={d.repos.includes(name)} onchange={(e) => (d.repos = e.currentTarget.checked ? [...d.repos, name] : d.repos.filter((x) => x !== name))} />{name}</label>
        {/each}
      </div>
    {:else if builtin}
      <p class="qa">{d.action === 'builtin:alerts-triage' ? t('Your open alerts in its prompt, as Alerts shows them: the agent sorts them (to handle, noise, already in progress) and proposes a first step for each.') : t('The Draft my points of the My sprint block: your sprint’s figures and Day trail in its prompt.')} {t('Without a repository: in your first working folder, in plan mode.')}</p>
    {/if}
    <div class="seg" role="radiogroup" aria-label={t('When')}>
      <button role="radio" aria-checked={d.pace === 'cron'} class:on={d.pace === 'cron'} onclick={() => (d.pace = 'cron')}>{t('Cron')}</button>
      <button role="radio" aria-checked={d.pace === 'every'} class:on={d.pace === 'every'} onclick={() => (d.pace = 'every')}>{t('Every…')}</button>
    </div>
    <div class="grid">
      {#if d.pace === 'cron'}
        <label>{t('Cron')}<small>{t('Minute hour day month weekday: 0 3 * * 1-5 is Monday to Friday at 03:00.')}</small><input class="mono" bind:value={d.cron} placeholder="0 3 * * 1-5" spellcheck="false" /></label>
        <label>{t('Time zone')}<small>{t('Empty: the computer’s.')}</small><input bind:value={d.time_zone} placeholder="Europe/Paris" spellcheck="false" /></label>
      {:else}
        <label>{t('Every')}<small>{t('30m, 6h, 1d…: from its last run.')}</small><input class="mono" bind:value={d.every} placeholder="6h" spellcheck="false" /></label>
      {/if}
    </div>
    <p class="words"><b>{words}</b>{#if d.pace === 'cron' && d.time_zone.trim()} ({d.time_zone.trim()}){/if}</p>
    {#if unchanged && savedView?.next.length}<p class="muted">{t('Next: {times}', { times: savedView.next.map(nextTime).join(' · ') })}</p>{:else if !unchanged}<p class="muted">{t('Its next times once saved.')}</p>{/if}
    <div class="grid">
      <label>{t('Result')}
        <select bind:value={d.deliver}><option value="review">{t('To review')}</option><option value="digest">{t('Digest')}</option></select>
      </label>
      <label>{t('Computer asleep')}
        <select bind:value={d.missed}><option value="once">{t('Catch up once on waking')}</option><option value="skip">{t('Skip')}</option></select>
      </label>
      <label>{t('Longest run')}<small>{t('Beyond it: interrupted, then stopped.')}</small><input type="number" min="1" bind:value={d.max_minutes} /></label>
    </div>
    <label class="chk"><input type="checkbox" bind:checked={d.enabled} />{t('On')}</label>
    <div class="btns">
      <button class="btn primary" onclick={save} disabled={saving || !d.name.trim() || taken}>{t('Save')}</button>
      {#if d.former}<button class="btn" disabled={!unchanged || !!savedView?.pending} onclick={() => runRoutine(d.former)} title={unchanged ? t('The routine as saved, now') : t('Save it first')}>{t('Run now')}</button>{/if}
      {#if d.former}<button class="btn danger" class:armed={armed === 'delete'} onclick={remove}>{armed === 'delete' ? t('Click again to remove') : t('Remove')}</button>{/if}
      <button class="btn ghost" onclick={() => (draft = null)}>{t('Close')}</button>
    </div>
  </div>
{/if}

<div class="card hist">
  <div class="hh"><b>{t('The last 24 hours')}</b> <span class="muted">{tn(history.length, '{n} run', '{n} runs')}</span></div>
  {#each history as r (r.id)}
    <div class="run {r.state}">
      <span class="ic">{STATE_ICON[r.state]}</span>
      <span class="t">{runTime(r.started ?? r.due)}</span>
      <b>{r.routine}</b>
      <span class="muted">{r.repo ?? t('no repository')}</span>
      <span class="st {r.state}">{STATE_LABEL[r.state]}</span>
      {#if r.caught_up}<span class="tag">{t('caught up on waking')}</span>{/if}
      {#if r.manual}<span class="tag">{t('run now')}</span>{/if}
      <span class="why" title={r.said ?? r.note ?? ''}>{r.note ?? r.said ?? ''}</span>
    </div>
  {:else}
    <div class="none">{t('No run in the last 24 hours.')}</div>
  {/each}
</div>

<style>
  .intro { margin: 0 0 10px; color: var(--text-muted); font-size: var(--fs-body); line-height: 1.5; }
  .banner { margin: 0 0 10px; padding: 8px 10px; border: 1px solid var(--warn); border-radius: var(--r8); background: var(--warn-bg); color: var(--text); font-size: var(--fs-body); }
  .ok { margin: 0 0 10px; color: var(--ok); font-size: var(--fs-meta); }
  .card { margin: 0 0 12px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--surface); font-size: var(--fs-body); }
  .muted { color: var(--text-muted); }
  .warn { color: var(--warn); }
  .ko { color: var(--ko); }
  .trust { border-color: var(--warn); }
  .th { display: flex; gap: 6px; align-items: baseline; }
  .diff { margin: 6px 0; padding: 6px 8px; border-radius: var(--r6); background: var(--surface-2, rgba(127, 127, 127, 0.08)); font-family: var(--mono); font-size: var(--fs-mono); white-space: pre-wrap; }
  .diff .add { color: var(--ok); }
  .diff .del { color: var(--ko); }
  .list .head, .list .line { display: grid; grid-template-columns: 28px minmax(110px, 1fr) minmax(110px, 1.2fr) minmax(150px, 1.6fr) 80px minmax(90px, 0.8fr) auto; gap: 8px; align-items: center; padding: 4px 0; }
  .list .head { color: var(--text-muted); font-size: var(--fs-tag); border-bottom: 1px solid var(--border); }
  .line.sel { background: color-mix(in srgb, var(--src-claude) 8%, transparent); }
  .name { padding: 0; border: none; background: none; color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .when { display: flex; flex-direction: column; gap: 1px; }
  .when code, .when small, .last small { color: var(--text-muted); font-size: var(--fs-tag); }
  .st { font-size: var(--fs-tag); padding: 0 6px; border-radius: var(--r9); border: 1px solid var(--border); color: var(--text-muted); }
  .st.ready { color: var(--ok); }
  .st.blocked { color: var(--warn); }
  .st.failed, .st.too_long { color: var(--ko); }
  .cfgerr { color: var(--ko); font-size: var(--fs-meta); padding: 0 0 4px 36px; }
  .none { color: var(--text-muted); padding: 6px 0; }
  .add { padding-top: 8px; }
  .editor { display: flex; flex-direction: column; gap: 8px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 8px 12px; }
  label { display: flex; flex-direction: column; gap: 3px; font-size: var(--fs-meta); color: var(--text-muted); }
  label small { font-size: var(--fs-tag); }
  input:not([type='checkbox']), select { padding: 4px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); }
  .mono { font-family: var(--mono); }
  label.chk { flex-direction: row; align-items: center; gap: 5px; color: var(--text); }
  .qa { margin: 0; color: var(--text-muted); font-size: var(--fs-meta); }
  .repos { display: flex; flex-wrap: wrap; gap: 4px 12px; align-items: center; }
  .lbl { display: flex; flex-direction: column; width: 100%; font-size: var(--fs-meta); color: var(--text-muted); }
  .seg { display: inline-flex; border: 1px solid var(--border); border-radius: var(--r6); overflow: hidden; align-self: flex-start; }
  .seg button { padding: 3px 12px; border: none; background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .seg button.on { background: color-mix(in srgb, var(--src-claude) 18%, transparent); color: var(--text); }
  .words { margin: 0; }
  .btns { display: flex; flex-wrap: wrap; gap: 6px; }
  .btn.armed { outline: 2px solid var(--warn); }
  .btn.danger { color: var(--ko); }
  .lnk { padding: 0; border: none; background: none; color: var(--info); font: inherit; cursor: pointer; text-decoration: underline; }
  .hh { display: flex; gap: 8px; align-items: baseline; padding-bottom: 4px; }
  .run { display: flex; gap: 8px; align-items: baseline; padding: 3px 0; border-top: 1px solid var(--border); font-size: var(--fs-meta); }
  .run.nothing, .run.skipped { opacity: 0.6; }
  .run .ic { width: 14px; text-align: center; }
  .run .t { font-family: var(--mono); color: var(--text-muted); }
  .run .tag { color: var(--info); font-size: var(--fs-tag); }
  .run .why { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-muted); }
</style>
