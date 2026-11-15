<script lang="ts">
  import { anchored, dismissable, exclusive } from '../popup'
  import { untrack } from 'svelte'
  import { pushLayer } from '../layers.svelte'
  import { previewTheme, setTheme, theme, THEMES } from '../theme.svelte'
  import { appearance } from '../fun.svelte'
  import { pref, setPref } from '../layout.svelte'
  import Icon from './Icon.svelte'
  import { t, tc } from '../i18n.svelte'

  /**
   * Rail button: pick the theme, or follow macOS ("System"); saved in config.toml. Hovering (or
   * tabbing to) a theme shows it on the whole window (`[appearance] theme_preview`); leaving the
   * menu or closing it puts the chosen one back.
   */
  let open = $state(false)
  let button: HTMLButtonElement | undefined = $state()
  let menu: HTMLDivElement | undefined = $state()
  // Open, a layer on top: Escape closes it first. Shown to the stack once it has placed the focus:
  // a theme focused is a theme tried.
  $effect(() => {
    if (!open) return
    let ready = false
    const layer = untrack(() => pushLayer({ kind: 'menu', key: 'theme-menu', el: () => (ready ? menu : undefined), close: () => (open = false), outside: false }))
    queueMicrotask(() => (ready = true))
    return layer.remove
  })

  function toggle(e: MouseEvent) {
    e.stopPropagation()
    open = !open
  }
  /** Collapsed families ("Default", "Catppuccin"…), remembered; the chosen theme stays visible. */
  let collapsed = $state<string[]>(pref<string[]>('themeCollapsed', []))
  function fold(e: MouseEvent, group: string) {
    e.stopPropagation()
    collapsed = collapsed.includes(group) ? collapsed.filter((g) => g !== group) : [...collapsed, group]
    setPref('themeCollapsed', collapsed)
  }
  /** Light, dark or all of them, remembered; the chosen theme stays visible. */
  type Tone = 'all' | 'light' | 'dark'
  let tone = $state<Tone>(pref<Tone>('themeTone', 'all'))
  function setTone(e: MouseEvent, next: Tone) {
    e.stopPropagation()
    tone = next
    setPref('themeTone', next)
  }
  const shown = $derived(THEMES.filter((th) => tone === 'all' || th.tone === tone || theme.choice === th.id))
  function pick(e: MouseEvent, id: string) {
    e.stopPropagation()
    clearTimeout(timer)
    setTheme(id)
    // Tried by hovering: the click settles it.
    open = false
    button?.focus()
  }
  const previews = $derived(appearance().theme_preview !== false)
  /** A short wait: crossing the list does not flash every theme on the way. */
  let timer: ReturnType<typeof setTimeout> | undefined
  function preview(id: string | null) {
    clearTimeout(timer)
    if (!previews) return
    if (id === null) previewTheme(null)
    else timer = setTimeout(() => previewTheme(id), 90)
  }
  $effect(() => {
    if (!open) {
      clearTimeout(timer)
      previewTheme(null)
    }
  })
</script>

<button bind:this={button} class="v" class:on={open} onclick={toggle} title={t('Theme')} aria-label={t('Theme')} aria-expanded={open}>
  <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="4" /><path d="M12 2.5v2.5M12 19v2.5M2.5 12H5M19 12h2.5M5.3 5.3l1.8 1.8M16.9 16.9l1.8 1.8M5.3 18.7l1.8-1.8M16.9 7.1l1.8-1.8" /></svg>
  <span>{t('Theme')}</span>
</button>
{#if open}
  <div class="menu popover" role="menu" tabindex="-1" bind:this={menu} {@attach anchored(() => button, { side: 'right', gap: 8 })} {@attach exclusive(() => (open = false))} {@attach dismissable(() => (open = false), { trigger: () => button, resize: true, escape: false })} onmouseleave={() => preview(null)}>
    <button class="mi" role="menuitemradio" aria-checked={theme.choice === 'system'} class:on={theme.choice === 'system'} onclick={(e) => pick(e, 'system')} onmouseenter={() => preview('system')} onfocus={() => preview('system')}>
      <span class="sw system"><i></i><i></i></span><span class="name">{t('System')}<small>{t('Paper or Graphite, as the system')}</small></span>
    </button>
    <div class="seg tones" role="group" aria-label={t('Light or dark')}>
      {#each [['all', tc('shell', 'All')], ['light', t('Light')], ['dark', t('Dark')]] as [id, label] (id)}
        <button class:on={tone === id} aria-pressed={tone === id} onclick={(e) => setTone(e, id as Tone)} onmouseenter={() => preview(null)}>{label}</button>
      {/each}
    </div>
    {#each shown as th, i (th.id)}
      {#if th.group && th.group !== shown[i - 1]?.group}
        <button class="hd group" class:folded={collapsed.includes(th.group)} onclick={(e) => fold(e, th.group!)} onmouseenter={() => preview(null)} aria-expanded={!collapsed.includes(th.group)}>
          <span class="chev" aria-hidden="true"><Icon name="chevronDown" /></span>{th.group}<small>{shown.filter((x) => x.group === th.group).length}</small>
        </button>
      {/if}
      {#if !(th.group && collapsed.includes(th.group)) || theme.choice === th.id}
        <button class="mi" role="menuitemradio" aria-checked={theme.choice === th.id} class:on={theme.choice === th.id} onclick={(e) => pick(e, th.id)} onmouseenter={() => preview(th.id)} onfocus={() => preview(th.id)}>
          <span class="sw" style="background: {th.swatch[0]}">{#each th.swatch as c, n (n)}<i style="background: {c}"></i>{/each}</span>
          <span class="name">{th.label}<small>{th.hint}</small></span>
        </button>
      {/if}
    {/each}
    {#if previews}<p class="tip">{t('Hover to try a theme on the window, click to keep it.')}</p>{/if}
  </div>
{/if}

<style>
  .menu { position: fixed; width: 230px; overflow: auto; display: flex; flex-direction: column; }
  .menu .mi { padding-block: 4px; }
  .group { display: flex; align-items: center; gap: 4px; margin-top: 4px; border: none; background: none; font-family: inherit; text-align: left; cursor: pointer; }
  .group:hover { color: var(--text); }
  .group small { margin-left: auto; }
  .chev { display: inline-flex; width: 10px; transition: transform 0.12s; }
  .group.folded .chev { transform: rotate(-90deg); }
  .tones { display: flex; margin: 4px 0 2px; }
  .tones > button { flex: 1; justify-content: center; }
  .tip { margin: 4px 0 0; padding: 6px 8px 3px; border-top: 1px solid var(--border); font-size: var(--fs-tag); color: var(--text-muted); }
  .name { display: flex; flex-direction: column; line-height: 1.25; }
  .name small { font-size: var(--fs-tag); color: var(--text-muted); }
  .sw { display: grid; grid-template-columns: repeat(4, 6px); gap: 2px; padding: 3px; border-radius: var(--r5); box-shadow: inset 0 0 0 1px var(--border); }
  .sw i { width: 6px; height: 16px; border-radius: var(--r2); }
  .sw.system { grid-template-columns: repeat(2, 13px); background: linear-gradient(90deg, #f6f5f1 50%, #1a1a18 50%); }
  .sw.system i { width: 13px; }
</style>
