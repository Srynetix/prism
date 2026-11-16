<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { untrack } from 'svelte'
  import { pushLayer } from '../layers.svelte'
  import { keys } from '../platform'
  import { activeProfile, currentProfileId, initial, loadProfiles, openProfile, profileColor, profiles, profilesView } from '../profiles.svelte'
  import { closeAgents } from '../agents.svelte'
  import { openSettings } from '../settings.svelte'
  import { toastError } from '../toast.svelte'
  import { t, tn } from '../i18n.svelte'
  import { invoke, isDesktop, machine, machineProfile } from '../bridge'

  import type { Machine } from '../bindings/Machine'

  /**
   * Foot of the rail: this window's profile badge, its letter on its color. Its menu lists the
   * profiles (⌥⌘1…), each in a window of the app (picked: its window in front, opened if need be);
   * New and Manage open Settings › General › Profiles. Below, the other computers' Prism
   * (`[[machines]]`), each opened in a window of its own. In a machine's window, the badge is that
   * machine's profile.
   */
  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  let menu: HTMLDivElement | undefined = $state()
  // Open, a layer on top: Escape closes it first.
  $effect(() => {
    if (!open) return
    const layer = untrack(() => pushLayer({ kind: 'menu', key: 'profile-menu', el: () => menu, close: () => (open = false), outside: false }))
    return layer.remove
  })
  const me = $derived(activeProfile())
  const here = machine()
  const hereProfile = machineProfile()
  /** This window is machine `name`'s profile `profile` (`null`: its hub's). */
  const isHere = (name: string, profile: string | null) => name === here && profile === hereProfile
  /** This window shows profile `id` of this computer. */
  const mine = (id: string) => !here && id === currentProfileId()
  let machines = $state<Machine[]>([])

  async function toggle(e: MouseEvent) {
    e.stopPropagation()
    open = !open
    if (open) {
      await loadProfiles().catch(() => {})
      if (isDesktop()) machines = await invoke<Machine[]>('machine_list').catch(() => [])
    }
  }
  async function pick(e: MouseEvent, id: string) {
    e.stopPropagation()
    const p = profiles.list.find((x) => x.id === id)
    open = false
    if (!p || mine(id)) return
    await openProfile({ id }).catch((err) => toastError(String(err)))
  }
  async function openMachine(e: MouseEvent, name: string, profile: string | null = null) {
    e.stopPropagation()
    open = false
    await invoke('machine_open', { name, profile }).catch((err) => toastError(String(err)))
  }
  function settingsMachines(e: MouseEvent) {
    e.stopPropagation()
    open = false
    closeAgents()
    openSettings('machines')
  }
  function manage(e: MouseEvent, creating: boolean) {
    e.stopPropagation()
    open = false
    closeAgents()
    profilesView.creating = creating
    openSettings('profiles')
  }
  const describe = (p: (typeof profiles.list)[number]) =>
    [p.packs.length ? t('pack {name}', { name: p.packs.join(', ') }) : '', p.agents ? tn(p.agents, '{n} agent running', '{n} agents running') : ''].filter(Boolean).join(' · ')
</script>

<button bind:this={button} class="v pf-btn" class:on={open} onclick={toggle} title={t('Profile: {name}', { name: me.name })} aria-label={t('Profile {name}', { name: me.name })} aria-expanded={open}>
  <span class="pf" class:rm={!!here} style:--c={profileColor(me.color)}>{initial(me.name)}</span>
  <span>{me.name}</span>
