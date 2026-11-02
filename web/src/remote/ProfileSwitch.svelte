<script lang="ts">
  import { t, tn, tc } from '../i18n.svelte'
  import Icon from '../components/Icon.svelte'
  import { initial, profileColor } from '../profiles.svelte'
  import { ALL, openProfile, phoneProfiles, refreshProfiles, shown, switchProfile, type PhoneProfile } from './phone-profiles.svelte'

  /**
   * The header's profile switcher (several profiles on the Mac, `phone-profiles`): the profile
   * shown, its initial on its color, or the badges of All; a tap opens the sheet of choices: All,
   * then each profile with what it is doing, a closed one with Open (its Prism launched on the Mac
   * beside this one, confirmed by the passkey).
   */
  let open = $state(false)
  let opening = $state<string | null>(null)
  let error = $state<string | null>(null)
  const all = $derived(phoneProfiles.choice === ALL)
  const current = $derived(shown())
  const reachable = $derived(phoneProfiles.list.filter((p) => p.reachable))
  let poll: ReturnType<typeof setInterval> | undefined

  function toggle() {
    open = !open
    error = null
    clearInterval(poll)
    if (open) {
      void refreshProfiles()
      // A profile opened from here shows once its Prism answers.
      poll = setInterval(() => void refreshProfiles(), 4000)
    }
  }
  $effect(() => () => clearInterval(poll))

  async function start(p: PhoneProfile) {
    opening = p.id
    error = null
    try {
      await openProfile(p.id)
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      opening = null
    }
  }

  const status = (p: PhoneProfile) =>
    p.reachable
      ? [tn(p.agents, '{n} agent', '{n} agents'), p.waiting ? tn(p.waiting, '{n} waiting', '{n} waiting') : ''].filter(Boolean).join(' · ')
      : p.open ? t('Open on the computer; its remote is not relayed here') : t('Closed on the computer')
</script>

{#snippet badge(p: PhoneProfile, big = false)}
  <span class="pf" class:big style:--c={profileColor(p.color)}>{initial(p.name)}</span>
{/snippet}

<button class="sw" onclick={toggle} aria-haspopup="dialog" aria-expanded={open} aria-label={t('Profile: {name}', { name: all ? tc('profile', 'All') : (current?.name ?? '') })}>
  {#if all}
    {#each reachable as p (p.id)}{@render badge(p)}{/each}<span>{tc('profile', 'All')}</span>
  {:else if current}
    {@render badge(current)}<span>{current.name}</span>
  {/if}
  <Icon name="chevronDown" size="12px" />
</button>

{#if open}
  <button class="shade" onclick={toggle} aria-label={t('Close')}></button>
  <div class="sheet" role="dialog" aria-label={t('Profile')}>
    <div class="grab"></div>
    <h3>{t('Profile')}</h3>
    <button class="opt" onclick={() => switchProfile(ALL)}>
      <span class="pf big every"><Icon name="group" size="16px" /></span>
      <span class="b"><b>{tc('profile', 'All')}</b><small>{t('Home and Agents of every profile; the other views: {name}’s', { name: phoneProfiles.list.find((p) => p.hub)?.name ?? '' })}</small></span>
      {#if all}<span class="chk">✓</span>{/if}
    </button>
    {#each phoneProfiles.list as p (p.id)}
      <div class="opt" class:off={!p.reachable}>
        <button class="pick" disabled={!p.reachable} onclick={() => switchProfile(p.id)}>
          {@render badge(p, true)}
          <span class="b"><b>{p.name}</b><small>{status(p)}</small></span>
          {#if !all && current?.id === p.id}<span class="chk">✓</span>{/if}
        </button>
        {#if !p.open}<button class="btn" disabled={opening === p.id} onclick={() => start(p)}>{opening === p.id ? t('Opening…') : t('Open')}</button>{/if}
      </div>
    {/each}
    {#if error}<p class="note ko">{error}</p>{/if}
    <p class="note">{t('Open launches its Prism on the computer, beside this one, confirmed by your passkey; it shows here once it answers, if its remote is relayed (Settings › Remote › Access › Tunnel relay).')}</p>
  </div>
{/if}

<style>
  .sw { display: inline-flex; align-items: center; gap: 4px; min-height: 30px; padding: 2px 8px 2px 4px; border: none; border-radius: 10px; background: var(--surface-muted); color: var(--text); font: inherit; font-size: 13px; }
  .sw > span:not(.pf) { max-width: 92px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sw :global(svg) { color: var(--text-muted); }
  .pf { flex: none; display: inline-flex; align-items: center; justify-content: center; width: 20px; height: 20px; border-radius: 6px; border: 1px solid color-mix(in srgb, var(--c) 55%, transparent); background: color-mix(in srgb, var(--c) 20%, transparent); color: var(--c); font-size: 11px; font-weight: 700; }
  .pf.big { width: 34px; height: 34px; border-radius: 10px; font-size: 16px; }
  .pf.every { --c: var(--text-muted); }
  .shade { position: fixed; inset: 0; z-index: 30; padding: 0; border: none; background: rgba(0, 0, 0, 0.45); }
  .sheet { position: fixed; left: 0; right: 0; bottom: 0; z-index: 31; max-height: 80dvh; overflow-y: auto; padding: 8px 16px calc(18px + env(safe-area-inset-bottom)); border-radius: 18px 18px 0 0; background: var(--surface); color: var(--text); }
  .grab { width: 38px; height: 4px; margin: 0 auto 12px; border-radius: 2px; background: var(--border); }
  h3 { margin: 0 0 6px; font-size: 16px; }
  .opt { width: 100%; display: flex; align-items: center; gap: 10px; padding: 8px 0; border: none; border-bottom: 1px solid var(--border); background: none; color: var(--text); font: inherit; text-align: left; }
  .pick { flex: 1; min-width: 0; display: flex; align-items: center; gap: 12px; padding: 2px 4px; border: none; background: none; color: inherit; font: inherit; text-align: left; }
  button.opt { padding: 10px 4px; gap: 12px; }
  .opt.off .b { opacity: 0.7; }
  .b { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .b b { font-size: 15px; font-weight: 600; }
  .b small { color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  .chk { color: var(--accent); font-weight: 700; font-size: 16px; }
  .btn { flex: none; padding: 6px 12px; border: 1px solid var(--accent); border-radius: 16px; background: none; color: var(--accent); font: inherit; font-size: 13px; }
  .btn:disabled { opacity: 0.6; }
  .note { margin: 12px 4px 0; color: var(--text-muted); font-size: 12px; overflow-wrap: anywhere; }
  .note.ko { color: var(--ko); }
</style>
