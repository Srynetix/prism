<script lang="ts" module>
  /** A new profile: its name, its color, and what it starts from. */
  export interface NewProfile {
    name: string
    color: string
    start: 'empty' | 'pack' | 'demo'
    pack: string
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte'
  import Icon from './Icon.svelte'
  import { t } from '../i18n.svelte'
  import { PROFILE_COLORS, profileColor, slugOf } from '../profiles.svelte'
  import { canPick, pickPath } from '../pick'

  /**
   * What a new profile starts from: its name, its color, and empty (Prism's starter pack), a team's
   * pack or the demo. Settings › General › Profiles creates one with it, the first launch sets up
   * the active one. `taken`: the id is another profile's (`hint` says where it goes otherwise).
   */
  let {
    value = $bindable(),
    taken = () => false,
    hint,
    autofocus = false,
    extra,
  }: {
    value: NewProfile
    taken?: (id: string) => boolean
    /** Under the name: where the profile goes (`profiles/<id>/`). */
    hint?: (id: string) => string
    autofocus?: boolean
    /** More rows of the form (Settings: "Switch to it now"). */
    extra?: Snippet
  } = $props()

  const id = $derived(slugOf(value.name))

  async function choosePack() {
    const p = await pickPath(/^(glab|gh):/.test(value.pack) ? '' : value.pack, { file: true, extensions: ['toml'] })
    if (p) value.pack = p
  }
</script>

<div class="f">
  <span>{t('Name')}</span>
  <span class="row">
    <!-- svelte-ignore a11y_autofocus -->
    <input class="t" bind:value={value.name} placeholder={t('Work')} {autofocus} spellcheck="false" aria-label={t('Profile name')} />
    {#if id && (taken(id) || hint)}<small class:warn={taken(id)}>{taken(id) ? t('{id} already exists', { id }) : hint!(id)}</small>{/if}
  </span>
  <span>{t('Color')}</span>
  <span class="colors">{#each PROFILE_COLORS as c (c)}<button class="sw" class:on={value.color === c} style:--c={profileColor(c)} onclick={() => (value.color = c)} aria-label={t('Color {name}', { name: c })}></button>{/each}</span>
  <span>{t('Start from')}</span>
  <div class="starts">
    <button class="start" class:on={value.start === 'empty'} onclick={() => (value.start = 'empty')}><b>{t('Empty')}</b><small>{t('Every source off, with Prism’s starter pack: the pages wizard then offers its Developer template.')}</small></button>
    <button class="start" class:on={value.start === 'pack'} onclick={() => (value.start = 'pack')}><b>{t('A pack')}</b><small>{t('A team’s configuration: a file, or a file of a GitLab or GitHub repository.')}</small></button>
    <button class="start" class:on={value.start === 'demo'} onclick={() => { value.start = 'demo'; if (!value.name.trim()) value.name = t('Demo') }}><b>{t('The demo')}</b><small>{t('A fictitious team’s work in every block: nothing read, to try Prism or show it.')}</small></button>
  </div>
  {#if value.start === 'pack'}
    <span></span>
    <span class="row">
      <input class="t mono grow" bind:value={value.pack} placeholder={t('~/team/pack.toml, glab:group/project:path/pack.toml@main or gh:owner/repo:path/pack.toml')} spellcheck="false" aria-label={t('Pack source')} />
      {#if canPick()}<button class="btn" onclick={choosePack} title={t('Choose a pack file')}><Icon name="folder" />{t('Choose…')}</button>{/if}
    </span>
  {/if}
  {@render extra?.()}
</div>

<style>
  .f { display: grid; grid-template-columns: 110px minmax(0, 1fr); gap: 10px 14px; align-items: center; }
  .f > :global(span:not(.row):not(.colors)) { color: var(--text-muted); font-size: var(--fs-meta); }
  .f small { color: var(--text-muted); font-size: var(--fs-meta); }
  .f small.warn { color: var(--warn); }
  .row { display: flex; align-items: center; gap: 6px; min-width: 0; }
  .grow { flex: 1; }
  .t { height: 28px; min-width: 0; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); outline: none; }
  .t:focus { border-color: var(--accent); }
  .mono { font-family: var(--mono); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover { border-color: var(--accent); }
  .colors { display: inline-flex; flex-wrap: wrap; gap: 6px; }
  .sw { width: 18px; height: 18px; padding: 0; border: none; border-radius: var(--r5); background: var(--c); cursor: pointer; }
  .sw.on { outline: 2px solid var(--text); outline-offset: 2px; }
  .starts { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .start { display: flex; flex-direction: column; gap: 2px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--r8); background: var(--bg); color: var(--text); font: inherit; text-align: left; cursor: pointer; }
  .start small { color: var(--text-muted); font-size: var(--fs-meta); line-height: 1.35; }
  .start.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, var(--bg)); }
  .start.on b { color: var(--accent); }
</style>
