<script lang="ts">
  import { onMount } from 'svelte'
  import { anchored, dismissable, exclusive } from '../popup'
  import { armer } from '../armed.svelte'
  import { closeShells, goToShells, openShells, refreshShells, shellGroups, type ShellGroup } from '../open-shells.svelte'
  import { shellDrawer } from '../shell-drawer.svelte'
  import { ago } from '../time'
  import Icon from './Icon.svelte'
  import { t, tn } from '../i18n.svelte'

  /**
   * The shells with no agent still open (a worktree's, a pod's) in the status bar: shown while there
   * is one. tmux keeps them once their drawer is closed; a click lists them, a line opens its drawer
   * again, × closes its shells (click twice). Read again when the shells drawer opens or closes,
   * when the list opens and when the window comes back.
   */
  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  const armed = armer()
  const groups = $derived(openShells.list.length ? shellGroups() : [])

  onMount(() => {
    const back = () => document.visibilityState === 'visible' && refreshShells()
    document.addEventListener('visibilitychange', back)
    window.addEventListener('focus', back)
    return () => {
      document.removeEventListener('visibilitychange', back)
      window.removeEventListener('focus', back)
    }
  })
  $effect(() => {
    void shellDrawer.owner
    refreshShells()
  })

  function toggle(e: MouseEvent) {
    e.stopPropagation()
    open = !open
    if (open) refreshShells()
  }
  function go(g: ShellGroup) {
    open = false
    goToShells(g)
  }
  async function close(g: ShellGroup) {
    if (armed.confirm(g.id)) await closeShells(g)
  }
  const badge = (g: ShellGroup) => (g.pod && g.ended ? t('kubectl exited') : g.running.length ? g.running.join(', ') : g.shells.length > 1 ? tn(g.shells.length, '{n} tab', '{n} tabs') : '')
</script>

{#if groups.length}
  <button bind:this={button} class="sbi shells" class:open onclick={toggle} aria-expanded={open} title={tn(groups.length, '{n} place with shells still open (tmux keeps them) · list', '{n} places with shells still open (tmux keeps them) · list')}>
    <span class="g"><Icon name="terminal" /></span><b>{groups.length}</b> {tn(groups.length, 'shell', 'shells')}
  </button>
  {#if open}
    <div class="pop" role="dialog" aria-label={t('Open shells')} {@attach anchored(() => button, { align: 'start', gap: 6 })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button })}>
      <p><span>{t('Open shells')}</span><span>{t('tmux keeps them after closing')}</span></p>
      {#each groups as g (g.id)}
        <div class="row">
          <button class="go" onclick={() => go(g)} title={t('Open its shells')}>
            <span class="ic" class:pod={g.pod}><Icon name={g.pod ? 'kube' : 'branch'} /></span>
            <span class="txt"><b>{g.title}</b><small>{g.where}</small></span>
            {#if badge(g)}<span class="st" class:ko={g.pod && g.ended} class:busy={!g.pod && g.running.length > 0}>{badge(g)}</span>{/if}
            <span class="when">{ago(new Date(g.started * 1000).toISOString())}</span>
          </button>
          <button class="x" class:armed={armed.is(g.id)} onclick={() => close(g)} title={g.pod ? t('Close its shells: kubectl exec ends (click twice)') : t('Close its shells, and what they run (click twice)')} aria-label={t('Close')}>{#if armed.is(g.id)}{t('Close?')}{:else}<Icon name="close" />{/if}</button>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  .shells .g { color: var(--src-k8s); }
  .shells.open { background: var(--surface-muted); color: var(--text); }
  .pop { position: fixed; z-index: 45; width: 400px; padding: 4px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--r10); box-shadow: var(--shadow-pop); font-size: var(--fs-body); color: var(--text); }
  .pop p { display: flex; justify-content: space-between; gap: 8px; margin: 0; padding: 5px 8px 6px; font-size: var(--fs-tag); text-transform: uppercase; letter-spacing: 0.4px; color: var(--text-muted); }
  .pop p span:last-child { text-transform: none; letter-spacing: 0; }
  .row { display: flex; align-items: center; border-radius: var(--r6); }
  .row:hover { background: var(--surface-muted); }
  button { border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
  .go { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; padding: 5px 8px; text-align: left; }
  .ic { display: inline-grid; place-items: center; flex: none; width: 20px; height: 20px; border-radius: var(--r5); background: var(--surface-muted); color: var(--text-muted); }
  .ic.pod { background: color-mix(in srgb, var(--src-k8s) 16%, transparent); color: var(--src-k8s); }
  .txt { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .txt b, .txt small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txt b { font-weight: 500; font-family: var(--mono); font-size: var(--fs-mono); }
  .txt small { color: var(--text-muted); font-size: var(--fs-meta); }
  .st { flex: none; max-width: 120px; overflow: hidden; text-overflow: ellipsis; padding: 0 6px; border-radius: var(--r-chip); background: var(--surface-muted); color: var(--text-muted); font-size: var(--fs-tag); }
  .st.ko { background: var(--ko-bg); color: var(--ko); }
  .st.busy { background: var(--info-bg); color: var(--info); }
  .when { flex: none; color: var(--text-muted); font-size: var(--fs-meta); }
  .x { flex: none; display: inline-flex; align-items: center; height: 22px; margin-right: 4px; padding: 0 5px; border-radius: var(--r5); color: var(--text-muted); opacity: 0; font-size: var(--fs-meta); }
  .row:hover .x, .x:focus-visible, .x.armed { opacity: 1; }
  .x:hover { background: var(--surface); color: var(--text); }
  .x.armed { color: var(--ko); }
</style>
