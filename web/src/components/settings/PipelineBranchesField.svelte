<script lang="ts">
  import { hasCore, invoke } from '../../bridge'
  import { t } from '../../i18n.svelte'
  import ListInput from './ListInput.svelte'
  import type { PipelineBranches } from '../../bindings/PipelineBranches'

  /**
   * A repository's followed branches (`[[repos]] pipeline_branches`): chips in their order, `@main`
   * and `@release` offered, and below, what the forge resolves them to, a name or a pattern it has
   * no branch for flagged. Empty: `[forge] pipeline_branches`, else the main and release ones.
   */
  let { repo, project, mainBranch, releasePrefix, value, fallback, onchange }: {
    repo: string
    project: string
    /** As the card shows them, saved or not: empty, the project's default branch / no releases. */
    mainBranch: string
    releasePrefix: string
    value: string[] | null
    fallback: string[]
    onchange: (v: string[] | null) => void
  } = $props()

  const TOKENS = ['@main', '@release']
  const shown = $derived(value ?? [])
  const asked = $derived(value ?? fallback)

  let preview = $state<PipelineBranches | null>(null)
  let error = $state('')
  let loading = $state(false)
  // Read again when what it depends on changes (not when the rows are rebuilt the same), the last answer only.
  const key = $derived(JSON.stringify([repo, project, mainBranch, releasePrefix, asked]))
  let seq = 0
  $effect(() => {
    const args = JSON.parse(key) as [string, string, string, string, string[]]
    if (!hasCore() || !args[0] || !args[1]) return
    const [repo, project, mainBranch, releasePrefix, branches] = args
    const mine = ++seq
    loading = true
    invoke<PipelineBranches>('pipeline_branches', { repo, project, mainBranch, releasePrefix, branches })
      .then((p) => {
        if (mine === seq) [preview, error] = [p, '']
      })
      .catch((e) => {
        if (mine === seq) [preview, error] = [null, String(e)]
      })
      .finally(() => {
        if (mine === seq) loading = false
      })
  })
</script>

<div class="field">
  <span class="lbl">{t('Branches followed')}</span>
  <ListInput value={shown} onchange={(v) => onchange(v.length ? v : null)} options={TOKENS} placeholder={fallback.length ? fallback.join(', ') + ' · ' + t('from Forge') : t('@main, @release: the main and release branches')} compact />
  <small class="hint">{t('Their pipelines in the branch banner, CI alerts and the header. A name, a pattern (env/*), @main (the main branch), @release (the release prefix’s).')}</small>
  {#if hasCore()}
    <p class="resolved" aria-live="polite">
      {#if error}<span class="err">{error}</span>
      {:else if preview}
        <span class="what">{t('On the forge:')}</span>
        {#each preview.refs as b (b)}<span class="br mono">{b}</span>{:else}<span class="none">{t('no branch')}</span>{/each}
        {#each preview.missing as m (m)}<span class="br mono miss" title={t('The forge has no branch for it')}>{m} ✗</span>{/each}
      {:else if loading}<span class="none">{t('Reading the forge…')}</span>{/if}
    </p>
  {/if}
</div>

<style>
  .field { display: flex; flex-direction: column; gap: 3px; margin-top: 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .hint { font-size: var(--fs-tag); }
  .resolved { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin: 2px 0 0; }
  .what { margin-right: 2px; }
  .br { padding: 0 6px; border-radius: var(--r9); background: var(--surface-muted); color: var(--text); font-size: var(--fs-tag); line-height: 17px; }
  .br.miss { background: var(--warn-bg); color: var(--warn); }
  .none { font-style: italic; }
  .err { color: var(--ko); }
</style>
