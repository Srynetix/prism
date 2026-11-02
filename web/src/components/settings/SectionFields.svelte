<script lang="ts">
  import Control from './Control.svelte'
  import Field from './Field.svelte'
  import TableInput from './TableInput.svelte'
  import CheckBadge from './CheckBadge.svelte'
  import GoogleSignIn from './GoogleSignIn.svelte'
  import LogsPanel from './LogsPanel.svelte'
  import MailSignIn from './MailSignIn.svelte'
  import TokenField from './TokenField.svelte'
  import ForgeLogin from './ForgeLogin.svelte'
  import { TOKEN_HINTS, TOKEN_PAGES, tokenCheck } from '../../credentials'
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { requestOf } from '../../settings-checks.svelte'
  import { live } from '../../live.svelte'
  import { fromPack, hasInherited, isChanged, isSet, keyedBy, mergedRows, packLabel, resetValue, setMergedRows, setValue, valueOf } from '../../settings.svelte'
  import RepoAdd from './RepoAdd.svelte'
  import RepoClones from './RepoClones.svelte'
  import RepoCards from './RepoCards.svelte'
  import RepoBlocks from './RepoBlocks.svelte'
  import type { FieldSpec, SectionSpec } from '../../settings-schema'
  import { openWizard } from '../../wizard.svelte'

  /**
   * A section of settings-schema.ts: one row per key, one table per `[[table]]`. `hide`: keys
   * shown elsewhere (the feature switch, in the page header).
   */
  let { spec, hide = [] }: { spec: SectionSpec; hide?: string[] } = $props()
  const pathOf = (key: string) => [...spec.path, key]
  /** The section's last read by its collector: successful, in error, or not yet. */
  const status = $derived.by(() => {
    const s = live.snapshot
    if (!spec.source || !s) return null
    if (spec.source === 'clickup' && s.tracker !== 'clickup') return { ok: null, text: t('Off: ClickUp is not read.') }
    if (spec.source === 'obsidian' && s.tracker !== 'obsidian') return { ok: null, text: t('Off: the tracker is not Obsidian (Tracker › Provider).') }
    if (spec.source === 'jira' && s.tracker !== 'jira') return { ok: null, text: t('Off: the tracker is not Jira (Tracker › Provider).') }
    // Its slow queues (ClickUp's, Jira's) fail on their own.
    const err = s.errors[spec.source] ?? s.errors[`${spec.source} queues`]
    if (err) return { ok: false, text: err }
    return s.answered?.includes(spec.source) ? { ok: true, text: t('Read correctly with the current config.') } : { ok: null, text: t('Not read yet.') }
  })
</script>

