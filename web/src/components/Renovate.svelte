<script lang="ts">
  import RepoChip from './RepoChip.svelte'
  import { t, tn } from '../i18n.svelte'
  import { armer } from '../armed.svelte'
  import { clock } from '../clock.svelte'
  import { locale } from '../locale'
  import { forgeLogo, mrNumber, onGithub } from '../forge'
  import { hasCore } from '../bridge'
  import BlockControls from './BlockControls.svelte'
  import { isCollapsed, isShort } from '../layout.svelte'
  import Icon from './Icon.svelte'
  import Logo from './Logo.svelte'
  import type { MergeRequest, Renovate } from '../types'
  import { checkRenovate, playRenovate } from '../pipeline-actions'
  import type { RenovateItem } from '../types'
  import { ago, span } from '../time'
  import { setQuery } from '../filter.svelte'

  /**
   * Renovate: the Dependency Dashboard items by section (awaiting approval,
   * open MRs…), and launching its pipeline schedule with the state of its last pipeline.
   */
  let { renovate, merge_requests, error }: { renovate: Renovate | null; merge_requests: MergeRequest[]; error?: string } = $props()

  const ACTIVE = new Set(['created', 'pending', 'preparing', 'waiting_for_resource', 'running'])
  const run = $derived(renovate?.run ?? null)
  const running = $derived(!!run && ACTIVE.has(run.status))
  const tone = (s: string) => (s === 'success' ? 'ok' : s === 'failed' ? 'ko' : ACTIVE.has(s) ? 'run' : '')
  const count = $derived(renovate?.sections.reduce((n, s) => n + s.items.filter((i) => i.branch).length, 0) ?? 0)
  /** An update's merge request: known to Prism, or next to the dashboard issue (`/-/issues/N` on GitLab, `/issues/N` on GitHub). */
  const mrUrl = (iid: number) => merge_requests.find((m) => !m.repo && m.iid === iid)?.url ?? (renovate?.dashboard_url?.replace(/\/-\/issues\/\d+$/, `/-/merge_requests/${iid}`).replace(/\/issues\/\d+$/, `/pull/${iid}`) ?? '#')

  const now = $derived(running ? clock(1000).now : Date.now())

  /**
   * An item's checkbox: tick it in the dashboard (two clicks), so that Renovate runs it
   * on the next pass. Not possible during a pass: Renovate rewrites the issue at the end of the run.
   */
  const marker = (i: RenovateItem) => `${i.action}=${i.branch}`
  /** What checking an item asks of Renovate, by its action. */
  const ACTION: Record<string, string> = {
    'approve-branch': t('Check it in the dashboard: Renovate will create this MR on its next run'),
    'rebase-branch': t('Check it in the dashboard: Renovate will rebase or recreate this MR on its next run'),
    'unlimit-branch': t('Check it in the dashboard: Renovate will create this MR despite the rate limit on its next run'),
    'unschedule-branch': t('Check it in the dashboard: Renovate will create this MR now on its next run'),
    'retry-branch': t('Check it in the dashboard: Renovate will retry this branch on its next run'),
  }
  /** Two clicks, a box's or the launch's: one armed at a time. */
  const arm = armer()
  let checking = $state<string | null>(null)
  let checkError = $state<{ marker: string; message: string } | null>(null)
  async function check(i: RenovateItem) {
    const m = marker(i)
    if (checking || i.checked) return
    if (!arm.confirm(m)) {
      checkError = null
      return
    }
    checking = m
    try {
      await checkRenovate(m)
    } catch (err) {
      checkError = { marker: m, message: String(err) }
    } finally {
      checking = null
    }
  }

  /** Two-click launch, like the other actions that trigger something on the forge. */
  let busy = $state(false)
  let failure = $state<string | null>(null)
  async function launch() {
    if (busy) return
    if (!arm.confirm('launch')) {
      failure = null
      return
    }
    busy = true
    try {
      await playRenovate()
    } catch (err) {
      failure = String(err)
    } finally {
      busy = false
    }
  }
