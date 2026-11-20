<script lang="ts">
  import { untrack } from 'svelte'
  import { suggest } from '../suggest'
  import { armer } from '../armed.svelte'
  import { invoke, isDesktop, isLocal } from '../bridge'
  import Icon from './Icon.svelte'
  import Markdown from './Markdown.svelte'
  import { canPick, pickPath } from '../pick'
  import ArmedLabel from './ArmedLabel.svelte'
  import ProfilesSettings from './ProfilesSettings.svelte'
  import Field from './settings/Field.svelte'
  import { DEFAULT_NOTES_KEY } from '../quick-notes.svelte'
  import ShortcutRecorder from './settings/ShortcutRecorder.svelte'
  import TomlPanel, { tomlModeOf } from './settings/TomlPanel.svelte'
  import SettingsSearch, { SearchState } from './settings/SettingsSearch.svelte'
  import StateMark, { pageState } from './settings/StateMark.svelte'
  import { currentZoom, setZoom, ZOOMS, zoomLabel } from '../zoom.svelte'
  import DictationSettings from './settings/DictationSettings.svelte'
  import McpSettings from './settings/McpSettings.svelte'
  import { configurePrism } from '../prism-agent'
  import RemoteDevices from './settings/RemoteDevices.svelte'
  import RemoteAway from './settings/RemoteAway.svelte'
  import FeedbackRequests from './settings/FeedbackRequests.svelte'
  import DependenciesPanel from './settings/DependenciesPanel.svelte'
  import CliCommandPanel from './settings/CliCommandPanel.svelte'
  import CustomBlocksSettings from './settings/CustomBlocksSettings.svelte'
  import RoutinesSettings from './settings/RoutinesSettings.svelte'
  import CleanupUsage from './settings/CleanupUsage.svelte'
  import { broughtBy } from '../feedback.svelte'
  import SectionFields from './settings/SectionFields.svelte'
  import ListInput from './settings/ListInput.svelte'
  import { DEFAULT_HIDDEN, STATUS_ITEMS, SIDE_BREAK } from '../statusbar'
  import { statusbarEditing } from '../statusbar-edit.svelte'
  import { SCHEMA } from '../settings-schema'
  import { ensure, failing, recheck, runs, sectionRequests } from '../settings-checks.svelte'
  import { live } from '../live.svelte'
  import { chainOf, descendants, groupsOf, isProvider, nodeOf, PARENT, pageOf, TREE, type TreeNode } from '../settings-tree'
  import { fieldsOf, pathOf, SECTION, type Section } from '../settings-sections'
  import { shown as showValue } from '../settings-search'
  import { cfg, changes, discardConfig, discardValue, showSettings, editable, importPack, isChanged, loadConfig, mergedRows, packBusy, packRef, removePack, restartApp, saveConfig, savedValue, raw, saveRaw, setValue, settingsView, updatePack, valueOf } from '../settings.svelte'
  import { DEFAULT_APPEARANCE, system, replayBoot, SAVER_MODES, startSaver, type Feature, type SaverMode } from '../fun.svelte'
  import { pref, setPref } from '../layout.svelte'
  import { THEMES } from '../theme.svelte'
  import { LANGUAGES, t, tc, tn } from '../i18n.svelte'
  import { COMMIT, versionLabel } from '../version'
  import { ensureFont, installed, MONO_FONTS, monoStack, SANS_FONTS, sansStack, terminalStack } from '../fonts.svelte'
  import { parseShortcut, worksWhileTyping } from '../shortcut'
  import { FILE_MANAGER, isMac, keys } from '../platform'
  import { checkPacks, checkUpdates, installUpdate, packCheck, packUpdates, updates } from '../updates.svelte'
  import { openLink } from '../links'
  import ModulesSettings from './settings/ModulesSettings.svelte'

  /**
   * Settings view: the mirror of config.toml. Seven domains of pages sorted by what they configure
   * (settings-tree.ts), opening on their overview; the chosen page — one or more sections, each a
   * table of the file, a provider's chosen at its top — and, when asked, the file alongside,
   * showing what saving would write. A search lists the keys themselves (settings-search.ts).
   * Appearance gathers the app preferences (theme, Fun), applied immediately; the rest is saved
   * with ⌘S, the bar at the bottom listing each change.
   */
  const sectionsOf = (n: TreeNode) => (n.sections ?? []).map((id) => SECTION.get(id)).filter((s) => !!s)

  /* ---------- Page ---------- */
  /**
   * What a link names (`settingsView.section`): a page, a provider (shown in its page), a section
   * (scrolled to), a domain (its pages), or nothing: the overview.
   */
  const target = $derived(settingsView.section === 'overview' ? undefined : nodeOf(settingsView.section))
  const page = $derived(target ? (pageOf(target.id) ?? target) : undefined)
  /** A domain shows its pages, as the overview does for all. */
  const domain = $derived(page && !PARENT.get(page.id) ? page : undefined)
  const chain = $derived(page ? chainOf(page) : [])
  const pageSections = $derived(page && !domain ? sectionsOf(page) : [])
  /** A page in tabs (a delivery model's stages): its first section, then the chosen tab. */
  const tab = $derived(page?.tabs ? (pageSections.slice(1).find((s) => s.id === settingsView.section) ?? pageSections[1]) : undefined)
  const shownSections = $derived(page?.tabs ? [pageSections[0], tab!] : pageSections)
  /** Shows a page (or the overview): out of the search, which would hide it. */
  const go = (id: string) => {
    showSettings(id)
    search.reset()
  }
  /** “Delivery · branch stage” → “Branch stage”: a tab's name. */
  const stageLabel = (label: string) => label.replace(/^.*· /, '').replace(/^./, (c) => c.toUpperCase())

  /* ---------- Providers ---------- */
  /** The value of a page's choice (Tracker › Provider…). */
  const chosen = (n: TreeNode | null | undefined) => (n?.choice ? String(valueOf(n.choice.path, n.choice.fallback)) : undefined)
  const providers = $derived(page?.children?.filter(isProvider) ?? [])
  const chosenProvider = $derived(providers.find((p) => p.provider === chosen(page)))
  const otherProviders = $derived(providers.filter((p) => p !== chosenProvider))
  /** Once each: Repositories' two sections both live in `[[repos]]`. */
  const tables = $derived([...new Set([...pageSections, ...(chosenProvider ? sectionsOf(chosenProvider) : [])].map((s) => s.table))])
  /** The repositories running an agent CLI of their own (`[[repos]] agent`), for its provider. */
  const repoUses = (n: TreeNode) => (n.provider && PARENT.get(n.id)?.id === 'agents' ? mergedRows(['repos']).filter((r) => r.agent === n.provider).length : 0)
  /**
   * The other providers, folded: opened by a click, or as the page opens on one of them (a link
   * to it, a result naming one of its keys); a click folds them again all the same.
   */
  let othersOpen = $state(false)
  $effect.pre(() => {
    othersOpen = otherProviders.some((p) => p === target || (!!settingsView.focus && sectionsOf(p).some((s) => focusIn(s))))
  })

  /** The keys a section leaves to the page: the feature switch (header), the provider's choice (top). */
  const hideOf = (s: Section): string[] => {
    if (!page) return []
    const out = page.toggle === s.id ? ['enabled'] : []
    if (page.choice && s.spec && s.spec.path.join('.') === page.choice.path.slice(0, -1).join('.')) out.push(page.choice.path.at(-1)!)
    return out
  }
  /** The page's anchors, when it has several sections: its own, then the chosen provider's. */
  const anchors = $derived(page?.tabs ? [] : [...shownSections, ...(chosenProvider ? sectionsOf(chosenProvider) : [])])
  const anchorTo = (id: string) => document.getElementById(`sec-${id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' })
  // A link to a section, or to a provider, lower on its page scrolls to it.
  $effect(() => {
    const id = settingsView.section
    if (!page || page.tabs || settingsView.focus || id === page.id || id === pageSections[0]?.id) return
    const to = target && isProvider(target) ? (sectionsOf(target)[0]?.id ?? id) : id
    requestAnimationFrame(() => document.getElementById(`sec-${to}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' }))
  })
  /**
   * A page opened on a key: scrolled to it once its fold is open and its row drawn — config.toml
   * may still be read when Settings first opens, so it is looked for a few seconds. The key stays
   * marked until another page is shown.
   */
  $effect(() => {
    const key = settingsView.focus
    const sec = settingsView.focusIn
    if (!key) return
    const find = () => (sec ? document.querySelector(`#sec-${CSS.escape(sec)} [data-key="${CSS.escape(key)}"]`) : null) ?? document.querySelector(`[data-key="${CSS.escape(key)}"]`)
    let tries = 0
    const timer = setInterval(() => {
      const el = find()
      if (el || ++tries > 40) clearInterval(timer)
      el?.scrollIntoView({ block: 'center', behavior: 'smooth' })
    }, 75)
    return () => clearInterval(timer)
  })
  /** The key the page was opened on is one of this section's. */
  const focusIn = (s: Section) => {
    const base = pathOf(s)
    return !!base && (!settingsView.focusIn || settingsView.focusIn === s.id) && fieldsOf(s).some((f) => [...base, f.key].join('.') === settingsView.focus)
  }

  /* ---------- Switches and states ---------- */
  /** The feature switch of a node: its section's `enabled` key. */
  const switchPath = (n: TreeNode) => {
    const spec = n.toggle ? SECTION.get(n.toggle)?.spec : undefined
    return spec ? [...spec.path, 'enabled'] : null
  }
  const isOn = (n: TreeNode) => {
    const p = switchPath(n)
    return p && cfg.file ? !!valueOf(p) : undefined
  }
  const flip = (n: TreeNode) => {
    const p = switchPath(n)
    if (p) setValue(p, !valueOf(p))
  }
  const pageSwitch = $derived(page ? switchPath(page) : null)
  const pageOn = $derived(!!pageSwitch && !!valueOf(pageSwitch))
  const switchField = (n: TreeNode) => SECTION.get(n.toggle ?? '')?.spec?.fields.find((f) => f.key === 'enabled')
  /** A page and its providers' sections. */
  const nodeSections = (n: TreeNode) => descendants(n).flatMap(sectionsOf)
  const nodeDirty = (n: TreeNode) => nodeSections(n).some(dirty)
  const nodeAttention = (n: TreeNode) => nodeSections(n).some(attention)
  const inactive = (n: TreeNode) => isOn(n) === false
  /** A domain's pages with a switch, and those on. */
  const switches = (root: TreeNode) => (root.children ?? []).filter((n) => switchPath(n))
  /** A section's key prefix: its table, or its `[[tables]]` key. */
  const prefix = (s: Section) => (s.spec ? (s.spec.path.length ? s.spec.path.join('.') : (s.spec.fields[0]?.key ?? s.id)) : s.id)
  const dirty = (s: Section) => changes().some((k) => (s.spec?.keys ? s.spec.keys.includes(k) : k === prefix(s) || k.startsWith(`${prefix(s)}.`)))
  /** Live-checks every checkable value, on open then on each change. */
  $effect(() => {
    if (!cfg.file) return
    void runs.n
    for (const spec of SCHEMA) for (const r of sectionRequests(spec)) ensure(r)
  })
  /** Needs attention: a check failed, or the section's collector is in error. */
  const attention = (s: Section) => !!s.spec && (failing(s.spec) || !!(s.spec.source && live.snapshot?.errors[s.spec.source]))
  /** A pack's blocks whose settings changed while they do not reload: they wait for a restart. */
  const waiting = $derived(live.snapshot?.waiting_restart ?? [])

  /* ---------- Search ---------- */
  /**
   * The search lists keys (settings-search.ts), each changed in place; ↵ opens the page on the
   * one chosen. The filters keep the keys changed, written in config.toml or given by a pack, or
   * the pages on or needing attention; each says how many (SettingsSearch.svelte).
   */
  const search = new SearchState({ isOn, nodeAttention, attention, go, stateOf: (n) => stateOf(n) })
  const searching = $derived(search.searching)
  const visiblePage = (n: TreeNode) => !searching || search.listedPages.has(n.id)

  /**
   * Three columns, as the system's own Settings: the domains (rail), the pages of one (list), the
   * page. The list shows the shown page's domain, else the last one shown (remembered); while
   * searching, the pages answering, from every domain, each naming its own.
   */
  let lastRoot = $state(pref<string>('settings.domain', 'general'))
  const pageRoot = $derived(page ? chainOf(page)[0] : undefined)
  $effect(() => {
    const id = pageRoot?.id
    if (id && id !== untrack(() => lastRoot)) {
      lastRoot = id
      setPref('settings.domain', id)
    }
  })
  const listRoot = $derived(pageRoot ?? TREE.find((r) => r.id === lastRoot) ?? TREE[0])
  /** A page's providers, listed under it (the chosen one marked). */
  const providersOf = (n: TreeNode) => n.children?.filter(isProvider) ?? []
  /** A page's state, one mark everywhere (StateMark.svelte). */
  const stateOf = (n: TreeNode) => pageState(nodeDirty(n), nodeAttention(n), isOn(n))
  const sectionState = (s: Section) => pageState(dirty(s), attention(s), undefined)

  /**
   * Keyboard: ↑ ↓ move within the rail or the list, showing what they reach (in the list while
   * searching, only the focus moves: ↵ opens); → goes from the rail to the list, ← back.
   */
  function onNavKey(e: KeyboardEvent, col: 'rail' | 'list') {
    const box = e.currentTarget as HTMLElement
    const rows = [...box.querySelectorAll<HTMLButtonElement>('button[data-go]')]
    const i = rows.indexOf(document.activeElement as HTMLButtonElement)
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const next = rows[i < 0 ? 0 : (i + (e.key === 'ArrowDown' ? 1 : rows.length - 1)) % rows.length]
      next?.focus()
      if (next && !(col === 'list' && searching)) go(next.dataset.go!)
    } else if (e.key === 'ArrowRight' && col === 'rail') {
      e.preventDefault()
      const list = document.querySelector<HTMLElement>('.settings .pages')
      ;(list?.querySelector<HTMLButtonElement>('button[data-go].on') ?? list?.querySelector<HTMLButtonElement>('button[data-go]'))?.focus()
    } else if (e.key === 'ArrowLeft' && col === 'list') {
      e.preventDefault()
      const rail = document.querySelector<HTMLElement>('.settings .rail')
      ;(rail?.querySelector<HTMLButtonElement>('button[data-go].on') ?? rail?.querySelector<HTMLButtonElement>('button[data-go]'))?.focus()
    }
  }
  /** `[cleanup] hide_off`: a feature turned off leaves the tree, unless searched for or shown. */
  const hideOff = $derived(!!valueOf(['cleanup', 'hide_off'], false))
  const offHidden = (n: TreeNode) => hideOff && !searching && inactive(n) && n !== page

  /** The TOML panel: closed at first (each key says its table and origin), remembered once opened. */
  let showToml = $state(pref<boolean>('settings.toml', false))
  function toggleToml() {
    showToml = !showToml
    setPref('settings.toml', showToml)
  }
  /** What the TOML panel shows (TomlPanel.svelte): config.toml, a pack's file, or the two merged. */
  let tomlView = $state(pref<string>('settings.tomlView', 'file'))

  /** The save bar's list of changes, key by key: before → after. */
  let diffOpen = $state(false)
  const diff = $derived(changes().map((k) => ({ key: k, path: k.split('.'), before: savedValue(k.split('.')), after: cfg.edits[k] })))

  /* ---------- Screensaver ---------- */
  const saverOn = $derived(valueOf(['appearance', 'screensaver'], true))
  const saverMode = $derived(valueOf(['appearance', 'screensaver_mode'], 'spectrum'))

  /* ---------- Console ---------- */
  const DEFAULT_KEY = '`'
  const key = $derived(valueOf(['console', 'key'], DEFAULT_KEY))
  const keyHint = $derived.by(() => {
    const sc = parseShortcut(key)
    if (/^[a-z0-9]$/i.test(key)) return { tone: 'warn', text: t('{key} alone is also typed in fields, and may clash with a grid shortcut (j / k move the cursor).', { key }) }
    if (worksWhileTyping(sc)) return { tone: 'ok', text: WHILE_TYPING }
    if (key === '`') return { tone: '', text: keys(t('On an AZERTY keyboard, ` is a dead key: Backquote (the key left of 1) or Cmd+K is a safer pick.')) }
    return { tone: '', text: t('Works when no field has the focus.') }
  })
  /** With a modifier the shortcut also works in a field; `Cmd` is Ctrl off macOS. */
  const WHILE_TYPING = isMac ? t('With ⌘ or ⌃, it also works while typing in a field.') : t('With Ctrl or Super, it also works while typing in a field.')
  const PRESETS = ['`', 'Backquote', '²', 'Cmd+K', 'Ctrl+Space', 'F1']

  /* ---------- Notes ---------- */
  const notesKey = $derived(valueOf(['notes', 'key'], DEFAULT_NOTES_KEY))
  const aliases = $derived(Object.entries(valueOf<Record<string, string>>(['console', 'aliases'], {})))
  function setAliases(list: [string, string][]) {
    setValue(['console', 'aliases'], Object.fromEntries(list.filter(([a]) => a.trim())), {})
  }
  let newAlias = $state('')
  let newCommand = $state('')
  function addAlias() {
    if (!newAlias.trim() || !newCommand.trim()) return
    setAliases([...aliases, [newAlias.trim(), newCommand.trim()]])
    newAlias = newCommand = ''
  }

  /* ---------- Appearance ---------- */
  /** One switch per feature, in config.example.toml order. */
  /** Offered fonts, those installed on this computer first (the others remain possible). */
  const fontChoices = (list: string[]) => {
    const found = list.filter(installed)
    return [...found.map((f) => ({ name: f, here: true })), ...list.filter((f) => !found.includes(f)).map((f) => ({ name: f, here: false }))]
  }
  const sansChoices = fontChoices(SANS_FONTS)
  const monoChoices = fontChoices(MONO_FONTS)
  const FONTS = [
    { key: 'font', label: t('Font'), hint: t('Text of the app. Empty: Inter, bundled in the app.'), placeholder: 'Inter', choices: sansChoices, stack: sansStack, sample: t('Sprint 43 · 4 tickets ready to dev, 2 MRs waiting for review') },
    { key: 'font_mono', label: t('Mono font'), hint: t('IDs, branches, code and the agents terminals (unless Terminal font). Empty: Recursive Mono, built in.'), placeholder: 'Recursive Mono', choices: monoChoices, stack: monoStack, sample: 'ABC-4537 · feat/abc-4537-visit-date !1793 → 0O1lI|' },
    { key: 'font_terminal', label: t('Terminal font'), hint: t('The agents terminals and their shells only. Empty: RecMonoCasual Nerd Font Mono, built in, whose icons draw your prompt.'), placeholder: 'RecMonoCasual Nerd Font Mono', choices: monoChoices, stack: (name: string) => terminalStack(name, valueOf<string>(['appearance', 'font_mono'], '')), sample: '~/prism on  agent/shell ❯ pnpm dev   ✓ 0O1lI|' },
  ] as const

  const localeTag = $derived(valueOf<string>(['appearance', 'locale'], '').trim())
  /** The tag as typed, if the webview knows it: its sample shows before it is saved. */
  const localeShown = $derived.by(() => {
    try {
      return localeTag ? Intl.getCanonicalLocales(localeTag)[0] : undefined
    } catch {
      return null
    }
  })
  const LOCALES = ['fr-FR', 'en-GB', 'en-US', 'de-DE', 'es-ES', 'it-IT']

  const ignoreReduce = $derived(!!valueOf(['appearance', 'ignore_reduce'], false))
  const tipsOn = $derived(!!valueOf(['appearance', 'tooltips'], true))
  const threadsOn = $derived(!!valueOf(['appearance', 'ticket_threads'], false))
  const swipeOn = $derived(!!valueOf(['appearance', 'page_swipe'], true))
  const pinsOn = $derived(!!valueOf(['appearance', 'pin_drawers'], false))
  const previewOn = $derived(!!valueOf(['appearance', 'theme_preview'], true))
  const FEATURES: { id: Feature; label: string; hint: string }[] = [
    { id: 'boot', label: t('Boot screen'), hint: t('At every launch, while the sources answer. Shown even when the system asks to reduce motion.') },
    { id: 'animations', label: t('Animations'), hint: t('Refresh icon, an agent waiting or working, running stage, rolling counters, a dragged block that tilts.') },
    { id: 'theme_motion', label: t('Theme motion'), hint: t('Synthwave: a sun that breathes over a rolling grid, stars, a flowing rail, chrome shine, rows that come in on a wipe. Hacker: a blinking cursor, a CRT sweep, phosphor, a faint rain, an ASCII spinner, rows typed in, alerts that glitch.') },
    { id: 'celebrations', label: t('Celebrations'), hint: t('Confetti when my MR is merged, my pipeline is green again or my day’s hours are logged, fireworks when the sprint is done, a shining shield, a sprout.') },
    { id: 'easter_eggs', label: t('Easter eggs'), hint: keys(t('Seven clicks on the logo; hold ⌥ to see the id and source of every block.')) },
    { id: 'friday_guard', label: t('Friday guard'), hint: t('A production job on Friday after 16:00 asks once more.') },
  ]

  /* ---------- Packs ---------- */
  let packSource = $state('')
  /** A repository pack's ref being typed, by pack: Update reads it there. */
  let refs = $state<Record<string, string>>({})
  /** A local pack file, from the system's picker, in the source field (Import then reads it). */
  async function choosePack() {
    const p = await pickPath(packSource.startsWith('glab:') || packSource.startsWith('gh:') ? '' : packSource, { file: true, extensions: ['toml'] })
    if (p) packSource = p
  }
  async function addPack() {
    if (!packSource.trim()) return
    if (await importPack(packSource.trim())) packSource = ''
  }
  /** Remove: a first click arms the button, a second one within 3 s removes. */
  const armed = armer()
  /** The status bar's hidden items when the file sets none: the default bar's, unless an order is set. */
  const hiddenByDefault = () => (valueOf<string[]>(['appearance', 'statusbar'], []).length ? [] : DEFAULT_HIDDEN)
  function askRemove(name: string) {
    if (armed.confirm(`pack:${name}`)) void removePack(name)
  }

  /** Install restarts Prism: a second click within 3 s. */
  function askInstall() {
    if (armed.confirm('install')) void installUpdate()
  }
  /** Opening the page reads the releases, once (the periodic check reads nothing with its switch off). */
  let updatesRead = false
  $effect(() => {
    if (page?.id !== 'updates' || updatesRead) return
    updatesRead = true
    void checkUpdates()
  })
  /** The same for the packs' sources, on opening Packs. */
  let packsRead = false
  $effect(() => {
    if (page?.id !== 'packs' || packsRead) return
    packsRead = true
    void checkPacks()
  })
  /** Updated, a pack is read again: its item leaves the status bar. */
  async function updatePackThen(name: string, reference: string | null) {
    if (await updatePack(name, reference)) void checkPacks()
  }

  async function openFile(with_: 'finder' | 'editor') {
    try {
      await invoke('open_config', { with: with_ })
    } catch (e) {
      cfg.error = String(e)
    }
  }

  /** The editor on screen: the TOML panel open on config.toml; the form inert meanwhile. */
  const rawShown = $derived(raw.on && showToml && tomlModeOf(tomlView) === 'file')

  /**
   * ⌘S saves; / (out of a field) or ⌘F puts the cursor in the search — ⌘F but in a text area,
   * whose own search it leaves alone.
   */
  function onkeydown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey
    if (mod && e.key === 's') {
      e.preventDefault()
      void (rawShown ? saveRaw() : saveConfig())
    } else if ((mod && e.key === 'f' && !inTextArea(e)) || (e.key === '/' && !mod && !typingIn(e))) {
      e.preventDefault()
      search.focus()
    }
  }
  const inTextArea = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement | null
    return !!el && (el.isContentEditable || el.tagName === 'TEXTAREA')
  }
  const typingIn = (e: KeyboardEvent) => {
    const el = e.target as HTMLElement | null
    return !!el && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName))
  }
  /** "Saved" for a few seconds after saving. */
  let saved = $state(false)
  $effect(() => {
    if (!cfg.savedAt) return
    saved = true
    const timer = setTimeout(() => (saved = false), 3000)
    return () => clearTimeout(timer)
  })