{#if spec.intro}<p class="intro">{spec.intro}</p>{/if}
{#if status}<p class="status" class:ok={status.ok === true} class:ko={status.ok === false} title={status.text}><Icon name={status.ok === false ? 'warning' : status.ok ? 'check' : 'pending'} /><span>{status.ok === false ? t('Last read failed: {error}', { error: status.text }) : status.text}</span></p>{/if}
<!-- Repositories: their cards and the Add panel; below, the blocks that read them. -->
{#if spec.id === 'repos'}
  <RepoCards />
  <div id="repo-add" class="repo-add"><RepoAdd /></div>
  <RepoClones />
{:else if spec.id === 'repo-blocks'}
  <RepoBlocks />
<!-- A section of text alone (macOS Calendar) has no card. -->
{:else if spec.fields.length || spec.signin || spec.token || spec.login}
<div class="card">
  {#if spec.signin === 'mail'}<MailSignIn />{/if}
  {#if spec.token && spec.token !== 'google'}<TokenField tracker={spec.token} hint={TOKEN_HINTS[spec.token]} create={TOKEN_PAGES[spec.token]} check={tokenCheck(spec.token, valueOf<string>(['jira', 'site'], ''), valueOf<string>(['jira', 'email'], ''))} />{/if}
  <!-- The forge's login is its CLI's: the command, and whom it is signed in as. -->
  {#if spec.login}
    <Field label={t('Login')} hint={spec.login === 'github' ? t('gh’s own login, in the system keychain: Prism calls gh and never sees the token.') : t('glab’s own login, in the system keychain: Prism calls glab and never sees the token.')}><ForgeLogin forge={spec.login} host={spec.login === 'gitlab' ? valueOf<string>(['gitlab', 'host'], '') : ''} /></Field>
  {/if}
  {#each spec.fields.filter((f) => !hide.includes(f.key)) as f (f.key)}
    {@const path = pathOf(f.key)}
    {#if f.kind === 'table'}
      <div class="tfield" class:changed={isChanged(path)}>
        <div class="lbl">
          <b>{f.label}</b>
          {#if f.hint}<small>{f.hint}</small>{/if}
          <span class="key">{spec.path.length ? `[[${[...spec.path, f.key].join('.')}]]` : `[[${f.key}]]`}</span>
          <!-- A list that has a default (Audio) reverts to it; [[repos]] and the like have none. A pack's list comes back from the pack. -->
          {#if hasInherited(path)}
            {#if isSet(path)}<button class="reset" onclick={() => resetValue(path)} title={t('Remove from config.toml: the pack’s list applies')}>{t('Back to pack value')}</button>
            {:else}<span class="badge pack">{t('from {pack}', { pack: packLabel() })}</span>{/if}
          {:else if (spec.path.length || f.seed) && isSet(path)}<button class="reset" onclick={() => resetValue(path)} title={f.seed === 'views' ? t('Remove from config.toml: one empty Dashboard') : t('Remove from config.toml: the built-in list applies')}>{t('Reset to default')}</button>{/if}
        </div>
        <!-- Emptying a pack's list writes an empty one, which overrides it. -->
        {#if keyedBy(path)}
          <!-- [[repos]]: the packs' entries and the file's, merged by name; the file keeps what differs. -->
          <TableInput rows={mergedRows(path)} columns={f.columns ?? []} blank={f.blank ?? {}} onchange={(rows) => setMergedRows(path, rows)} fromPack={(r) => fromPack(path, r)} />
        {:else}
          <TableInput rows={valueOf<Record<string, unknown>[]>(path) ?? []} columns={f.columns ?? []} blank={f.blank ?? {}} onchange={(rows) => setValue(path, rows.length || hasInherited(path) ? rows : null)} />
        {/if}
        <span class="seeds">
          {#if f.seed === 'views'}<button class="seed" onclick={openWizard} title={t('The packs’ templates (or Prism’s starter pack): preview the pages and blocks before creating them')}>{t('Start from a template…')}</button>{/if}
        </span>
      </div>
    {:else}
      <Field label={f.label} hint={f.hint} {path}>
        <Control kind={f.kind} value={valueOf(path)} onchange={(v) => setValue(path, v)} options={f.options} labels={f.labels} optional={f.optional} placeholder={f.placeholder} unit={f.unit} step={f.step} />
        {#if f.check}<CheckBadge req={requestOf(f.check, valueOf(path), (k) => valueOf(pathOf(k)))} />{/if}
      </Field>
    {/if}
  {/each}
  <!-- Google: the client's id above, then its secret, then signing in. -->
  {#if spec.token === 'google'}<TokenField tracker="google" label={t('Client secret')} hint={t('The “Client secret” of the same OAuth client. Kept in the system keychain, this profile’s entry.')} />{/if}
  {#if spec.signin === 'google'}<GoogleSignIn />{/if}
  {#if spec.id === 'logs'}<LogsPanel />{/if}
</div>
{/if}

<style>
  .status { display: flex; align-items: center; gap: 6px; margin: 0 0 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .status span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status.ok { color: var(--ok); }
  .status.ko { color: var(--ko); }
  .repo-add { margin: 14px 0 18px; }
  .intro { margin: 0 0 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .tfield { display: flex; flex-direction: column; gap: 8px; padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .tfield:last-child { border-bottom: none; }
  .tfield.changed { background: color-mix(in srgb, var(--warn) 6%, transparent); box-shadow: inset 3px 0 0 var(--warn); }
  .lbl { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 10px; }
  .lbl b { display: flex; align-items: center; gap: 6px; font-weight: 550; font-size: var(--fs-body); }
  .lbl small { color: var(--text-muted); font-size: var(--fs-meta); }
  .key { font: var(--fs-tag) var(--mono); color: var(--text-muted); opacity: 0.75; }
  .badge { padding: 0 6px; border-radius: var(--r9); font-size: 10px; font-weight: 500; line-height: 16px; }
  .seeds { display: flex; gap: 6px; }
  .seed { align-self: flex-start; margin-top: 4px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .seed:hover { border-color: var(--accent); }
  .badge.pack { background: var(--info-bg); color: var(--info); }
  .reset { padding: 0; border: none; background: none; color: var(--info); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
</style>