</button>
{#if open}
  <div class="menu popover" role="menu" tabindex="-1" bind:this={menu} {@attach anchored(() => button, { side: 'right', gap: 8, align: 'end' })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true, escape: false })}>
    <div class="hd">{here ? t('This computer') : t('Profile')}</div>
    {#each profiles.list as p, i (p.id)}
      <div class="row">
        <button class="mi" role="menuitemradio" aria-checked={mine(p.id)} class:cur={mine(p.id)} style:--c={profileColor(p.color)} onclick={(e) => pick(e, p.id)} title={mine(p.id) ? t('This window') : p.state === 'closed' ? t('Open {name} in a window', { name: p.name }) : t('Show {name}’s window', { name: p.name })}>
          <span class="chk">{mine(p.id) ? '✓' : ''}</span>
          <span class="pf sm" style:--c={profileColor(p.color)}>{initial(p.name)}</span>
          <span class="name">{p.name}{#if !mine(p.id) && p.state === 'window'}<em>{t('window')}</em>{:else if p.state === 'background'}<em class="bg">{t('background')}</em>{/if}<small>{p.error ? t('config not readable') : describe(p)}</small></span>
          {#if i < 9}<kbd class="k">{keys(`⌥⌘${i + 1}`)}</kbd>{/if}
        </button>
      </div>
    {/each}
    {#if machines.length}
      <div class="hd">{t('Other computers')}</div>
      {#each machines as m (m.name)}
        {@const hub = m.profiles.find((p) => p.hub)}
        <button class="mi" role="menuitemradio" aria-checked={isHere(m.name, null)} class:cur={isHere(m.name, null)} style:--c="var(--info)" onclick={(e) => openMachine(e, m.name)} title={isHere(m.name, null) ? t('This window') : t('{name}’s Prism, in a window of its own', { name: m.name })}>
          <span class="chk">{isHere(m.name, null) ? '✓' : ''}</span>
          <span class="pf sm rm" style:--c={hub ? profileColor(hub.color) : 'var(--info)'}>{initial(m.name)}</span>
          <span class="name">{m.name}{#if m.open && !isHere(m.name, null)}<em>{t('open')}</em>{/if}<small>{!m.token ? t('no token yet') : hub ? `${hub.name}${hub.agents ? ` · ${tn(hub.agents, '{n} agent running', '{n} agents running')}` : ''}` : m.url}</small></span>
        </button>
        <!-- Its other profiles, reached through it: each in a window of its own. -->
        {#each m.profiles.filter((p) => !p.hub) as p (p.id)}
          <button class="mi sub" role="menuitemradio" aria-checked={isHere(m.name, p.id)} class:cur={isHere(m.name, p.id)} style:--c={profileColor(p.color)} disabled={!p.reachable} onclick={(e) => openMachine(e, m.name, p.id)} title={p.reachable ? t('{profile} on {name}, in a window of its own', { profile: p.name, name: m.name }) : t('Its Prism is closed on {name}, or its remote not relayed there', { name: m.name })}>
            <span class="chk">{isHere(m.name, p.id) ? '✓' : ''}</span>
            <span class="pf sm rm" style:--c={profileColor(p.color)}>{initial(p.name)}</span>
            <span class="name">{p.name}<small>{!p.reachable ? t('closed there') : p.waiting ? tn(p.waiting, '{n} agent waiting', '{n} agents waiting') : tn(p.agents, '{n} agent running', '{n} agents running')}</small></span>
          </button>
        {/each}
      {/each}
    {/if}
    <hr />
    <p class="note">{t('Each profile in a window of its own (⌘` from one to the other); closed, its Prism goes on in the background. Agents keep running, in their own profile.')}</p>
    <button class="mi" onclick={(e) => manage(e, true)}><span class="chk"></span>{t('New profile…')}</button>
    <button class="mi" onclick={(e) => manage(e, false)}><span class="chk"></span>{t('Manage profiles…')}</button>
    {#if isDesktop() && !here}<button class="mi" onclick={settingsMachines}><span class="chk"></span>{t('Other computers…')}</button>{/if}
  </div>
{/if}

<style>
  /* Its own color on the badge itself (`style:--c`): this rule's default would hide a parent's. */
  .pf { --c: var(--accent); display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: var(--r7); background: color-mix(in srgb, var(--c) 18%, transparent); color: var(--c); border: 1.5px solid color-mix(in srgb, var(--c) 55%, transparent); font-weight: 700; font-size: var(--fs-body); line-height: 1; }
  /* Another computer's: a dot of its own color. */
  .pf.rm { position: relative; }
  .pf.rm::after { content: ''; position: absolute; right: -4px; bottom: -4px; width: 9px; height: 9px; border-radius: 50%; background: var(--info); border: 2px solid var(--surface); }
  .pf.sm { width: 18px; height: 18px; border-radius: var(--r5); font-size: var(--fs-tag); border-width: 1px; flex-shrink: 0; }
  .menu { position: fixed; width: 270px; display: flex; flex-direction: column; }
  .menu .mi { padding-block: 4px; }
  .mi.sub { padding-left: 22px; }
  .row { display: flex; align-items: stretch; gap: 2px; }
  .row > button:first-child { flex: 1; min-width: 0; }
  .name em.bg { background: var(--surface-muted); color: var(--text-muted); }
  .name em { margin-left: 6px; padding: 0 5px; border-radius: var(--r6); background: var(--ok-bg); color: var(--ok); font-style: normal; font-size: var(--fs-tag); }
  .menu .mi.cur { background: color-mix(in srgb, var(--c) 10%, transparent); }
  .chk { width: 12px; flex-shrink: 0; color: var(--c, var(--accent)); font-weight: 700; }
  .name { display: flex; flex-direction: column; min-width: 0; line-height: 1.25; }
  .name small { font-size: var(--fs-tag); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mi kbd.k { padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); font-family: var(--mono); }
  hr { width: 100%; }
  .note { margin: 0; padding: 2px 8px 5px; font-size: var(--fs-meta); color: var(--text-muted); white-space: normal; }
</style>
