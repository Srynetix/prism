<script lang="ts">
  import Icon from '../Icon.svelte'
  import { t } from '../../i18n.svelte'
  import { profileForge } from '../../forge'
  import { fromPack, hasInherited, isSet, mergedRows, resetValue, setMergedRows, setValue, valueOf } from '../../settings.svelte'
  import { forgeOfRow, namedBy, repoBlockLabel, repoBlocks } from './repo-blocks'
  import type { RepoBlock } from '../../bindings/RepoBlock'

  /**
   * Settings › Repositories, below the list: each block that reads repositories and the one (or
   * those) it reads, chosen in its key (`[renovate] repo`, `[vulnerabilities] repos`, a pack's
   * `[shipping] repo`); none chosen: the repository bare MR references name (`[forge]
   * bare_mr_repo`), itself chosen here. A pack's choice shows as the pack's until changed.
   */
  type Row = Record<string, unknown>
  const REPOS = ['repos']
  const PRIMARY = ['forge', 'bare_mr_repo']
  const rows = $derived(mergedRows(REPOS))
  const str = (v: unknown) => (typeof v === 'string' ? v : '')
  /** On the profile's forge (their own, or their working folder's): the only ones bare references and the blocks can read. */
  const here = (r: Row) => forgeOfRow(r) === profileForge()
  const names = $derived(rows.filter((r) => str(r.name) && here(r)).map((r) => str(r.name)))
  const flagged = $derived(str(rows.find((r) => r.primary)?.name))
  const bare = $derived(valueOf<string>(PRIMARY, '') || flagged)

  const HINTS: Record<string, string> = {
    ci_health: t('Failure rate and durations of its pipelines.'),
    renovate: t('Its Dependency Dashboard, and what runs Renovate.'),
    vulnerabilities: t('Trivy on the dependencies of each one (base images: a switch of each repository).'),
  }
  const blocks = $derived(repoBlocks())
  const many = (b: RepoBlock) => b.uses === 'many'
  const pick = (b: RepoBlock, name: string) => setValue(b.key, name ? (many(b) ? [name] : name) : null)
  function add(b: RepoBlock, name: string) {
    if (name && !namedBy(b).includes(name)) setValue(b.key, [...namedBy(b), name])
  }
  function drop(b: RepoBlock, name: string) {
    const next = namedBy(b).filter((n) => n !== name)
    setValue(b.key, next.length ? next : null)
  }

  /** `[forge] bare_mr_repo`; the former `primary = true` of config.toml's own entry goes with it. */
  function setBare(name: string) {
    setValue(PRIMARY, name || null)
    const own = rows.find((r) => r.primary && !fromPack(REPOS, r))
    if (own) setMergedRows(REPOS, rows.map((r) => (r === own ? Object.fromEntries(Object.entries(r).filter(([k]) => k !== 'primary')) : r)))
  }
  /** Everything on one repository: each block, and bare references. */
  function allOn(name: string) {
    if (!name) return
    for (const b of blocks) pick(b, name)
    setBare(name)
  }
  const fromPackKey = (key: string[]) => hasInherited(key) && !isSet(key)
</script>

<div class="blocks">
  <p class="intro">{t('Each reads the repository chosen. None chosen: the one bare MR references name, if any; otherwise the block stays empty and says so.')}</p>
  <div class="row">
    <span class="what"><b><Icon name="star" /> {t('Bare MR references')}</b><small>{t('!1294 without a repository before it, in notes, the palette, alerts; that repository’s MRs listed without its name, always read. None: every MR is named shop!1294.')}</small></span>
    <select value={valueOf<string>(PRIMARY, '')} onchange={(e) => setBare(e.currentTarget.value)} class:pack={fromPackKey(PRIMARY)}>
      <option value="">{flagged ? t('{name} · primary = true', { name: flagged }) : t('None')}</option>
      {#each names as n, i (i)}<option value={n}>{n}</option>{/each}
    </select>
  </div>
  {#each blocks as b (b.id)}
    {@const named = namedBy(b)}
    <div class="row">
      <span class="what"><b>{repoBlockLabel(b.id)}{#if !HINTS[b.id]}<span class="tag">pack</span>{/if}</b><small>{HINTS[b.id] ?? t('A block of a pack that reads {what}.', { what: many(b) ? t('repositories') : t('a repository') })} <code>[{b.key[0]}] {b.key[1]}</code></small></span>
      {#if many(b)}
        <span class="multi">
          {#each named as n, i (i)}<span class="chip">{n}<button onclick={() => drop(b, n)} aria-label={t('Remove {name}', { name: n })}><Icon name="close" /></button></span>{/each}
          <select value="" onchange={(e) => { add(b, e.currentTarget.value); e.currentTarget.value = '' }} class:pack={fromPackKey(b.key)}>
            <option value="">{named.length ? t('＋ add') : bare ? t('{name} · default (bare references)', { name: bare }) : t('None')}</option>
            {#each names.filter((n) => !named.includes(n)) as n, i (i)}<option value={n}>{n}</option>{/each}
          </select>
        </span>
      {:else}
        <select value={named[0] ?? ''} onchange={(e) => pick(b, e.currentTarget.value)} class:pack={fromPackKey(b.key)}>
          <option value="">{bare ? t('{name} · default (bare references)', { name: bare }) : t('None')}</option>
          {#each names as n, i (i)}<option value={n}>{n}</option>{/each}
        </select>
      {/if}
      {#if isSet(b.key)}<button class="reset" onclick={() => resetValue(b.key)} title={hasInherited(b.key) ? t('Back to the pack’s value') : t('Back to bare references’ repository')}><Icon name="undo" /></button>{/if}
    </div>
  {/each}
  <div class="all">
    <span>{t('Everything on one repository (a monorepo):')}</span>
    <select value="" onchange={(e) => { allOn(e.currentTarget.value); e.currentTarget.value = '' }}>
      <option value="">{t('Choose…')}</option>
      {#each names as n, i (i)}<option value={n}>{n}</option>{/each}
    </select>
  </div>
</div>

<style>
  .blocks { margin-bottom: 14px; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .intro { margin: 0 0 6px; font-size: var(--fs-meta); color: var(--text-muted); }
  .row { display: flex; align-items: center; gap: 12px; padding: 7px 0; border-top: 1px dashed var(--border); }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .what b { display: flex; align-items: center; gap: 6px; font-weight: 550; }
  .what small { font-size: var(--fs-meta); color: var(--text-muted); }
  .what code { font: var(--fs-tag) var(--mono); opacity: 0.8; }
  .tag { padding: 0 6px; border: 1px solid var(--info); border-radius: var(--r9); color: var(--info); font-size: var(--fs-tag); font-weight: 500; }
  select { max-width: 220px; padding: 2px 6px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-meta); }
  select.pack { border-color: var(--info); }
  .multi { display: inline-flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 4px; max-width: 50%; }
  .chip { display: inline-flex; align-items: center; gap: 3px; padding: 0 4px 0 8px; border: 1px solid var(--border); border-radius: var(--r9); font-size: var(--fs-tag); line-height: 18px; }
  .chip button { display: grid; place-items: center; padding: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .reset { padding: 0; border: none; background: none; color: var(--info); cursor: pointer; }
  .all { display: flex; justify-content: flex-end; align-items: center; gap: 8px; padding-top: 8px; border-top: 1px solid var(--border); font-size: var(--fs-meta); color: var(--text-muted); }
</style>
