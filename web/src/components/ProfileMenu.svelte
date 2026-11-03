<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { keys } from '../platform'
  import { activeProfile, initial, loadProfiles, openProfile, profileColor, profiles, profilesView, switchProfile } from '../profiles.svelte'
  import { closeAgents } from '../agents.svelte'
  import { openSettings } from '../settings.svelte'
  import { toast } from '../toast.svelte'
  import { t, tn } from '../i18n.svelte'
  import { invoke, isDesktop, machine, machineProfile } from '../bridge'
  import type { MachineProfile } from '../bindings/MachineProfile'

  interface Machine { name: string; url: string; ssh: string; token: boolean; open: boolean; profiles: MachineProfile[] }

  /**
   * Foot of the rail: the active profile's badge, its letter on its color. Its menu lists the
   * profiles (⌥⌘1…) and switches to one, which relaunches Prism; New and Manage open Settings › General › Profiles.
   * Below, the other computers' Prism (`[[machines]]`), each opened in a window of its own. In a
   * machine's window, the badge is that machine's profile, and this computer's profiles open its
   * own window rather than relaunching anything.
   */
  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  const me = $derived(activeProfile())
  const here = machine()
  const hereProfile = machineProfile()
  /** This window is machine `name`'s profile `profile` (`null`: its hub's). */
  const isHere = (name: string, profile: string | null) => name === here && profile === hereProfile
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
    if (!p) return
    // From a machine's window, this computer's profile is its own window; another, its own Prism.
    if (here) return void (p.active ? invoke('machine_main') : openProfile({ id })).catch((err) => toast(String(err), 6000))
    if (p.active) return
    await switchProfile(p).catch((err) => toast(String(err), 6000))
  }
  async function openMachine(e: MouseEvent, name: string, profile: string | null = null) {
    e.stopPropagation()
    open = false
    await invoke('machine_open', { name, profile }).catch((err) => toast(String(err), 6000))
  }
  function settingsMachines(e: MouseEvent) {
    e.stopPropagation()
    open = false
    closeAgents()
    openSettings('machines')
  }
  /** Next to this Prism, in its own: Open alongside. */
  async function alongside(e: MouseEvent, id: string) {
    e.stopPropagation()
    open = false
    await openProfile({ id }).catch((err) => toast(String(err), 6000))
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
  <div class="menu" role="menu" tabindex="-1" {@attach anchored(() => button, { side: 'right', gap: 8, align: 'end' })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true })}>
    <div class="t">{here ? t('This computer') : t('Profile')}</div>
    {#each profiles.list as p, i (p.id)}
      <div class="row">
        <button role="menuitemradio" aria-checked={p.active && !here} class:cur={p.active && !here} style:--c={profileColor(p.color)} onclick={(e) => pick(e, p.id)} title={p.active ? t('This Prism') : p.open ? t('Open in another Prism: bring it to the front') : t('Switch this Prism to {name}', { name: p.name })}>
          <span class="chk">{p.active && !here ? '✓' : ''}</span>
          <span class="pf sm">{initial(p.name)}</span>
          <span class="name">{p.name}{#if p.open}<em>{t('open')}</em>{/if}<small>{p.error ? t('config not readable') : describe(p)}</small></span>
          {#if i < 9}<kbd>{keys(`⌥⌘${i + 1}`)}</kbd>{/if}
        </button>
        {#if !p.active && !p.open && !here}<button class="side" onclick={(e) => alongside(e, p.id)} title={t('Open {name} alongside, in its own Prism', { name: p.name })} aria-label={t('Open {name} alongside', { name: p.name })}>⧉</button>{/if}
      </div>
    {/each}
    {#if machines.length}
      <div class="t">{t('Other computers')}</div>
      {#each machines as m (m.name)}
        {@const hub = m.profiles.find((p) => p.hub)}
        <button role="menuitemradio" aria-checked={isHere(m.name, null)} class:cur={isHere(m.name, null)} style:--c="var(--info)" onclick={(e) => openMachine(e, m.name)} title={isHere(m.name, null) ? t('This window') : t('{name}’s Prism, in a window of its own', { name: m.name })}>
          <span class="chk">{isHere(m.name, null) ? '✓' : ''}</span>
          <span class="pf sm rm">{initial(m.name)}</span>
          <span class="name">{m.name}{#if m.open && !isHere(m.name, null)}<em>{t('open')}</em>{/if}<small>{!m.token ? t('no token yet') : hub ? `${hub.name}${hub.agents ? ` · ${tn(hub.agents, '{n} agent running', '{n} agents running')}` : ''}` : m.url}</small></span>
        </button>
        <!-- Its other profiles, reached through it: each in a window of its own. -->
        {#each m.profiles.filter((p) => !p.hub) as p (p.id)}
          <button class="sub" role="menuitemradio" aria-checked={isHere(m.name, p.id)} class:cur={isHere(m.name, p.id)} style:--c={profileColor(p.color)} disabled={!p.reachable} onclick={(e) => openMachine(e, m.name, p.id)} title={p.reachable ? t('{profile} on {name}, in a window of its own', { profile: p.name, name: m.name }) : t('Its Prism is closed on {name}, or its remote not relayed there', { name: m.name })}>
            <span class="chk">{isHere(m.name, p.id) ? '✓' : ''}</span>
            <span class="pf sm rm">{initial(p.name)}</span>
            <span class="name">{p.name}<small>{!p.reachable ? t('closed there') : p.waiting ? tn(p.waiting, '{n} agent waiting', '{n} agents waiting') : tn(p.agents, '{n} agent running', '{n} agents running')}</small></span>
          </button>
        {/each}
      {/each}
    {/if}
    <hr />
    <p class="note">{t('Switching relaunches Prism; ⧉ opens a profile alongside, in its own Prism. Agents keep running, in their own profile.')}</p>
    <button onclick={(e) => manage(e, true)}><span class="chk"></span>{t('New profile…')}</button>
    <button onclick={(e) => manage(e, false)}><span class="chk"></span>{t('Manage profiles…')}</button>
    {#if isDesktop() && !here}<button onclick={settingsMachines}><span class="chk"></span>{t('Other computers…')}</button>{/if}
  </div>
{/if}

<style>
  .pf { --c: var(--accent); display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: var(--r7); background: color-mix(in srgb, var(--c) 18%, transparent); color: var(--c); border: 1.5px solid color-mix(in srgb, var(--c) 55%, transparent); font-weight: 700; font-size: 12px; line-height: 1; }
  /* Another computer's: a dot of its own color. */
  .pf.rm { position: relative; }
  .pf.rm::after { content: ''; position: absolute; right: -4px; bottom: -4px; width: 9px; height: 9px; border-radius: 50%; background: var(--info); border: 2px solid var(--surface); }
  .pf.sm { width: 18px; height: 18px; border-radius: var(--r5); font-size: 10px; border-width: 1px; flex-shrink: 0; }
  .menu { position: fixed; z-index: 45; width: 270px; padding: 4px; display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r8); box-shadow: var(--shadow-pop); }
  .menu .t { padding: 4px 8px 3px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .menu button { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .menu button:hover { background: var(--surface-muted); }
  .menu button.sub { padding-left: 22px; }
  .menu button:disabled { opacity: 0.5; cursor: default; }
  .row { display: flex; align-items: stretch; gap: 2px; }
  .row > button:first-child { flex: 1; min-width: 0; }
  .menu .side { flex: 0 0 auto; justify-content: center; width: 28px; padding: 0; color: var(--text-muted); font-size: 13px; }
  .menu .side:hover { color: var(--text); }
  .name em { margin-left: 6px; padding: 0 5px; border-radius: var(--r6); background: var(--ok-bg); color: var(--ok); font-style: normal; font-size: var(--fs-tag); }
  .menu button.cur { background: color-mix(in srgb, var(--c) 10%, transparent); }
  .chk { width: 12px; flex-shrink: 0; color: var(--c, var(--accent)); font-weight: 700; }
  .name { display: flex; flex-direction: column; min-width: 0; line-height: 1.25; }
  .name small { font-size: var(--fs-tag); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  kbd { margin-left: auto; font: 10px var(--mono); padding: 0 4px; border: 1px solid var(--border); border-radius: var(--r4); color: var(--text-muted); }
  hr { width: 100%; border: 0; border-top: 1px solid var(--border); margin: 4px 0; }
  .note { margin: 0; padding: 2px 8px 5px; font-size: var(--fs-meta); color: var(--text-muted); }
</style>
