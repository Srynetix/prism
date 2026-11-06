<script lang="ts">
  import { tildePath } from '../platform'
  import { armer } from '../armed.svelte'
  import { onMount } from 'svelte'
  import ArmedLabel from './ArmedLabel.svelte'
  import ProfileForm, { type NewProfile } from './ProfileForm.svelte'
  import {
    PROFILE_COLORS, profileColorName, createProfile, currentProfileId, initial, loadProfiles, openProfile, profileColor, profiles, profilesView, removeProfile, renameProfile, slugOf, stopProfile, type ProfileInfo,
  } from '../profiles.svelte'
  import { toast } from '../toast.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * Settings › General › Profiles: every profile with its folder, packs and running agents, and its
   * state here: in a window of the app, in the background (its Prism runs, no window shown), or
   * closed. Show or Open its window; Stop its Prism (two clicks, its agents go on in tmux); rename
   * (and recolor); remove (two clicks: neither the default, nor the app's own, nor one running or
   * with running agents). A new profile starts empty (with Prism's starter pack, whose Developer
   * template the pages wizard offers), or from a team's pack.
   */
  onMount(() => void loadProfiles().catch((e) => (error = String(e))))


  let error = $state('')
  let busy = $state('')
  async function run(key: string, f: () => Promise<unknown>) {
    busy = key
    error = ''
    try {
      await f()
    } catch (e) {
      error = String(e)
    } finally {
      busy = ''
    }
  }

  /** Renaming: one profile at a time, its name and color edited in place. */
  let editing = $state<{ id: string; name: string; color: string } | null>(null)
  const edit = (p: ProfileInfo) => (editing = { id: p.id, name: p.name, color: profileColorName(p.color) })
  const saveEdit = () => editing && run(`rename:${editing.id}`, async () => {
    await renameProfile(editing!.id, editing!.name, editing!.color)
    editing = null
  })

  /** Remove arms on the first click (no confirm(): the webview always answers no). */
  const armed = armer(3500)
  function askRemove(p: ProfileInfo) {
    if (!armed.confirm(p.id)) return
    void run(`remove:${p.id}`, async () => {
      await removeProfile(p.id)
      toast(t('Profile {name} removed', { name: p.name }))
    })
  }
  const removable = (p: ProfileInfo) => p.id !== 'default' && !p.active && !p.open && !p.agents
  const whyKept = (p: ProfileInfo) => (p.id === 'default' ? t('The default profile stays') : p.active ? t('The app’s own profile: open the app on another first') : p.open ? t('Its Prism runs: stop it first') : p.agents ? tn(p.agents, '{n} agent running: stop it first', '{n} agents running: stop them first') : t('Remove its config, packs, agents’ records and notes (click twice)'))

  /** Stop arms on the first click too. */
  const stopping = armer(3500)
  function askStop(p: ProfileInfo) {
    if (!stopping.confirm(p.id)) return
    void run(`stop:${p.id}`, async () => {
      await stopProfile(p)
      toast(t('{name}: its Prism stopped, its agents go on', { name: p.name }))
    })
  }
  const STATE: Record<string, () => string> = { window: () => t('window'), background: () => t('background'), closed: () => t('closed') }

  /** New profile. */
  let form = $state<NewProfile>({ name: '', color: PROFILE_COLORS[1], start: 'empty', pack: '' })
  let thenSwitch = $state(true)
  const taken = (id: string) => id === 'default' || profiles.list.some((p) => p.id === id)
  const id = $derived(slugOf(form.name))
  const ready = $derived(!!id && !taken(id) && (form.start !== 'pack' || !!form.pack.trim()))
  const create = () => run('create', async () => {
    const p = await createProfile(form.name.trim(), form.color, form.start === 'pack' ? form.pack.trim() : null, form.start === 'demo')
    profilesView.creating = false
    form.name = ''
    form.pack = ''
    toast(t('Profile {name} created', { name: p.name }))
    if (thenSwitch) await openProfile(p)
  })
</script>