</script>

<svelte:window {onkeydown} />

<div class="settings layer-in"><div class="layout" class:withtoml={showToml}>
  <!-- The domains, each its icon and how many of its features are on. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <nav class="rail" aria-label={t('Settings domains')} onkeydown={(e) => onNavKey(e, 'rail')}>
    <button class="rd" class:on={!page && !searching} data-go="overview" onclick={() => go('overview')} title={t('Overview')}><span class="ico ov"><Icon name="overview" /></span><span class="name">{t('Overview')}</span></button>
    {#each TREE as root (root.id)}
      {@const sw = switches(root)}
      <button class="rd" class:on={root === pageRoot && !searching} data-go={root.id} onclick={() => go(root.id)} title={root.label}>
        <span class="ico" style:background={root.color}>{#if root.icon}<Icon name={root.icon} />{/if}</span>
        <span class="name">{root.label}</span>
        {#if sw.length}<span class="cnt" title={t('Features on')}>{sw.filter((n) => isOn(n)).length}/{sw.length}</span>{/if}
      </button>
    {/each}
    <span class="ver" title="{t('Prism version, and the commit it was built from')}{COMMIT.endsWith('+') ? t(' (+: with uncommitted changes)') : ''}">Prism {versionLabel()}</span>
  </nav>

  <!-- The pages of a domain: groups apart, each page's providers under it; or the pages found. -->
  <aside class="pages">
    <SettingsSearch {search} part="box" />
    {#snippet row(n: TreeNode, where: TreeNode | null)}
      {@const provs = providersOf(n)}
      {@const pick = chosen(n)}
      <button class="pg" class:on={n === page && target === page} class:within={n === page && target !== page} class:off={inactive(n)} data-go={n.id} onclick={() => go(n.id)} title={sectionsOf(n).map((s) => s.table).join(' ')}>
        <span class="name">{n.label}{#if where}<span class="where"> · {where.label}</span>{/if}</span>
        <StateMark state={stateOf(n)} />
      </button>
      {#each provs as p (p.id)}
        <button class="pg prov" class:on={p === target} class:unused={p.provider !== pick} data-go={p.id} onclick={() => go(p.id)}>
          <span class="sw" style:background={p.color ?? 'var(--text-muted)'}></span>
          <span class="name">{p.label}</span>
          {#if p.provider === pick}<span class="inuse">{tc('settings', 'in use')}</span>{/if}
        </button>
      {/each}
    {/snippet}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <nav class="list" aria-label={searching ? t('Pages found') : listRoot.label} onkeydown={(e) => onNavKey(e, 'list')}>
      {#if searching}
        {#each TREE as root (root.id)}
          {#each (root.children ?? []).filter((n) => visiblePage(n)) as n (n.id)}{@render row(n, root)}{/each}
        {/each}
        {#if !search.listedPages.size}<p class="hint none">{t('Nothing matches.')}</p>{/if}
      {:else}
        <button class="lhead" onclick={() => go(listRoot.id)}><span class="ico" style:background={listRoot.color}>{#if listRoot.icon}<Icon name={listRoot.icon} />{/if}</span><b>{listRoot.label}</b></button>
        {#each groupsOf(listRoot) as g, gi (g.id)}
          {@const shownPages = g.pages.filter((n) => !offHidden(n))}
          {#if shownPages.length}
            {#if gi}<hr />{/if}
            {#if g.label}<span class="grp">{g.label}</span>{/if}
            {#each shownPages as n (n.id)}{@render row(n, null)}{/each}
          {/if}
        {/each}
      {/if}
    </nav>
  </aside>

  <section class="body" inert={rawShown}>
    {#if !editable()}
      <p class="note warn">{t('Settings are edited from the app: in the browser preview they are read-only.')}</p>
    {:else if cfg.loading && !cfg.file}
      <p class="note">{t('Reading config.toml…')}</p>
    {/if}
    {#if cfg.error}<p class="note err">{cfg.error}</p>{/if}

    {#snippet body(s: Section)}
    {#if s.id === 'profiles'}
      <ProfilesSettings />
    {:else if s.id === 'packs'}
      <p class="intro">{t('A pack is a team’s configuration (repositories, ClickUp lists and statuses, environments, skill plugin…), a TOML file shaped like config.toml. Packs are read first, in order, then config.toml on top: what you set here overrides them. [open_with] is never taken from a pack.')}</p>
      <div class="card">
        {#each cfg.file?.packs ?? [] as p (p.name)}
          {@const pc = packCheck(p.name)}
          <div class="pack">
            <div class="pinfo">
              <b>{p.name}{#if p.version}<span class="ver mono">v{p.version}</span>{/if}</b>
              {#if p.missing}<span class="hint warn"><Icon name="warning" /> {t('Missing: listed in config.toml but not in packs/. Import it again.')}</span>{/if}
              {#if p.too_new}<span class="hint warn"><Icon name="warning" /> {t('Needs Prism {version} or later: update Prism, some of its settings may not apply.', { version: p.requires ?? '' })}</span>{:else if p.requires}<small>{t('Needs Prism {version} or later.', { version: p.requires })}</small>{/if}
              {#if p.description}<small>{p.description}</small>{/if}
              {#if pc?.error}<span class="hint warn"><Icon name="warning" /> {t('Its source does not read: {error}', { error: pc.error })}</span>
              {:else if pc?.changed && pc.too_new}<span class="hint warn"><Icon name="warning" /> {pc.latest ? t('Changed at its source ({latest}), but needs Prism {version} or later: update Prism first.', { latest: pc.latest, version: pc.requires ?? '' }) : t('Changed at its source, but needs Prism {version} or later: update Prism first.', { version: pc.requires ?? '' })}</span>
              {:else if pc?.changed}<span class="hint new"><Icon name="up" /> {t('Changed at its source')}{#if pc.latest && pc.latest !== p.version}: v{pc.latest}{/if}{#if pc.commit}{' '}· {pc.commit.slice(0, 8)}{/if}. {t('Update to apply it.')}</span>
              {:else if pc}<small>{t('Up to date with its source.')}</small>{/if}
              {#if p.modules.length}<small>{t('Brings the modules {names}: Modules, below.', { names: p.modules.map((m) => m.id).join(', ') })}</small>{/if}
              {#each pc?.modules.filter((m) => m.changed || m.error) ?? [] as m (m.id)}
                {#if m.error}<span class="hint warn"><Icon name="warning" /> {t('Its module {id} does not read at its source: {error}', { id: m.id, error: m.error })}</span>
                {:else}<span class="hint new"><Icon name="up" /> {m.installed ? t('Its module {id} changed at its source{latest}: update the pack, then trust it again.', { id: m.id, latest: m.latest && m.latest !== m.version ? ` (v${m.version} → v${m.latest})` : '' }) : t('Its module {id} is not installed: update the pack.', { id: m.id })}</span>{/if}
              {/each}
              {#if p.source}<span class="mono src" title={p.source}>{p.source}{#if p.commit}{' '}· <span title={t('Read at commit {commit}', { commit: p.commit })}>{p.commit.slice(0, 8)}</span>{/if}</span>{/if}
            </div>
            <span class="row">
              {#if p.source && packRef(p.source) !== null}
                {@const current = packRef(p.source) ?? ''}
                <input class="t mono ref" value={refs[p.name] ?? current} oninput={(e) => (refs[p.name] = e.currentTarget.value.trim())} onkeydown={(e) => e.key === 'Enter' && updatePackThen(p.name, refs[p.name] ?? current)} placeholder={t('default branch')} spellcheck="false" aria-label={t('Branch, tag or commit of {name}', { name: p.name })} title={t('The branch, tag or commit the pack is read at: Update reads it there from now on (empty: the default branch)')} />
              {/if}
              {#if p.source}<button class="btn sm" class:primary={!!pc?.changed && !pc.too_new} onclick={() => updatePackThen(p.name, packRef(p.source) !== null ? (refs[p.name] ?? packRef(p.source)) : null)} class:busy={packBusy.name === p.name} disabled={!!packBusy.name || !editable()} title={refs[p.name] !== undefined && refs[p.name] !== packRef(p.source) ? t('Import it again from {source}, at {ref}', { source: p.source, ref: refs[p.name] || t('the default branch') }) : t('Import it again from {source}', { source: p.source })}>{#if packBusy.name === p.name}<span class="spinner"></span>{t('Updating…')}{:else}{t('Update')}{/if}</button>{/if}
              <button class="btn sm" class:armed={armed.is(`pack:${p.name}`)} onclick={() => askRemove(p.name)} disabled={!!packBusy.name || !editable()} title={t('Take it out of config.toml and packs/ (click twice)')}><ArmedLabel label={t('Remove')} armedLabel={t('Click again to confirm')} armed={armed.is(`pack:${p.name}`)} /></button>
            </span>
          </div>
        {:else}
          <p class="hint pad">{t('No pack: everything comes from config.toml and the defaults.')}</p>
        {/each}
      </div>
      {#if cfg.file?.packs?.some((p) => p.source)}
        <div class="checkrow">
          {#if packUpdates.error}<span class="hint warn"><Icon name="warning" /> {packUpdates.error}</span>{/if}
          <span class="grow"></span>
          <button class="btn sm" class:busy={packUpdates.checking} onclick={() => checkPacks()} disabled={packUpdates.checking || !!packBusy.name} title={t('Read the packs’ sources again ([updates] packs: at launch and every 6 hours)')}>{#if packUpdates.checking}<span class="spinner"></span>{t('Checking…')}{:else}{t('Check now')}{/if}</button>
        </div>
      {/if}
      <ModulesSettings />
      <div class="card">
        <div class="ch"><b>{t('Import a pack')}</b><small>{t('copied into packs/ next to config.toml, and added to packs')}</small></div>
        <div class="filerow">
          <input class="t mono grow" bind:value={packSource} placeholder={t('~/team/pack.toml, glab:group/project:path/pack.toml@main or gh:owner/repo:path/pack.toml')} spellcheck="false" onkeydown={(e) => e.key === 'Enter' && addPack()} aria-label={t('Pack source')} />
          {#if canPick()}<button class="btn" onclick={choosePack} disabled={!!packBusy.name || !editable()} title={t('Choose a pack file')}><Icon name="folder" />{t('Choose…')}</button>{/if}
          <button class="btn primary" class:busy={packBusy.name === '+import'} onclick={addPack} disabled={!packSource.trim() || !!packBusy.name || !editable()}>{#if packBusy.name === '+import'}<span class="spinner"></span>{t('Importing…')}{:else}{t('Import')}{/if}</button>
        </div>
        <p class="hint pad">{t('A local file path, or a file of a repository: on GitLab with glab,')} <code>glab:group/project:path/pack.toml@ref</code>{t('; on GitHub with gh,')} <code>gh:owner/repo:path/pack.toml@ref</code>{t(', the ref a branch, a tag or a commit (without')} <code>@ref</code>{t(': the default branch). Its source is kept, so Update reads it again; the field next to it changes the ref, to try a commit before it reaches main.')}</p>
      </div>
    {:else if s.id === 'appearance'}
      <div class="card">
        <Field label={t('Theme')} hint={t('Or follow the system (Paper or Graphite). The Theme button of the rail saves it here at once.')} path={['appearance', 'theme']}>
          <select class="t" value={valueOf(['appearance', 'theme'], 'system')} onchange={(e) => setValue(['appearance', 'theme'], e.currentTarget.value, 'system')}>
            <option value="system">{t('System')}</option>
            {#each THEMES as th (th.id)}<option value={th.id}>{th.group ? `${th.group} · ` : ''}{th.label}</option>{/each}
          </select>
        </Field>
        <Field label={t('Theme preview')} hint={t('Hovering a theme in the rail’s Theme menu shows it on the whole window; leaving the menu puts the chosen one back.')} path={['appearance', 'theme_preview']}>
          <span class="row">
            <button class="toggle" class:on={previewOn} role="switch" aria-checked={previewOn} aria-label={t('Theme preview')} onclick={() => setValue(['appearance', 'theme_preview'], !previewOn, true)}><i></i></button>
            <span class="hint">{previewOn ? t('On') : t('Off')}</span>
          </span>
        </Field>
        <Field label={t('Density')} hint={t('Room around the content, whatever the theme: compact as drawn, or comfortable (larger text, airier rows).')} path={['appearance', 'density']}>
          <select class="t" value={valueOf(['appearance', 'density'], 'compact')} onchange={(e) => setValue(['appearance', 'density'], e.currentTarget.value, 'compact')}>
            <option value="compact">{tc('settings', 'Compact')}</option>
            <option value="comfortable">{t('Comfortable')}</option>
          </select>
        </Field>
        <Field label={tc('settings', 'Zoom')} hint={keys(t('The whole window, as a browser’s: text, icons, pictures and terminals, saved at once. ⌘+ / ⌘- / ⌘0 everywhere, or the View menu; the agents’ own text size is ⌘⌥+ / ⌘⌥- / ⌘⌥0.'))} path={['appearance', 'zoom']}>
          {@const z = currentZoom()}
          <span class="row">
            <select class="t" value={ZOOMS.includes(z) ? String(z) : ''} onchange={(e) => void setZoom(Number(e.currentTarget.value))} aria-label={tc('settings', 'Zoom')}>
              {#if !ZOOMS.includes(z)}<option value="">{zoomLabel(z)}</option>{/if}
              {#each ZOOMS as s (s)}<option value={String(s)}>{zoomLabel(s)}</option>{/each}
            </select>
            {#if z !== 1}<button class="btn sm ghost" onclick={() => void setZoom(1)}>{t('Reset')}</button>{/if}
          </span>
        </Field>
        {#each FONTS as f (f.key)}
          {@const name = valueOf(['appearance', f.key], '') as string}
          <Field label={f.label} hint={f.hint} path={['appearance', f.key]}>
            <input class="t" {@attach suggest(() => f.choices.map((c) => ({ value: c.name, label: c.here ? t('installed') : t('not installed') })))} value={name} placeholder={f.placeholder} spellcheck="false" onchange={(e) => { const v = e.currentTarget.value.trim(); void ensureFont(v); setValue(['appearance', f.key], v, '') }} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
            {#if name && !installed(name)}<span class="hint warn">{t('Not found on this computer: the default font is used instead.')}</span>{/if}
            <span class="sample" style:font-family={f.stack(name)}>{f.sample}</span>
          </Field>
        {/each}
        <Field label={t('Language')} hint={t('Prism’s words. System: the computer’s language (English when Prism does not speak it). Changing it reloads the window.')} path={['appearance', 'language']}>
          <select class="t" value={valueOf(['appearance', 'language'], '')} onchange={(e) => setValue(['appearance', 'language'], e.currentTarget.value, '')}>
            <option value="">{t('System')}</option>
            {#each LANGUAGES as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
          </select>
        </Field>
        <Field label={t('Dates and numbers')} hint={t('Their format, as a language tag: fr-FR, en-GB, en-US… Empty: the system’s.')} path={['appearance', 'locale']}>
          <input class="t" {@attach suggest(() => LOCALES)} value={localeTag} placeholder={t('System')} spellcheck="false" onchange={(e) => setValue(['appearance', 'locale'], e.currentTarget.value.trim(), '')} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
          {#if localeShown === null}<span class="hint warn">{t('Not a language tag: the system’s format is used instead.')}</span>{/if}
          <span class="sample">{new Date().toLocaleString(localeShown ?? undefined, { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })} · {(1234567.89).toLocaleString(localeShown ?? undefined)}</span>
        </Field>
        {#each FEATURES as f (f.id)}
          {@const on = valueOf(['appearance', f.id], DEFAULT_APPEARANCE[f.id])}
          <Field label={f.label} hint={f.hint} path={['appearance', f.id]}>
            <span class="row">
              <button class="toggle" class:on role="switch" aria-checked={on} aria-label={f.label} onclick={() => setValue(['appearance', f.id], !on, DEFAULT_APPEARANCE[f.id])}><i></i></button>
              <span class="hint">{on ? t('On') : t('Off')}</span>
              {#if f.id === 'boot'}<button class="btn ghost" onclick={replayBoot} title={t('Show the boot screen now (console: boot)')}>{t('Replay now')}</button>{/if}
            </span>
          </Field>
        {/each}
        <Field label={t('Tooltips')} hint={t('Prism’s own, in its colours, with shortcuts shown as keys; otherwise the system’s. The delay before one shows; moving to the next one shows it at once.')} path={['appearance', 'tooltips']}>
          <span class="row">
            <button class="toggle" class:on={tipsOn} role="switch" aria-checked={tipsOn} aria-label={t('Prism tooltips')} onclick={() => setValue(['appearance', 'tooltips'], !tipsOn, true)}><i></i></button>
            <span class="hint">{tipsOn ? t('Prism’s, after') : t('The system’s')}</span>
            {#if tipsOn}
              <input class="t num" type="number" min="0" max="3000" step="50" value={valueOf(['appearance', 'tooltip_delay'], 400)} onchange={(e) => setValue(['appearance', 'tooltip_delay'], Math.min(3000, Math.max(0, Math.round(Number(e.currentTarget.value) || 0))), 400)} aria-label={t('Delay before a tooltip shows')} />
              <span class="hint">ms</span>
            {/if}
          </span>
        </Field>
        <Field label={t('Ticket threads')} hint={t('A ticket row shows its thread next to its id, folded into small squares: worktree, agent, MR, pipeline, environments. Off: the thread only in its drawer (t, or Thread in the ⋯ menu).')} path={['appearance', 'ticket_threads']}>
          <span class="row">
            <button class="toggle" class:on={threadsOn} role="switch" aria-checked={threadsOn} aria-label={t('Ticket threads')} onclick={() => setValue(['appearance', 'ticket_threads'], !threadsOn, false)}><i></i></button>
            <span class="hint">{threadsOn ? t('On the rows') : t('Off')}</span>
          </span>
        </Field>
        <Field label={t('Swipe between pages')} hint={keys(t('On the trackpad, two fingers up or down over the rail (the icons on the left) go to the next or previous page. Off: a click on the rail and ⌘1… only.'))} path={['appearance', 'page_swipe']}>
          <span class="row">
            <button class="toggle" class:on={swipeOn} role="switch" aria-checked={swipeOn} aria-label={t('Swipe between pages')} onclick={() => setValue(['appearance', 'page_swipe'], !swipeOn, true)}><i></i></button>
            <span class="hint">{swipeOn ? t('Two fingers, over the rail') : t('Off')}</span>
          </span>
        </Field>
        <Field label={t('Pin side panes')} hint={t('A pin in the header of the side panes (MR, ticket, logs, shells…): pinned, a click outside no longer closes the pane, only × (or Esc). Remembered per kind of pane. Off: a click outside closes, and ⌘⇧T brings it back.')} path={['appearance', 'pin_drawers']}>
          <span class="row">
            <button class="toggle" class:on={pinsOn} role="switch" aria-checked={pinsOn} aria-label={t('Pin side panes')} onclick={() => setValue(['appearance', 'pin_drawers'], !pinsOn, false)}><i></i></button>
            <span class="hint">{pinsOn ? t('A pin in their header') : t('Off')}</span>
          </span>
        </Field>
        <Field label={t('Status bar order')} hint={t('Its items, from left to right; “|” parts the left side from the right, and an item left out takes its default place. Empty: by zone, what waits for you first, then the work, then the computer and Prism. Customize… arranges them by dragging, on the bar itself.')} path={['appearance', 'statusbar']}>
          <span class="row">
            <ListInput value={valueOf<string[]>(['appearance', 'statusbar'], [])} onchange={(v) => setValue(['appearance', 'statusbar'], v, [])} options={[SIDE_BREAK, ...STATUS_ITEMS.map((i) => i.id)]} placeholder={t('default order')} compact />
            <button class="btn" onclick={() => (statusbarEditing.on = true)}>{t('Customize…')}</button>
          </span>
        </Field>
        <Field label={t('Hidden in the status bar')} hint={t('These items never show (a right-click on the bar hides or shows them too).')} path={['appearance', 'statusbar_hidden']}>
          <ListInput value={valueOf<string[]>(['appearance', 'statusbar_hidden'], hiddenByDefault())} onchange={(v) => setValue(['appearance', 'statusbar_hidden'], v, hiddenByDefault())} options={STATUS_ITEMS.map((i) => i.id)} compact />
        </Field>
        <Field label={t('Screensaver')} hint={t('Starts by itself after this long without a key, a click or the mouse, while the window shows. Off: only on demand (console: screensaver). ← / → switch between them, a click or another key wakes.')} path={['appearance', 'screensaver']}>
          <span class="row">
            <button class="toggle" class:on={saverOn} role="switch" aria-checked={saverOn} aria-label={t('Screensaver')} onclick={() => setValue(['appearance', 'screensaver'], !saverOn, true)}><i></i></button>
            <span class="hint">{saverOn ? t('After') : t('Off')}</span>
            {#if saverOn}
              <input class="t num" type="number" min="5" max="7200" step="5" value={valueOf(['appearance', 'screensaver_seconds'], 600)} onchange={(e) => setValue(['appearance', 'screensaver_seconds'], Math.min(7200, Math.max(5, Math.round(Number(e.currentTarget.value) || 600))), 600)} aria-label={t('Seconds before it starts')} />
              <span class="hint">s</span>
            {/if}
          </span>
        </Field>
        <Field label={t('Screensaver shown')} hint={t('The one that starts by itself, and with the console command screensaver alone.')} path={['appearance', 'screensaver_mode']}>
          <span class="row">
            <select class="t" value={saverMode} onchange={(e) => setValue(['appearance', 'screensaver_mode'], e.currentTarget.value, 'spectrum')}>
              {#each SAVER_MODES as m (m)}<option value={m}>{m}</option>{/each}
              <option value="random">random</option>
            </select>
            <button class="btn ghost" onclick={() => startSaver(saverMode as SaverMode | 'random')} title={t('Show it now (console: screensaver)')}>{t('Try now')}</button>
          </span>
        </Field>
      </div>
      <div class="card">
        <Field label={t('Ignore Reduce motion and transparency')} hint={t('macOS’s accessibility settings (Display): followed, animations, celebrations, the boot screen’s motion, a theme’s glass and overlays stay off while the system asks for less; ignored, Prism shows them anyway.')} path={['appearance', 'ignore_reduce']}>
          <span class="row">
            <button class="toggle" class:on={ignoreReduce} role="switch" aria-checked={ignoreReduce} aria-label={t('Ignore Reduce motion and transparency')} onclick={() => setValue(['appearance', 'ignore_reduce'], !ignoreReduce, false)}><i></i></button>
            <span class="hint">{ignoreReduce ? t('Ignored') : t('Followed')}{system.reduceMotion || system.reduceTransparency ? ` · ${system.reduceMotion && system.reduceTransparency ? t('the system asks to reduce motion and transparency') : system.reduceMotion ? t('the system asks to reduce motion') : t('the system asks to reduce transparency')}` : ''}</span>
          </span>
        </Field>
      </div>
    {:else if s.id === 'notes'}
      <div class="card">
        <Field label={t('Shortcut')} hint={t('Opens the quick notes over any view: Enter saves and keeps it open for the next one, Esc closes.')} path={['notes', 'key']}>
          <ShortcutRecorder value={notesKey} fallback={DEFAULT_NOTES_KEY} onchange={(v) => setValue(['notes', 'key'], v, DEFAULT_NOTES_KEY)} />
          <span class="hint {worksWhileTyping(parseShortcut(notesKey)) ? 'ok' : ''}">{worksWhileTyping(parseShortcut(notesKey)) ? WHILE_TYPING : t('Works when no field has the focus.')}</span>
        </Field>
      </div>
    {:else if s.id === 'console'}
      <div class="card">
        <Field label={t('Shortcut')} hint={t('Drops the console from the top of the window. A character, a key name, or a combination with {mods}.', { mods: isMac ? '⌘ / ⌃ / ⌥' : 'Ctrl / Alt / Super' })} path={['console', 'key']}>
          <ShortcutRecorder value={key} fallback={DEFAULT_KEY} onchange={(v) => setValue(['console', 'key'], v, DEFAULT_KEY)} />
          <span class="presets"><span class="hint">{t('Try:')}</span>{#each PRESETS as p (p)}<button onclick={() => setValue(['console', 'key'], p, DEFAULT_KEY)}>{keys(p)}</button>{/each}</span>
          <span class="hint {keyHint.tone}">{keyHint.text}</span>
        </Field>
        <Field label={t('History')} hint={t('Commands kept between launches.')} path={['console', 'history']}>
          <span class="row"><input class="t num" type="number" min="0" max="1000" value={valueOf(['console', 'history'], 50)} onchange={(e) => setValue(['console', 'history'], Math.max(0, Math.round(Number(e.currentTarget.value) || 0)), 50)} /><span class="hint">{t('lines')}</span></span>
        </Field>
      </div>
      <div class="card">
        <div class="ch"><b>{t('Aliases')}</b><small>{t('your own commands, expanded before running:')} <code>r !1957</code> {t('runs')} <code>agent review !1957</code></small><span class="grow"></span><span class="path">[console.aliases]</span></div>
        <div class="table">
          {#each aliases as [a, c], i (a)}
            <div class="arow">
              <input class="t mono" value={a} onchange={(e) => setAliases(aliases.map((x, n) => (n === i ? [e.currentTarget.value.trim(), x[1]] : x)))} aria-label={t('Alias')} />
              <input class="t mono" value={c} onchange={(e) => setAliases(aliases.map((x, n) => (n === i ? [x[0], e.currentTarget.value] : x)))} aria-label={t('Command')} />
              <button class="rm" onclick={() => setAliases(aliases.filter((_, n) => n !== i))} title={t('Remove {name}', { name: a })}><Icon name="close" /></button>
            </div>
          {/each}
          <div class="arow">
            <input class="t mono" bind:value={newAlias} placeholder={t('alias')} onkeydown={(e) => e.key === 'Enter' && addAlias()} />
            <input class="t mono" bind:value={newCommand} placeholder={t('command, e.g. goto ci')} onkeydown={(e) => e.key === 'Enter' && addAlias()} />
            <button class="rm add" onclick={addAlias} title={t('Add the alias')} disabled={!newAlias.trim() || !newCommand.trim()}><Icon name="add" /></button>
          </div>
        </div>
      </div>
    {:else if s.id === 'remote-devices'}
      <RemoteDevices />
    {:else if s.id === 'remote-away-checks'}
      <RemoteAway />
    {:else if s.id === 'feedback-requests'}
      <FeedbackRequests />
    {:else if s.id === 'dependencies'}
      <DependenciesPanel />
    {:else if s.id === 'cli-command'}
      <CliCommandPanel />
    {:else if s.id === 'custom-blocks'}
      <CustomBlocksSettings />
    {:else if s.id === 'routines'}
      <RoutinesSettings />
    {:else if s.id === 'mcp-agents'}
      <McpSettings />
    {:else if s.id === 'dictation-panel'}
      <DictationSettings />
    {:else if s.id === 'cleanup-usage'}
      <CleanupUsage onToml={() => !showToml && toggleToml()} />
    {:else if s.id === 'updates-now'}
      {@const u = updates.info}
      <!-- Notes: what an update brings (each release since this Prism's, newest first), or else what this version brought. -->
      {@const shown = u?.available ? u.newer.filter((r) => r.notes.trim()) : u?.installed?.notes.trim() ? [u.installed] : []}
      <div class="card">
        <div class="pack">
          <div class="pinfo">
            <b>Prism {u?.current ?? versionLabel()}{#if u?.latest}<span class="ver mono">{t('newest: {version}', { version: u.latest.version })}</span>{/if}</b>
            {#if !u}<small>{updates.checking ? t('Checking…') : t('Not checked yet.')}</small>
            {:else if !u.source}<small>{t('No source: set one below, or import the pack of your team, which names it.')}</small>
            {:else if !u.latest}<small>{t('No Prism release (prism-v… tag) in {source}.', { source: u.source })}</small>
            {:else if !u.available}<small>{t('Up to date.')}</small>
            {:else if !u.install}<span class="hint warn"><Icon name="warning" /> {t('{version} has no file for this system: see its page.', { version: u.latest.version })}</span>
            {:else}<small>{u.install === 'replace' ? t('Installs {asset} in place of this Prism, then restarts it.', { asset: u.asset ?? '' }) : u.asset?.endsWith('.pkg.tar.zst') ? t('Downloads {asset}, to install with sudo pacman -U.', { asset: u.asset }) : t('Downloads {asset}, to install with sudo apt install.', { asset: u.asset ?? '' })}</small>{/if}
            {#if u?.source}<span class="mono src" title={u.source}>{u.source}</span>{/if}
            {#if updates.error}<span class="hint warn"><Icon name="warning" /> {updates.error}</span>{/if}
            {#if updates.done}<small>{updates.done}</small>{/if}
          </div>
          <span class="row">
            {#if u?.latest?.url}<button class="btn ghost" onclick={() => openLink(u.latest!.url)} title={u.latest.url}>{t('Release page')}</button>{/if}
            <button class="btn" class:busy={updates.checking} onclick={() => checkUpdates()} disabled={updates.checking || updates.installing} title={t('Read the releases of the saved source again')}>{#if updates.checking}<span class="spinner"></span>{t('Checking…')}{:else}{t('Check now')}{/if}</button>
            {#if u?.available && u.install}<button class="btn primary" class:armed={armed.is('install')} class:busy={updates.installing} onclick={askInstall} disabled={updates.installing} title={u.install === 'replace' ? t('Download, replace this Prism and restart it (click twice)') : t('Download the package (click twice)')}>{#if updates.installing}<span class="spinner"></span>{t('Installing…')}{:else}<ArmedLabel label={t('Install {version}', { version: u.latest!.version })} armedLabel={t('Click again to confirm')} armed={armed.is('install')} />{/if}</button>{/if}
          </span>
        </div>
        {#if shown.length}
          <div class="pad notes">
            <small class="ntitle">{u?.available ? t('What the update brings') : t('What {version} brought', { version: u?.installed?.version ?? '' })}</small>
            {#each shown as r (r.tag)}
              <section>{#if u?.available}<b class="mono">{r.version}</b>{/if}<Markdown text={r.notes} /></section>
            {/each}
            {#each shown.flatMap((r) => broughtBy(r.notes)) as q (q.number)}
              <p class="brings"><Icon name="check" /> {u?.available ? t('Brings your request') : t('Brought your request')} <b>#{q.number}</b> {q.title}</p>
            {/each}
          </div>
        {/if}
      </div>
    {:else if s.id === 'file'}
      <div class="card">
        <div class="filerow"><Icon name="logs" /><span class="mono grow">{cfg.file?.path ?? '~/.config/prism/config.toml'}</span>
          {#if isLocal()}<button class="btn" onclick={() => openFile('finder')}>{t('Reveal in {app}', { app: FILE_MANAGER })}</button><button class="btn" onclick={() => openFile('editor')}>{t('Open in editor')}</button>{/if}{#if isDesktop()}<button class="btn" onclick={loadConfig} title={t('Read the file again')}>{t('Reload')}</button>{/if}
        </div>
        <p class="hint pad">{t('Saving rewrites only the keys you changed: comments, order and layout of the file are kept, and the previous version goes to config.toml.bak. A file the app could not read is not saved.')}</p>
        <p class="hint pad">{t('A setting applies as it is saved, here or in the file by hand (an editor, an agent): the sources it concerns are read again, the agents and the phone untouched. Only a pack’s block that does not reload waits for Prism to restart.')}</p>
      </div>
    {:else if s.spec}
      <SectionFields spec={s.spec} hide={hideOf(s)} />
    {/if}
    {/snippet}

    {#if searching}
      <SettingsSearch {search} part="results" {showToml} ontoml={toggleToml} />
    {:else if !page || domain}
      <!-- The overview: every feature by domain, its switch beside it, and what to look at. -->
      <div class="head">
        {#if domain}<div class="crumb"><button onclick={() => go('overview')}>{t('Settings')}</button><span>›</span></div>{/if}
        <h3>{domain ? domain.label : t('Settings')}</h3>
        <span class="grow"></span>
        <button class="btn sm ghost" onclick={toggleToml} aria-pressed={showToml} title={t('Show config.toml next to the settings')}>TOML</button>
      </div>
      {#if !domain}
        <div class="counters">
          {#each [['changed', t('not saved'), t('Changes waiting for ⌘S')], ['file', t('not default'), t('Keys written in config.toml')], ['pack', t('from a pack'), t('Keys a pack gives')], ['attention', t('to check'), t('Pages whose check failed or whose source does not read')]] as const as [f, label, title] (f)}
            <button class="counter {f}" class:hot={search.counts[f] > 0} onclick={() => (search.filter = f)} title={title}><b>{search.counts[f]}</b><span>{label}</span></button>
          {/each}
        </div>
      {/if}
      <div class="domains">
        {#each domain ? [domain] : TREE as root (root.id)}
          {@const sw = switches(root)}
          <div class="dom">
            <div class="dh">
              <span class="ico" style:background={root.color}>{#if root.icon}<Icon name={root.icon} />{/if}</span>
              <button class="dn" onclick={() => go(root.id)}>{root.label}</button>
              <span class="grow"></span>
              {#if sw.length}<span class="cnt" title={t('Features on')}>{sw.filter((n) => isOn(n)).length}/{sw.length}</span>{/if}
            </div>
            {#each groupsOf(root) as g, gi (g.id)}
              {#if gi}<hr />{/if}
              {#if g.label}<span class="grp">{g.label}</span>{/if}
              {#each g.pages as n (n.id)}
                <div class="frow" class:off={inactive(n)}>
                  <StateMark state={stateOf(n)} />
                  <button class="fname" onclick={() => go(n.id)}>{n.label}</button>
                  {#if n.choice}<span class="soft">· {providersOf(n).find((p) => p.provider === chosen(n))?.label ?? chosen(n)}</span>{/if}
                  <span class="grow"></span>
                  {#if switchPath(n)}<button class="toggle" class:on={!!isOn(n)} role="switch" aria-checked={!!isOn(n)} aria-label={n.label} onclick={() => flip(n)}><i></i></button>{/if}
                </div>
              {/each}
            {/each}
          </div>
        {/each}
      </div>
    {:else}
      <div class="crumb"><button onclick={() => go('overview')}>{t('Settings')}</button><span>›</span>{#each chain.slice(0, -1) as c (c.id)}<button onclick={() => go(c.id)}>{c.label}</button><span>›</span>{/each}</div>
      <div class="head">
        <h3>{page.label}</h3>
        {#each tables.slice(0, 3) as tb (tb)}<span class="path">{tb}</span>{/each}
        {#if tables.length > 3}<span class="path" title={tables.slice(3).join(' ')}>+{tables.length - 3}</span>{/if}
        <span class="grow"></span>
        {#if pageSwitch}
          <span class="feat" class:changed={isChanged(pageSwitch)} title={switchField(page)?.hint}>
            {switchField(page)?.label}
            <button class="toggle" class:on={pageOn} role="switch" aria-checked={pageOn} aria-label={switchField(page)?.label} onclick={() => setValue(pageSwitch, !pageOn)}><i></i></button>
          </span>
        {/if}
        {#if pageSections.some((s) => s.spec && sectionRequests(s.spec).length)}<button class="btn sm ghost" onclick={recheck} title={t('Run the checks of the settings again')}>{t('Check again')}</button>{/if}
        {#if editable()}<button class="btn sm ghost" onclick={() => configurePrism({ page: chain.map((c) => c.label).join(' › '), table: tables.join(' ') })} title={t('Ask an agent about this page: it reads Prism’s docs and your config, and proposes the change')}>✦ {t('Ask an agent')}</button>{/if}
        <button class="btn sm ghost" onclick={toggleToml} aria-pressed={showToml} title={t('Show config.toml next to the settings')}>TOML</button>
      </div>

      {#if page.choice && providers.length}
        <!-- The provider, chosen at the top: its keys below the page's, the others folded after. -->
        <div class="choice">
          <span class="clbl">{t('Provider')}</span>
          <div class="seg" role="radiogroup" aria-label={page.choice.noun}>
            {#each providers as p (p.id)}
              <button role="radio" aria-checked={p === chosenProvider} class:on={p === chosenProvider} onclick={() => setValue(page.choice!.path, p.provider, page.choice!.fallback)}>{p.label}{#if repoUses(p)}<small class="uses">{tn(repoUses(p), '{n} repo', '{n} repos')}</small>{/if}</button>
            {/each}
          </div>
          {#if isChanged(page.choice.path)}<span class="hint warn">{t('not saved')}</span>{/if}
        </div>
      {/if}
      {#if anchors.length > 1}
        <nav class="anchors" aria-label={t('Sections of the page')}>
          {#each anchors as s (s.id)}<button onclick={() => anchorTo(s.id)}>{s.label}<StateMark state={sectionState(s)} /></button>{/each}
        </nav>
      {/if}
      {#each shownSections as s, i (s.id)}
        {#if page.tabs && i === 1}
          <div class="tabs" role="tablist">
            {#each pageSections.slice(1) as ts (ts.id)}
              <button role="tab" class:on={ts.id === tab?.id} aria-selected={ts.id === tab?.id} onclick={() => go(ts.id)}>{stageLabel(ts.label)}<span class="path">{ts.table}</span>{#if dirty(ts)}<span class="dot warn" title={t('Unsaved changes')}></span>{/if}</button>
            {/each}
          </div>
        {/if}
        <div class="sec" id="sec-{s.id}">
          {#if (shownSections.length > 1 || chosenProvider) && !(page.tabs && i === 1) && !(i === 0 && s.label === page.label)}<h4 class="sub">{s.label}<span class="path">{s.table}</span></h4>{/if}
          {@render body(s)}
        </div>
      {/each}
      {#if chosenProvider}
        {#each sectionsOf(chosenProvider) as s (s.id)}
          <div class="sec" id="sec-{s.id}">
            <h4 class="sub">{chosenProvider.label}{#if s.label !== chosenProvider.label}<span class="soft">· {s.label}</span>{/if}<span class="path">{s.table}</span><span class="tag on">{tc('settings', 'in use')}</span></h4>
            {@render body(s)}
          </div>
        {/each}
      {/if}
      {#if otherProviders.length}
        <button class="others" class:open={othersOpen} onclick={() => (othersOpen = !othersOpen)} aria-expanded={othersOpen}>
          <Icon name="chevronRight" /><b>{t('Other providers')}</b>
          <span>{otherProviders.map((p) => (repoUses(p) ? `${p.label} (${tn(repoUses(p), '{n} repo', '{n} repos')})` : p.label)).join(', ')}</span>
          <small>{t('not used, editable all the same')}</small>
        </button>
        {#if othersOpen}
          {#each otherProviders as p (p.id)}
            {#each sectionsOf(p) as s (s.id)}
              <div class="sec other" id="sec-{s.id}">
                <h4 class="sub">{p.label}{#if s.label !== p.label}<span class="soft">· {s.label}</span>{/if}<span class="path">{s.table}</span>
                  {#if repoUses(p)}<span class="tag repo">{tn(repoUses(p), '{n} repo', '{n} repos')}</span>{:else}<span class="tag">{t('unused')}</span>{/if}
                  <span class="grow"></span><button class="link" onclick={() => setValue(page.choice!.path, p.provider, page.choice!.fallback)}>{t('Make {name} {noun}', { name: p.label, noun: page.choice!.noun })}</button>
                </h4>
                {@render body(s)}
              </div>
            {/each}
          {/each}
        {/if}
      {/if}
    {/if}
    {#if cfg.file?.error}<p class="note err">{t('config.toml does not load: {error}', { error: cfg.file.error })}</p>{/if}
    {#if cfg.file?.unread?.length}<p class="note warn">{t('Read by nothing in this Prism: {tables}. A module not installed, or a typo.', { tables: cfg.file.unread.map((tb) => `[${tb}]`).join(', ') })}</p>{/if}

    {#if changes().length || saved || waiting.length}
      <div class="savewrap">
        {#if diffOpen && changes().length}
          <div class="diff">
            {#each diff as d (d.key)}
              <div class="drow">
                <span class="mono dkey">{d.key}</span>
                <span class="before mono" title={showValue(d.before)}>{showValue(d.before)}</span>
                <span class="arrow">→</span>
                <span class="after mono" title={d.after === null ? '' : showValue(d.after)}>{d.after === null ? t('removed: the pack’s value or the default') : showValue(d.after)}</span>
                <button class="reset" onclick={() => discardValue(d.path)} title={t('Drop this change')} aria-label={t('Drop this change')}>↺</button>
              </div>
            {/each}
          </div>
        {/if}
        <div class="savebar" class:done={!changes().length}>
          {#if changes().length}
            <Icon name="edit" /><b>{tn(changes().length, '{n} change', '{n} changes')}</b><span class="hint">{t('not saved yet')}</span>
            <button class="btn sm ghost" onclick={() => (diffOpen = !diffOpen)} aria-expanded={diffOpen}>{diffOpen ? t('Hide the changes') : t('Show the changes')}</button>
            <span class="grow"></span>
            <button class="btn ghost" onclick={discardConfig}>{t('Discard')}</button>
            <button class="btn primary" class:busy={cfg.saving} onclick={saveConfig} disabled={cfg.saving || !editable()}>{#if cfg.saving}<span class="spinner"></span>{t('Saving…')}{:else}{t('Save to config.toml')} <span class="mono kbd">{keys('⌘S')}</span>{/if}</button>
          {:else if waiting.length}
            <Icon name="retry" /><b>{t('Saved')}</b><span class="hint">{tn(waiting.length, '{tables}, a pack’s block, applies at the next launch', '{tables}, a pack’s blocks, apply at the next launch', { tables: waiting.map((id) => `[${id}]`).join(', ') })}</span>
            <span class="grow"></span>
            <button class="btn primary" onclick={restartApp}>{t('Restart Prism')}</button>
          {:else}
            <Icon name="check" /><b>{t('Saved')}</b><span class="hint">{t('config.toml updated and applied')}</span>
          {/if}
        </div>
      </div>
    {/if}
  </section>

  {#if showToml}<TomlPanel bind:view={tomlView} />{/if}
</div></div>

<style>
  .settings { flex: 1; min-height: 0; display: flex; container-type: inline-size; }
  .layout { flex: 1; min-height: 0; display: grid; grid-template-columns: 190px 230px minmax(0, 1fr); }
  .layout.withtoml { grid-template-columns: 190px 230px minmax(0, 1fr) minmax(260px, 30%); }
  /* Rail: the domains, an icon on each one's color. */
  .rail { display: flex; flex-direction: column; gap: 2px; padding: 12px 8px; overflow: auto; background: var(--surface); border-right: 1px solid var(--border); }
  .rd { display: flex; align-items: center; gap: 8px; width: 100%; padding: 5px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .rd:hover { background: var(--surface-muted); }
  .rd.on { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); font-weight: 600; }
  .rd:first-child { margin-bottom: 6px; }
  .ico { display: inline-grid; place-items: center; width: 22px; height: 22px; flex-shrink: 0; border-radius: var(--r6); color: #fff; font-size: var(--fs-title); }
  .ico.ov { background: var(--surface-muted); color: var(--text-muted); }
  .cnt { color: var(--text-muted); font: var(--fs-tag) var(--mono); font-weight: 400; }
  .rail .ver { margin-top: auto; padding: 10px 8px 0; color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); user-select: text; }
  /* Pages of the domain: groups apart by a line, providers under their page. */
  .pages { display: flex; flex-direction: column; min-height: 0; padding: 12px 8px; overflow: auto; background: var(--surface); border-right: 1px solid var(--border); }
  .list { display: flex; flex-direction: column; gap: 1px; }
  .lhead { display: flex; align-items: center; gap: 8px; margin: 2px 0 6px; padding: 2px 6px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-title); text-align: left; cursor: pointer; }
  .lhead:hover b { color: var(--accent); }
  .list hr { width: calc(100% - 16px); margin: 5px 8px; border: none; border-top: 1px solid var(--border); }
  .grp { margin: 3px 0 1px 8px; color: var(--text-muted); font-size: var(--fs-tag); }
  .pg { display: flex; align-items: center; gap: 7px; width: 100%; padding: 4px 8px; border: none; border-radius: var(--r6); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .pg:hover { background: var(--surface-muted); }
  .pg.on { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); font-weight: 600; }
  .pg.within { color: var(--accent); }
  .pg.off .name, .pg.unused .name { color: var(--text-muted); }
  .pg.prov { padding-left: 20px; font-size: var(--fs-meta); }
  .pg.prov .sw { width: 7px; height: 7px; }
  .inuse { color: var(--ok); font-size: var(--fs-tag); font-weight: 400; }
  .where { color: var(--text-muted); font-size: var(--fs-tag); font-weight: 400; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tag { color: var(--text-muted); font-size: var(--fs-tag); font-weight: 400; }
  .tag.on { color: var(--ok); }
  .tag.repo { color: var(--info); }
  .none { margin: 8px; }
  .sample { display: block; margin-top: 4px; padding: 6px 8px; border-radius: var(--r6); background: var(--surface-muted); font-size: var(--fs-title); color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .sw { width: 8px; height: 8px; flex-shrink: 0; border-radius: var(--r2); }
  .warn { display: inline-flex; color: var(--ko); font-size: var(--fs-meta); }

  .body { display: flex; flex-direction: column; min-width: 0; overflow: auto; padding: 16px 22px 0; container-type: inline-size; }
  .crumb { display: flex; align-items: center; gap: 5px; margin-bottom: 2px; color: var(--text-muted); font-size: var(--fs-meta); }
  .crumb button { padding: 0; border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
  .crumb button:hover { color: var(--accent); }
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .feat { display: inline-flex; align-items: center; gap: 7px; padding: 2px 4px 2px 8px; border-radius: var(--r6); color: var(--text-muted); font-size: var(--fs-meta); }
  .feat.changed { background: color-mix(in srgb, var(--warn) 12%, transparent); box-shadow: inset 3px 0 0 var(--warn); }
  .sec { scroll-margin-top: 44px; }
  .soft { color: var(--text-muted); font-weight: 400; }
  /* A provider chosen at the top of its page; the others folded at the bottom. */
  .choice { display: flex; align-items: center; gap: 10px; margin-bottom: 12px; padding: 8px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .clbl { color: var(--text-muted); font-size: var(--fs-meta); }
  .uses { margin-left: 5px; color: var(--info); font-size: var(--fs-tag); font-weight: 400; }
  .others { display: flex; align-items: center; gap: 8px; width: 100%; margin: 4px 0 14px; padding: 9px 12px; border: 1px dashed var(--border); border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-meta); text-align: left; cursor: pointer; }
  .others b { color: var(--text); font-weight: 550; }
  .others small { margin-left: auto; }
  .others :global(svg) { transition: transform 0.12s; }
  .others.open :global(svg) { transform: rotate(90deg); }
  .sec.other { opacity: 0.92; }
  /* Several sections: their anchors, stuck to the top. */
  .anchors { position: sticky; top: -16px; z-index: 2; display: flex; flex-wrap: wrap; gap: 4px; margin: -4px -22px 12px; padding: 8px 22px; background: color-mix(in srgb, var(--bg) 92%, transparent); border-bottom: 1px solid var(--border); backdrop-filter: blur(6px); }
  .anchors button { display: inline-flex; align-items: center; gap: 5px; padding: 2px 9px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .anchors button:hover { color: var(--text); border-color: var(--accent); }
  /* Overview: the counters, then each domain's features and their switches. */
  .counters { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; margin-bottom: 16px; }
  .counter { display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: 10px 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text-muted); font: inherit; font-size: var(--fs-meta); cursor: pointer; }
  .counter b { color: var(--text-muted); font-size: var(--fs-head); font-weight: 650; }
  .counter:hover { border-color: var(--accent); }
  .counter.hot.changed b { color: var(--warn); }
  .counter.hot.file b { color: var(--accent); }
  .counter.hot.pack b { color: var(--info); }
  .counter.hot.attention b { color: var(--ko); }
  .domains { display: grid; grid-template-columns: repeat(auto-fill, minmax(250px, 1fr)); gap: 10px; align-items: start; margin-bottom: 16px; }
  .dom { display: flex; flex-direction: column; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); }
  .dh { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; }
  .dn { padding: 0; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-title); font-weight: 650; cursor: pointer; }
  .dn:hover { color: var(--accent); }
  .frow { display: flex; align-items: center; gap: 6px; min-height: 26px; }
  .fname { padding: 0; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .fname:hover { color: var(--accent); text-decoration: underline; }
  .frow.off .fname { color: var(--text-muted); }
  .dom .grp { margin: 6px 0 0; }
  .dom hr { width: 100%; margin: 5px 0; border: none; border-top: 1px solid var(--border); }
  @container (max-width: 700px) { .counters { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .sub { display: flex; align-items: center; gap: 8px; margin: 4px 0 8px; font-size: var(--fs-title); font-weight: 600; }
  .tabs { display: flex; gap: 0; margin: 4px 0 12px; border-bottom: 1px solid var(--border); }
  .tabs button { display: inline-flex; align-items: center; gap: 7px; padding: 6px 12px; border: none; border-bottom: 2px solid transparent; margin-bottom: -1px; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .tabs button.on { color: var(--text); font-weight: 600; border-bottom-color: var(--accent); }
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; }
  .head h3 { margin: 0; font-size: var(--fs-head); font-weight: 650; }
  .path { padding: 1px 7px; border-radius: var(--r5); background: var(--surface-muted); color: var(--text-muted); font: var(--fs-tag) var(--mono); }
  .grow { flex: 1; }
  .brings { display: flex; align-items: center; gap: 6px; margin: 8px 0 0; padding: 6px 9px; border-left: 3px solid var(--ok); border-radius: var(--r4); background: var(--ok-bg); color: var(--ok); font-size: var(--fs-meta); }
  .card { margin-bottom: 14px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); flex-shrink: 0; }
  .ch { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .ch b { font-size: var(--fs-title); }
  .ch small { color: var(--text-muted); font-size: var(--fs-meta); }
  .note { margin: 0 0 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .note.warn { color: var(--warn); }
  .note.err { color: var(--ko); }
  .hint { font-size: var(--fs-meta); color: var(--text-muted); }
  .hint.ok { color: var(--ok); }
  .hint.warn { color: var(--warn); }
  .hint.pad { margin: 0; padding: 10px 14px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .t { height: 28px; min-width: 0; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-body); outline: none; }
  .t:focus { border-color: var(--accent); }
  .t.num { width: 80px; }
  .mono { font-family: var(--mono); font-size: var(--fs-mono); }
  .toggle { position: relative; flex-shrink: 0; width: 30px; height: 17px; padding: 0; border: 1px solid var(--border); border-radius: var(--r9); background: var(--surface-muted); cursor: pointer; }
  .toggle i { position: absolute; top: 2px; left: 2px; width: 11px; height: 11px; border-radius: 50%; background: var(--text-muted); transition: left 0.15s; }
  .toggle.on { background: color-mix(in srgb, var(--accent) 40%, var(--surface)); border-color: var(--accent); }
  .toggle.on i { left: 15px; background: var(--accent); }
  .presets { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; }
  .presets button { padding: 1px 8px; border: 1px solid var(--border); border-radius: var(--r10); background: none; color: var(--text-muted); font: var(--fs-tag) var(--mono); cursor: pointer; }
  .presets button:hover { color: var(--text); border-color: var(--accent); }
  .table { display: flex; flex-direction: column; gap: 6px; padding: 10px 14px; }
  .arow { display: grid; grid-template-columns: 140px minmax(0, 1fr) 28px; gap: 8px; align-items: center; }
  .rm { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; border: none; border-radius: var(--r6); background: none; color: var(--text-muted); cursor: pointer; }
  .rm:hover { color: var(--ko); background: var(--ko-bg); }
  .rm.add:hover { color: var(--ok); background: var(--ok-bg); }
  .rm:disabled { opacity: 0.4; cursor: default; background: none; }
  .intro { margin: 0 0 10px; font-size: var(--fs-meta); color: var(--text-muted); }
  .pack { display: flex; align-items: flex-start; gap: 12px; padding: 11px 14px; border-bottom: 1px solid var(--border); }
  .pack:last-child { border-bottom: none; }
  .pinfo { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .pinfo b { display: flex; align-items: baseline; gap: 8px; font-weight: 550; font-size: var(--fs-body); }
  .pinfo small { color: var(--text-muted); font-size: var(--fs-meta); }
  .pinfo .ver { color: var(--text-muted); font-weight: 400; }
  .pinfo .src { color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .notes { max-height: 260px; overflow: auto; padding: 10px 14px; user-select: text; }
  .notes section > b { display: block; margin-bottom: 4px; }
  .notes .ntitle { display: block; color: var(--text-muted); margin-bottom: 4px; }
  .notes section + section { margin-top: 10px; padding-top: 8px; border-top: 1px solid var(--border); }
  .pinfo .hint.warn, .pinfo .hint.new { display: inline-flex; align-items: center; gap: 4px; }
  .hint.new { color: var(--accent); }
  .checkrow { display: flex; align-items: center; gap: 8px; margin: -4px 0 10px; }
  .ref { width: 110px; }
  .filerow { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .btn[aria-pressed='true'] { color: var(--accent); border-color: var(--accent); }
  .kbd { opacity: 0.7; }
  code { font-family: var(--mono); font-size: var(--fs-tag); }
  /* Changes bar: stuck to the bottom of the area, even if the section is short. */
  .savewrap { position: sticky; bottom: 0; z-index: 3; margin: auto -22px 0; }
  .diff { max-height: 220px; overflow: auto; padding: 8px 22px; border-top: 1px solid var(--border); background: var(--surface); }
  .drow { display: grid; grid-template-columns: minmax(120px, 30%) minmax(0, 1fr) auto minmax(0, 1fr) 24px; gap: 8px; align-items: center; padding: 3px 0; font-size: var(--fs-meta); }
  .dkey { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .before { color: var(--text-muted); text-decoration: line-through; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .after { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .arrow { color: var(--text-muted); }
  .reset { padding: 0 4px; border: none; border-radius: var(--r4); background: none; color: var(--text-muted); font: inherit; cursor: pointer; }
  .reset:hover { color: var(--accent); background: var(--surface-muted); }
  .savebar { display: flex; align-items: center; gap: 10px; padding: 10px 22px; background: color-mix(in srgb, var(--warn) 12%, var(--surface)); border-top: 1px solid color-mix(in srgb, var(--warn) 40%, var(--border)); color: var(--warn); animation: up 0.2s ease-out; }
  .savebar.done { background: color-mix(in srgb, var(--ok) 12%, var(--surface)); border-color: color-mix(in srgb, var(--ok) 40%, var(--border)); color: var(--ok); }
  .savebar b { color: var(--text); }
  @keyframes up { from { transform: translateY(100%); } }
  /* Narrow: the file goes below the section rather than beside it. */
  /* Narrow: the rail keeps its icons, the file goes below the page rather than beside it. */
  @container (max-width: 1100px) {
    .layout { grid-template-columns: 50px 210px minmax(0, 1fr); }
    .layout.withtoml { grid-template-columns: 50px 210px minmax(0, 1fr) minmax(240px, 30%); }
    .rail { padding: 12px 6px; }
    .rd { justify-content: center; padding: 5px 0; }
    .rd .name, .rd .cnt, .rail .ver { display: none; }
  }
  @container (max-width: 980px) {
    .layout.withtoml { grid-template-columns: 50px 200px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) 260px; }
    .layout.withtoml > :global(.file) { grid-column: 3; border-left: none; border-top: 1px solid var(--border); }
    .layout.withtoml .rail, .layout.withtoml .pages { grid-row: 1 / span 2; }
  }
</style>