</script>

<section class="panel" style="--src: var(--src-gitlab)" class:collapsed={isCollapsed('renovate')}>
  <h2>
    <span class="title"><Logo name={forgeLogo()} />Renovate<RepoChip id="renovate" /></span>
    <span class="head">
      <small>{tn(count, '{n} update', '{n} updates')}</small>
      {#if renovate?.dashboard_url}<a href={renovate.dashboard_url} target="_blank" rel="noreferrer">{t('Dashboard')} <Icon name="external" /></a>{/if}
    </span>
    <BlockControls id="renovate" />
  </h2>
  {#if !renovate}
    <div class="empty">{error ? t('Renovate unavailable: {error}', { error }) : t('Loading Renovate…')}</div>
  {:else}
    <!-- Manual launch of the Renovate schedule and the state of its last pipeline. -->
    <div class="runbar">
      <!-- GitHub: Renovate runs as a workflow, or as the Renovate app (its dashboard's "run again" box). -->
      <span class="lbl">{onGithub() ? (renovate.schedule_id ? t('Workflow') : t('App')) : t('Schedule')}</span>
      {#if run}
        <a class="st {tone(run.status)}" href={run.url} target="_blank" rel="noreferrer" title={t('{what} #{id} · started {date}', { what: onGithub() ? t('Run') : t('Pipeline'), id: run.id, date: new Date(run.started_at).toLocaleString(locale()) })}>
          {#if running}<span class="dot"></span>{/if}{run.status}
        </a>
        <span class="mono muted">{running ? span(now - new Date(run.started_at).getTime()) : ago(run.updated_at)}</span>
      {:else}
        <span class="muted">{t('never run')}</span>
      {/if}
      {#if renovate.schedule_ref}<span class="mono muted">{t('on {ref}', { ref: renovate.schedule_ref })}</span>{/if}
      <span class="grow"></span>
      {#if hasCore() && (renovate.schedule_id || (onGithub() && renovate.dashboard_iid))}
        <button
          class="launch"
          class:armed={arm.is('launch')}
          class:error={!!failure}
          disabled={busy || running}
          onclick={launch}
          title={failure ? t('Could not start Renovate: {error}', { error: failure }) : running ? t('Renovate is running') : arm.is('launch') ? t('Click again to run Renovate now') : onGithub() ? (renovate.schedule_id ? t('Run the Renovate workflow now') : t('Ask the Renovate app for a pass (its dashboard’s run again box)')) : t('Run the Renovate schedule now')}
        ><Icon name="play" /> {busy ? t('starting…') : running ? t('running') : arm.is('launch') ? t('run now?') : t('Run now')}</button>
      {/if}
    </div>
    {#if isShort('renovate')}
      <!-- Short: the launch bar and a counter per section. -->
      <div class="short-body">
        <div class="srow counts">
          {#each renovate.sections as s (s.title)}<span title={s.title}>{s.title.replace(/^(\S+\s+\S+).*$/, '$1')} <b>{s.items.length}</b></span>{:else}<span class="more">{t('Nothing pending.')}</span>{/each}
        </div>
      </div>
    {:else}
    <div class="scroll">
      {#each renovate.sections as s (s.title)}
        <div class="section">{s.title} · {s.items.length}</div>
        {#each s.items as i (i.action + (i.branch ?? i.title))}
          {@const m = marker(i)}
          <div class="row">
            {#if hasCore()}
              <button
                class="box"
                class:on={i.checked}
                class:armed={arm.is(m)}
                class:error={checkError?.marker === m}
                disabled={i.checked || checking === m || running}
                onclick={() => check(i)}
                aria-label={i.checked ? t('Requested') : arm.is(m) ? t('Confirm: check this item') : t('Check this item in the dashboard')}
                title={checkError?.marker === m
                  ? t('Could not check: {error}', { error: checkError.message })
                  : i.checked
                    ? t('Checked: Renovate will act on its next run')
                    : running
                      ? t('Renovate is running: wait for it to finish')
                      : arm.is(m)
                        ? t('Click again to check it in the dashboard')
                        : (ACTION[i.action] ?? t('Check it in the dashboard: Renovate will act on it on its next run'))}
              >{#if i.checked}<Icon name="check" stroke={3} />{:else}{checking === m ? '…' : arm.is(m) ? '?' : ''}{/if}</button>
            {/if}
            {#if i.mr}
              <button class="mono link" onclick={() => setQuery(mrNumber(i.mr!))} title={t('Filter on {mr}', { mr: mrNumber(i.mr!) })}>{mrNumber(i.mr!)}</button>
              <a class="open" href={mrUrl(i.mr)} target="_blank" rel="noreferrer" aria-label={t('Open {id}', { id: mrNumber(i.mr!) })}><Icon name="external" /></a>
            {/if}
            <span class="grow t" title="{i.title}{i.branch ? `\n${i.branch}` : ''}">{i.title}</span>
            {#if i.packages.length}
              <span class="pkgs" title={i.packages.join(', ')}>{i.packages.slice(0, 3).join(', ')}{i.packages.length > 3 ? ` +${i.packages.length - 3}` : ''}</span>
            {/if}
            {#if i.checked && !hasCore()}<span class="pill info" title={t('Requested from the dashboard, waiting for Renovate')}>{t('requested')}</span>{/if}
          </div>
        {/each}
      {:else}
        <div class="empty">{t('The dependency dashboard has nothing pending.')}</div>
      {/each}
    </div>
    {/if}
  {/if}
</section>

<style>
  .head { display: flex; gap: 10px; align-items: baseline; }
  .head a { font-size: var(--fs-meta); font-weight: 400; }
  .runbar { display: flex; align-items: center; gap: 8px; padding: 6px 12px; border-bottom: 1px solid var(--border); background: var(--surface-muted); font-size: var(--fs-meta); }
  .runbar .lbl { font-weight: 500; color: var(--text); }
  .runbar .grow { flex: 1; }
  .st { display: inline-flex; align-items: center; gap: 5px; font-weight: 500; color: var(--text-muted); }
  .st.ok { color: var(--ok); } .st.ko { color: var(--ko); } .st.run { color: var(--warn); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--warn); animation: blink 1.2s infinite; }
  @keyframes blink { 50% { opacity: 0.35; } }
  .launch { padding: 1px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .launch:hover:not(:disabled) { color: var(--text); border-color: var(--accent); }
  .launch.armed { color: var(--accent); border-color: var(--accent); background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .launch.error { color: var(--ko); border-color: var(--ko); }
  .launch:disabled { cursor: default; opacity: 0.7; }
  .section { padding: 4px 12px; background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); border-top: 1px solid var(--border); }
  .section:first-child { border-top: none; }
  .box { display: inline-flex; align-items: center; justify-content: center; flex-shrink: 0; width: 15px; height: 15px; padding: 0; border: 1.5px solid var(--text-muted); border-radius: var(--r3); background: none; color: var(--surface); font: inherit; font-size: 10px; font-weight: 700; line-height: 1; cursor: pointer; }
  .box:hover:not(:disabled) { border-color: var(--accent); }
  .box.armed { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 25%, transparent); color: var(--accent); }
  .box.on { border-color: var(--ok); background: var(--ok); }
  .box.error { border-color: var(--ko); }
  .box:disabled { cursor: default; }
  .box:disabled:not(.on) { opacity: 0.4; }
  .t { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pkgs { flex-shrink: 1; min-width: 0; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--mono); font-size: var(--fs-tag); color: var(--text-muted); }
  .link { border: none; background: none; padding: 0; color: var(--info); cursor: pointer; }
  .open { font-size: var(--fs-tag); opacity: 0.5; margin-left: -4px; }
  .counts { gap: 14px; flex-wrap: wrap; color: var(--text-muted); }
  .counts b { color: var(--text); font-variant-numeric: tabular-nums; }
</style>