<p class="intro">{t('Each profile has its own config.toml and packs, its agents, notes and layout, and its keychain entries. Each opens in a window of the app, its Prism running apart: closing the window leaves it in the background (agents, alerts, the phone), Quit stops the ones the app started. Claude Desktop sessions show in the profile whose repositories they work in, and everywhere when they work in none.')}</p>
{#if error}<p class="err">{error}</p>{/if}

<div class="list">
  {#each profiles.list as p (p.id)}
    <div class="prof" class:cur={p.id === currentProfileId()} style:--c={profileColor(editing?.id === p.id ? editing.color : p.color)}>
      <span class="pf">{initial(editing?.id === p.id ? editing.name : p.name)}</span>
      <div class="info">
        {#if editing?.id === p.id}
          <div class="row">
            <input class="t" bind:value={editing.name} aria-label={t('Profile name')} onkeydown={(e) => { if (e.key === 'Enter') void saveEdit(); if (e.key === 'Escape') editing = null }} />
            <span class="colors">{#each PROFILE_COLORS as c (c)}<button class="sw" class:on={editing.color === c || (!editing.color && c === 'accent')} style:--c={profileColor(c)} onclick={() => (editing!.color = c)} aria-label={t('Color {name}', { name: c })}></button>{/each}</span>
          </div>
        {:else}
          <div class="nm">{p.name}{#if p.active}<span class="pill ok">{t('the app’s')}</span>{/if}<span class="pill" class:ok={p.state === 'window'}>{(STATE[p.state] ?? STATE.closed)()}</span>{#if p.agents}<span class="pill warn">{tn(p.agents, '{n} agent', '{n} agents')}</span>{/if}</div>
        {/if}
        <div class="meta">
          <code>{tildePath(p.config)}</code>
          <span>{p.packs.length ? t('pack {name}', { name: p.packs.join(', ') }) : t('no pack')}</span>
          {#if p.error}<span class="warn">{t('config not readable: {error}', { error: p.error })}</span>{/if}
        </div>
      </div>
      <div class="acts">
        {#if editing?.id === p.id}
          <button class="btn" onclick={() => (editing = null)}>{t('Cancel')}</button>
          <button class="btn primary" onclick={saveEdit} disabled={!editing.name.trim() || !!busy}>{t('Save')}</button>
        {:else}
          {#if p.id !== currentProfileId()}<button class="btn primary" onclick={() => run(`open:${p.id}`, () => openProfile(p))} disabled={!!busy} title={p.state === 'closed' ? t('Open {name} in a window', { name: p.name }) : t('Show {name}’s window', { name: p.name })}>{p.state === 'closed' ? t('Open') : t('Show')}</button>{/if}
          {#if !p.active && p.state !== 'closed'}<button class="btn" class:armed={stopping.is(p.id)} onclick={() => askStop(p)} disabled={!!busy} title={t('Close its window and stop its Prism: its agents go on in tmux (click twice)')}><ArmedLabel label={t('Stop')} armedLabel={t('Click again to stop')} armed={stopping.is(p.id)} /></button>{/if}
          <button class="btn" onclick={() => edit(p)} disabled={!!busy} title={t('Rename it, or change its color')}>{t('Edit')}</button>
          <button class="btn" class:armed={armed.is(p.id)} onclick={() => askRemove(p)} disabled={!removable(p) || !!busy} title={whyKept(p)}><ArmedLabel label={t('Remove')} armedLabel={t('Click again to remove')} armed={armed.is(p.id)} /></button>
        {/if}
      </div>
    </div>
  {/each}
</div>

{#if profilesView.creating}
  <div class="form">
    <h3>{t('New profile')}</h3>
    <ProfileForm bind:value={form} {taken} hint={(id) => `profiles/${id}/`} autofocus>
      {#snippet extra()}
        <span>{t('Then')}</span>
        <label class="check"><input type="checkbox" bind:checked={thenSwitch} /> {t('Switch to it now')}</label>
      {/snippet}
    </ProfileForm>
    <div class="foot">
      <span class="row">
        <button class="btn" onclick={() => (profilesView.creating = false)}>{t('Cancel')}</button>
        <button class="btn primary" onclick={create} disabled={!ready || !!busy}>{busy === 'create' ? t('Creating…') : t('Create')}</button>
      </span>
    </div>
  </div>
{:else}
  <button class="btn add" onclick={() => (profilesView.creating = true)}>+ {t('New profile')}</button>
{/if}

<style>
  .intro { margin: 0 0 12px; font-size: var(--fs-meta); color: var(--text-muted); max-width: 760px; }
  .err { margin: 0 0 10px; color: var(--ko); font-size: var(--fs-meta); }
  .list { display: flex; flex-direction: column; gap: 8px; }
  .prof { --c: var(--accent); display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; gap: 12px; align-items: center; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r9); background: var(--surface); }
  .prof.cur { border-color: color-mix(in srgb, var(--c) 45%, transparent); background: color-mix(in srgb, var(--c) 5%, var(--surface)); }
  .pf { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; border-radius: var(--r9); background: color-mix(in srgb, var(--c) 18%, transparent); color: var(--c); border: 1.5px solid color-mix(in srgb, var(--c) 55%, transparent); font-weight: 700; font-size: 14px; }
  .info { min-width: 0; }
  .nm { display: flex; align-items: center; gap: 7px; font-weight: 600; font-size: 13px; }
  .meta { display: flex; flex-wrap: wrap; gap: 4px 12px; margin-top: 3px; font-size: var(--fs-meta); color: var(--text-muted); }
  .meta code { font: var(--fs-tag) var(--mono); }
  .meta .warn { color: var(--warn); }
  .pill { padding: 0 7px; border-radius: var(--r8); font-size: var(--fs-tag); font-weight: 600; background: var(--surface-muted); color: var(--text-muted); }
  .pill.ok { background: var(--ok-bg); color: var(--ok); }
  .pill.warn { background: var(--warn-bg); color: var(--warn); }
  .acts, .row { display: flex; align-items: center; gap: 6px; }
  .t { height: 28px; min-width: 0; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); outline: none; }
  .t:focus { border-color: var(--accent); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: 0.45; cursor: default; }
  .btn.primary { border-color: var(--accent); background: var(--accent); color: var(--on-accent); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .btn.armed { color: var(--ko); border-color: var(--ko); background: var(--ko-bg); }
  .add { margin-top: 12px; }
  .colors { display: inline-flex; flex-wrap: wrap; gap: 6px; }
  .sw { width: 18px; height: 18px; padding: 0; border: none; border-radius: var(--r5); background: var(--c); cursor: pointer; }
  .sw.on { outline: 2px solid var(--text); outline-offset: 2px; }
  .form { margin-top: 14px; padding: 14px 16px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); max-width: 680px; }
  .form h3 { margin: 0 0 12px; font-size: 14px; }
  .check { display: inline-flex; align-items: center; gap: 6px; font-size: var(--fs-meta); }
  .foot { display: flex; justify-content: flex-end; align-items: center; gap: 12px; margin-top: 14px; padding-top: 12px; border-top: 1px solid var(--border); }
</style>
