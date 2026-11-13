<script lang="ts">
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
  import TomlPreview from './settings/TomlPreview.svelte'
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
  import { STATUS_ITEMS, SIDE_BREAK } from '../statusbar'
  import { statusbarEditing } from '../statusbar-edit.svelte'
  import { SCHEMA, type SectionSpec } from '../settings-schema'
  import { ensure, failing, recheck, runs, sectionRequests } from '../settings-checks.svelte'
  import { live } from '../live.svelte'
  import { chainOf, descendants, NODES, PARENT, pageOf, TREE, type TreeNode } from '../settings-tree'
  import { cfg, changes, discardConfig, editable, importPack, isChanged, loadConfig, mergedRows, mergedToml, packBusy, packRef, removePack, restartApp, saveConfig, setValue, settingsView, updatePack, valueOf } from '../settings.svelte'
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
   * Settings view: the mirror of config.toml. A tree of pages sorted by what they configure
   * (settings-tree.ts), the chosen page — one or more sections, each a table of the file — and the
   * file alongside, showing what saving would write. Appearance gathers the app preferences
   * (theme, Fun), applied immediately; the rest is saved with ⌘S.
   */
  /** Profiles, Packs, Appearance, Console, Notes and Config file have their own layout; the others come from the schema. */
  interface Section { id: string; label: string; table: string; spec?: SectionSpec }
  const SECTIONS: Section[] = [
    { id: 'profiles', label: t('Profiles'), table: 'profiles/' },
    { id: 'packs', label: t('Packs'), table: 'packs = […]' },
    { id: 'appearance', label: t('Appearance'), table: '[appearance]' },
    { id: 'console', label: t('Console'), table: '[console]' },
    { id: 'notes', label: t('Notes'), table: '[notes]' },
    { id: 'updates-now', label: t('Updates'), table: '[updates]' },
    { id: 'remote-devices', label: t('Devices'), table: '[remote]' },
    { id: 'remote-away-checks', label: t('Before leaving'), table: '[remote]' },
    { id: 'feedback-requests', label: t('My requests'), table: 'feedback.json' },
    { id: 'dependencies', label: t('Dependencies'), table: 'PATH' },
    { id: 'cli-command', label: t('Command line'), table: 'prism --help' },
    { id: 'custom-blocks', label: t('Custom blocks'), table: '[[custom_blocks]]' },
    { id: 'routines', label: t('Routines'), table: '[[agents.routines]]' },
    { id: 'mcp-agents', label: t('Agents'), table: 'prism mcp' },
    { id: 'dictation-panel', label: t('Engine and models'), table: '[dictation]' },
    { id: 'cleanup-usage', label: t('What you use'), table: 'usage.json' },
    ...SCHEMA.map((spec) => ({ id: spec.id, label: spec.label, table: spec.table, spec })),
    { id: 'file', label: t('Config file'), table: 'config.toml' },
  ]
  const SECTION = new Map(SECTIONS.map((s) => [s.id, s]))
  const sectionsOf = (n: TreeNode) => (n.sections ?? []).map((id) => SECTION.get(id)).filter((s) => !!s)

  /* ---------- Tree ---------- */
  /** The page shown, and its sections; a link may name a section (settings/gitlab) or a page. */
  const page = $derived(pageOf(settingsView.section) ?? NODES.get('appearance')!)
  const chain = $derived(chainOf(page))
  const pageSections = $derived(sectionsOf(page))
  /** A page in tabs (a delivery model's stages): its first section, then the chosen tab. */
  const tab = $derived(page.tabs ? (pageSections.slice(1).find((s) => s.id === settingsView.section) ?? pageSections[1]) : undefined)
  const shown = $derived(page.tabs ? [pageSections[0], tab!] : pageSections)
  /** Once each: Repositories' two sections both live in `[[repos]]`. */
  const tables = $derived([...new Set(pageSections.map((s) => s.table))])
  const go = (id: string) => (settingsView.section = id)
  /** “Delivery · branch stage” → “Branch stage”: a tab's name. */
  const stageLabel = (label: string) => label.replace(/^.*· /, '').replace(/^./, (c) => c.toUpperCase())
  // A link to a section lower on its page scrolls to it.
  $effect(() => {
    const id = settingsView.section
    if (page.tabs || id === page.id || id === pageSections[0]?.id) return
    requestAnimationFrame(() => document.getElementById(`sec-${id}`)?.scrollIntoView({ block: 'start', behavior: 'smooth' }))
  })

  /** The feature switch of a node: its section's `enabled` key. */
  const switchPath = (n: TreeNode) => {
    const spec = n.toggle ? SECTION.get(n.toggle)?.spec : undefined
    return spec ? [...spec.path, 'enabled'] : null
  }
  const isOn = (n: TreeNode) => {
    const p = switchPath(n)
    return p && cfg.file ? !!valueOf(p) : undefined
  }
  const pageSwitch = $derived(switchPath(page))
  const pageOn = $derived(!!pageSwitch && !!valueOf(pageSwitch))
  const switchField = (n: TreeNode) => SECTION.get(n.toggle ?? '')?.spec?.fields.find((f) => f.key === 'enabled')
  /** The value of a node's choice (Tracker › Provider…), and of its parent's for a provider. */
  const chosen = (n: TreeNode | null | undefined) => (n?.choice ? String(valueOf(n.choice.path, n.choice.fallback)) : undefined)
  const parentOf = (n: TreeNode) => PARENT.get(n.id)
  /** A provider other than its parent's choice. */
  const unchosen = (n: TreeNode) => !!n.provider && n.provider !== chosen(parentOf(n))
  /** The repositories running an agent CLI of their own (`[[repos]] agent`), for its node. */
  const repoUses = (n: TreeNode) => (n.provider && parentOf(n)?.id === 'agents' ? mergedRows(['repos']).filter((r) => r.agent === n.provider).length : 0)
  /** An unused provider's tag: the repositories that run it, if any. */
  const unusedTag = (n: TreeNode) => (repoUses(n) ? tn(repoUses(n), '{n} repo', '{n} repos') : t('unused'))
  // An unused provider stays plain: its keys are edited ahead, or apply to a repository's agents.
  const inactive = (n: TreeNode) => isOn(n) === false
  const nodeSections = (n: TreeNode) => descendants(n).flatMap(sectionsOf)
  const nodeDirty = (n: TreeNode) => nodeSections(n).some(dirty)
  const nodeAttention = (n: TreeNode) => nodeSections(n).some(attention)

  /** Search: names, tables, and the keys and labels of the fields. */
  let query = $state('')
  type Filter = 'all' | 'on' | 'changed' | 'attention'
  let filter = $state<Filter>('all')
  const FILTERS: { id: Filter; label: string; title: string }[] = [
    { id: 'all', label: t('All'), title: t('Every page') },
    { id: 'on', label: tc('filter', 'On'), title: t('The features switched on') },
    { id: 'changed', label: tc('settings', 'Changed'), title: t('Unsaved changes') },
    { id: 'attention', label: t('Attention'), title: t('A check failed, or a source could not be read') },
  ]
  const textOf = (n: TreeNode) =>
    [n.label, ...sectionsOf(n).flatMap((s) => [s.label, s.table, ...(s.spec?.fields.flatMap((f) => [f.key, f.label]) ?? [])])].join(' ').toLowerCase()
  const TEXT = new Map([...NODES.values()].map((n) => [n.id, textOf(n)]))
  /** `[cleanup] hide_off`: a feature turned off leaves the tree, unless searched for or shown. */
  const hideOff = $derived(!!valueOf(['cleanup', 'hide_off'], false))
  const selfMatch = (n: TreeNode) => {
    const q = query.trim().toLowerCase()
    if (q && !TEXT.get(n.id)!.includes(q)) return false
    if (filter === 'on') return isOn(n) === true
    if (filter === 'changed') return nodeDirty(n)
    if (filter === 'attention') return nodeAttention(n)
    return true
  }
  const offHidden = (n: TreeNode) => hideOff && !query.trim() && inactive(n) && !chainOf(page).includes(n)
  const visibleNode = (n: TreeNode): boolean => !offHidden(n) && (selfMatch(n) || (n.children ?? []).some(visibleNode))
  const narrowed = $derived(!!query.trim() || filter !== 'all')
  /** Folded nodes, remembered; a search or a filter unfolds every match. */
  let folded = $state(new Set(pref<string[]>('settings.folded', [])))
  const isOpen = (n: TreeNode) => narrowed || !folded.has(n.id)
  function fold(n: TreeNode, open = folded.has(n.id)) {
    const next = new Set(folded)
    if (open) next.delete(n.id)
    else next.add(n.id)
    folded = next
    setPref('settings.folded', [...next])
  }
  /** ← folds the page's node, → unfolds it, while the tree has the focus. */
  function onTreeKey(e: KeyboardEvent) {
    if (!page.children || (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight')) return
    e.preventDefault()
    fold(page, e.key === 'ArrowRight')
  }
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

  let showToml = $state(pref<boolean>('settings.toml', true))
  function toggleToml() {
    showToml = !showToml
    setPref('settings.toml', showToml)
  }
  /**
   * What the TOML panel shows: config.toml (what saving would write), a pack's file, read-only,
   * or the packs and config.toml merged as the app reads them, the unsaved changes included.
   */
  let tomlView = $state(pref<string>('settings.tomlView', 'file'))
  const tomlPack = $derived(cfg.file?.pack_files.find((p) => `pack:${p.name}` === tomlView))
  const tomlMode = $derived(tomlView === 'merged' && cfg.file?.pack_files.length ? 'merged' : tomlPack ? 'pack' : 'file')
  function pickToml(v: string) {
    tomlView = v
    setPref('settings.tomlView', v)
  }
  let merged = $state({ text: '', error: '' })
  $effect(() => {
    if (!showToml || tomlMode !== 'merged' || !isDesktop()) return
    let gone = false
    mergedToml(cfg.preview).then(
      (text) => !gone && (merged = { text, error: '' }),
      (e) => !gone && (merged = { text: '', error: String(e) }),
    )
    return () => (gone = true)
  })

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
    if (page.id !== 'updates' || updatesRead) return
    updatesRead = true
    void checkUpdates()
  })
  /** The same for the packs' sources, on opening Packs. */
  let packsRead = false
  $effect(() => {
    if (page.id !== 'packs' || packsRead) return
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

  function onkeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 's') {
      e.preventDefault()
      void saveConfig()
    }
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
  <aside class="toc">
    <label class="search"><Icon name="search" /><input bind:value={query} placeholder={t('Search settings or keys…')} spellcheck="false" /></label>
    <div class="filters">
      {#each FILTERS as f (f.id)}<button class:on={filter === f.id} onclick={() => (filter = f.id)} title={f.title}>{f.label}</button>{/each}
    </div>
    {#snippet branch(list: TreeNode[], depth: number)}
      {#each list.filter(visibleNode) as n (n.id)}
        <div class="node" class:root={depth === 0} class:on={n.id === page.id} class:off={inactive(n)}>
          {#if n.children}<button class="car" class:open={isOpen(n)} tabindex="-1" onclick={() => fold(n)} aria-label={isOpen(n) ? t('Fold {name}', { name: n.label }) : t('Unfold {name}', { name: n.label })}><Icon name="chevronRight" /></button>{:else}<span class="car"></span>{/if}
          <button class="lbl" onclick={() => go(n.id)} title={sectionsOf(n).map((s) => s.table).join(' ')}>
            {#if depth === 0}<span class="sw" style:background={n.color}></span>{/if}
            <span class="name">{n.label}</span>
            {#if n.choice}<span class="tag">{chosen(n)}</span>{:else if unchosen(n)}<span class="tag" class:repo={repoUses(n) > 0}>{unusedTag(n)}</span>{/if}
            {#if nodeDirty(n)}<span class="dot" title={t('Unsaved changes')}></span>
            {:else if nodeAttention(n)}<span class="warn" title={t('A check failed, or this source could not be read')}><Icon name="warning" /></span>
            {:else if isOn(n) !== undefined}<span class="st" class:lit={isOn(n)} title={isOn(n) ? t('On') : t('Off')}></span>{/if}
          </button>
        </div>
        {#if n.children && isOpen(n)}<div class="kids">{@render branch(n.children, depth + 1)}</div>{/if}
      {:else}
        {#if depth === 0}<p class="hint none">{t('Nothing matches.')}</p>{/if}
      {/each}
    {/snippet}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <nav class="tree" onkeydown={onTreeKey}>{@render branch(TREE, 0)}</nav>
    <span class="ver" title="{t('Prism version, and the commit it was built from')}{COMMIT.endsWith('+') ? t(' (+: with uncommitted changes)') : ''}">Prism {versionLabel()}</span>
  </aside>

  <section class="body">
    {#if chain.length > 1}<div class="crumb">{#each chain.slice(0, -1) as c (c.id)}<button onclick={() => go(c.id)}>{c.label}</button><span>›</span>{/each}</div>{/if}
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
      {#if pageSections.some((s) => s.spec && sectionRequests(s.spec).length)}<button class="btn ghost" onclick={recheck} title={t('Run the checks of the settings again')}>{t('Check again')}</button>{/if}
      {#if editable()}<button class="btn ghost" onclick={() => configurePrism({ page: chain.map((c) => c.label).join(' › '), table: tables.join(' ') })} title={t('Ask an agent about this page: it reads Prism’s docs and your config, and proposes the change')}>✦ {t('Ask an agent')}</button>{/if}
      <button class="btn ghost" onclick={toggleToml} aria-pressed={showToml} title={t('Show config.toml next to the settings')}>TOML</button>
    </div>

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
              {#if p.source}<button class="btn" class:primary={!!pc?.changed && !pc.too_new} onclick={() => updatePackThen(p.name, packRef(p.source) !== null ? (refs[p.name] ?? packRef(p.source)) : null)} disabled={!!packBusy.name || !editable()} title={refs[p.name] !== undefined && refs[p.name] !== packRef(p.source) ? t('Import it again from {source}, at {ref}', { source: p.source, ref: refs[p.name] || t('the default branch') }) : t('Import it again from {source}', { source: p.source })}>{packBusy.name === p.name ? t('Updating…') : t('Update')}</button>{/if}
              <button class="btn" class:armed={armed.is(`pack:${p.name}`)} onclick={() => askRemove(p.name)} disabled={!!packBusy.name || !editable()} title={t('Take it out of config.toml and packs/ (click twice)')}><ArmedLabel label={t('Remove')} armedLabel={t('Remove?')} armed={armed.is(`pack:${p.name}`)} /></button>
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
          <button class="btn" onclick={() => checkPacks()} disabled={packUpdates.checking || !!packBusy.name} title={t('Read the packs’ sources again ([updates] packs: at launch and every 6 hours)')}>{packUpdates.checking ? t('Checking…') : t('Check now')}</button>
        </div>
      {/if}
      <ModulesSettings />
      <div class="card">
        <div class="ch"><b>{t('Import a pack')}</b><small>{t('copied into packs/ next to config.toml, and added to packs')}</small></div>
        <div class="filerow">
          <input class="t mono grow" bind:value={packSource} placeholder={t('~/team/pack.toml, glab:group/project:path/pack.toml@main or gh:owner/repo:path/pack.toml')} spellcheck="false" onkeydown={(e) => e.key === 'Enter' && addPack()} aria-label={t('Pack source')} />
          {#if canPick()}<button class="btn" onclick={choosePack} disabled={!!packBusy.name || !editable()} title={t('Choose a pack file')}><Icon name="folder" />{t('Choose…')}</button>{/if}
          <button class="btn primary" onclick={addPack} disabled={!packSource.trim() || !!packBusy.name || !editable()}>{packBusy.name === '+import' ? t('Importing…') : t('Import')}</button>
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
          <ListInput value={valueOf<string[]>(['appearance', 'statusbar_hidden'], [])} onchange={(v) => setValue(['appearance', 'statusbar_hidden'], v, [])} options={STATUS_ITEMS.map((i) => i.id)} compact />
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
            <button class="btn" onclick={() => checkUpdates()} disabled={updates.checking || updates.installing} title={t('Read the releases of the saved source again')}>{updates.checking ? t('Checking…') : t('Check now')}</button>
            {#if u?.available && u.install}<button class="btn primary" class:armed={armed.is('install')} onclick={askInstall} disabled={updates.installing} title={u.install === 'replace' ? t('Download, replace this Prism and restart it (click twice)') : t('Download the package (click twice)')}>{#if updates.installing}{t('Installing…')}{:else}<ArmedLabel label={t('Install {version}', { version: u.latest!.version })} armedLabel={u.install === 'replace' ? t('Install and restart?') : t('Download?')} armed={armed.is('install')} />{/if}</button>{/if}
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
      <SectionFields spec={s.spec} hide={page.toggle === s.id ? ['enabled'] : []} />
    {/if}
    {/snippet}

    {#if unchosen(page)}
      {@const parent = parentOf(page)!}
      {@const uses = repoUses(page)}
      <p class="note">{t('Not {noun} (now {chosen})', { noun: parent.choice!.noun, chosen: chosen(parent) ?? '' })}{#if uses}{tn(uses, ', but {n} repository runs it (Repositories › Agent): its keys apply there', ', but {n} repositories run it (Repositories › Agent): its keys apply there')}{:else}{t(': its keys apply once it is chosen')}{/if}. <button class="link" onclick={() => setValue(parent.choice!.path, page.provider)}>{t('Make {name} {noun}', { name: page.label, noun: parent.choice!.noun })}</button></p>
    {/if}
    {#each shown as s, i (s.id)}
      {#if page.tabs && i === 1}
        <div class="tabs" role="tablist">
          {#each pageSections.slice(1) as ts (ts.id)}
            <button role="tab" class:on={ts.id === tab?.id} aria-selected={ts.id === tab?.id} onclick={() => go(ts.id)}>{stageLabel(ts.label)}<span class="path">{ts.table}</span>{#if dirty(ts)}<span class="dot" title={t('Unsaved changes')}></span>{/if}</button>
          {/each}
        </div>
      {/if}
      <div class="sec" id="sec-{s.id}">
        {#if shown.length > 1 && !(page.tabs && i === 1) && !(i === 0 && s.label === page.label)}<h4 class="sub">{s.label}<span class="path">{s.table}</span></h4>{/if}
        {@render body(s)}
      </div>
    {/each}
    {#if page.children}
      {#if page.sections}<h4 class="sub">{page.choice ? t('Providers') : t('More')}</h4>{:else}<p class="intro">{tn(page.children.length, '{name}: {n} page.', '{name}: {n} pages.', { name: page.label })}</p>{/if}
      <div class="subs">
        {#each page.children as c (c.id)}
          <button class="subp" class:off={inactive(c)} onclick={() => go(c.id)}>
            <span class="ptop"><b>{c.label}</b><span class="grow"></span>
              {#if unchosen(c)}<span class="tag" class:repo={repoUses(c) > 0}>{unusedTag(c)}</span>{:else if c.provider}<span class="tag on">{tc('settings', 'in use')}</span>{/if}
              {#if nodeDirty(c)}<span class="dot" title={t('Unsaved changes')}></span>{:else if nodeAttention(c)}<span class="warn"><Icon name="warning" /></span>{:else if isOn(c) !== undefined}<span class="st" class:lit={isOn(c)} title={isOn(c) ? t('On') : t('Off')}></span>{/if}
            </span>
            <small class="mono">{sectionsOf(c).map((x) => x.table).join(' ')}</small>
            {#if c.children}<small>{c.children.map((k) => k.label).join(', ')}</small>{/if}
          </button>
        {/each}
      </div>
    {/if}
    {#if cfg.file?.error}<p class="note err">{t('config.toml does not load: {error}', { error: cfg.file.error })}</p>{/if}
    {#if cfg.file?.unread?.length}<p class="note warn">{t('Read by nothing in this Prism: {tables}. A module not installed, or a typo.', { tables: cfg.file.unread.map((tb) => `[${tb}]`).join(', ') })}</p>{/if}

    {#if changes().length || saved || waiting.length}
      <div class="savebar" class:done={!changes().length}>
        {#if changes().length}
          <Icon name="edit" /><b>{tn(changes().length, '{n} change', '{n} changes')}</b><span class="hint">{t('not saved yet')}</span>
          <span class="grow"></span>
          <button class="btn ghost" onclick={discardConfig}>{t('Discard')}</button>
          <button class="btn primary" onclick={saveConfig} disabled={cfg.saving || !editable()}>{cfg.saving ? t('Saving…') : t('Save to config.toml')} <span class="mono kbd">{keys('⌘S')}</span></button>
        {:else if waiting.length}
          <Icon name="retry" /><b>{t('Saved')}</b><span class="hint">{tn(waiting.length, '{tables}, a pack’s block, applies at the next launch', '{tables}, a pack’s blocks, apply at the next launch', { tables: waiting.map((id) => `[${id}]`).join(', ') })}</span>
          <span class="grow"></span>
          <button class="btn primary" onclick={restartApp}>{t('Restart Prism')}</button>
        {:else}
          <Icon name="check" /><b>{t('Saved')}</b><span class="hint">{t('config.toml updated and applied')}</span>
        {/if}
      </div>
    {/if}
  </section>

  {#if showToml}
    <aside class="file">
      <div class="th">
        {#if cfg.file?.pack_files.length}
          <select class="tsel" value={tomlMode === 'pack' ? tomlView : tomlMode} onchange={(e) => pickToml(e.currentTarget.value)} aria-label={t('File shown')}>
            <option value="file">config.toml</option>
            {#each cfg.file.pack_files as p (p.name)}<option value="pack:{p.name}">{t('Pack {name}', { name: p.name })}</option>{/each}
            <option value="merged">{tc('settings', 'Merged')}</option>
          </select>
        {:else}<b>config.toml</b>{/if}
        {#if tomlMode === 'pack'}
          <span class="mono grow">{tomlPack?.path.replace(/^\/Users\/[^/]+/, '~')}</span><span title={t('Update it from Settings › General › Packs')}>{t('read-only')}</span>
        {:else if tomlMode === 'merged'}
          <span class="grow" title={t('Tables merge key by key, [[repos]] by name, anything else is replaced; [pack] and the sections a pack may not set are left out. The defaults are not shown.')}>{t('packs, then config.toml on top')}</span><span>{changes().length ? t('{n} unsaved', { n: changes().length }) : t('saved')}</span>
        {:else}
          <span class="mono grow">{cfg.file?.path.replace(/^\/Users\/[^/]+/, '~') ?? ''}</span><span>{changes().length ? t('{n} unsaved', { n: changes().length }) : t('saved')}</span>
        {/if}
      </div>
      {#if tomlMode === 'pack' && tomlPack}
        <TomlPreview text={tomlPack.text} original={tomlPack.text} />
      {:else if tomlMode === 'merged'}
        {#if merged.error}<p class="note err pad">{merged.error}</p>
        {:else}<TomlPreview text={merged.text || (cfg.file?.merged ?? '')} original={cfg.file?.merged ?? ''} />{/if}
      {:else}
        <TomlPreview text={cfg.preview} original={cfg.file?.text ?? ''} />
      {/if}
    </aside>
  {/if}
</div></div>

<style>
  .settings { flex: 1; min-height: 0; display: flex; container-type: inline-size; }
  .layout { flex: 1; min-height: 0; display: grid; grid-template-columns: 230px minmax(0, 1fr); }
  .layout.withtoml { grid-template-columns: 230px minmax(0, 1fr) minmax(260px, 30%); }
  .toc { display: flex; flex-direction: column; gap: 1px; padding: 12px 8px; overflow: auto; background: var(--surface); border-right: 1px solid var(--border); }
  .search { display: flex; align-items: center; gap: 6px; height: 28px; margin: 0 2px 8px; padding: 0 9px; border: 1px solid var(--border); border-radius: var(--r7); background: var(--bg); color: var(--text-muted); }
  .search input { flex: 1; min-width: 0; border: none; outline: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); }
  .filters { display: flex; gap: 4px; margin: 0 2px 6px; }
  .filters button { padding: 1px 7px; border: 1px solid var(--border); border-radius: var(--r10); background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-tag); cursor: pointer; }
  .filters button.on { color: var(--accent); border-color: var(--accent); }
  /* Tree: a caret to fold, then the page; children indented along a line. */
  .tree { display: flex; flex-direction: column; gap: 1px; }
  .node { display: flex; align-items: center; border-radius: var(--r6); }
  .node:hover { background: var(--surface-muted); }
  .node.root { margin-top: 6px; }
  .node.root .name { font-weight: 600; }
  .node.on { background: color-mix(in srgb, var(--accent) 15%, transparent); }
  .node.on .lbl { color: var(--accent); font-weight: 600; }
  .node.off .name { color: var(--text-muted); }
  .car { display: inline-flex; align-items: center; justify-content: center; width: 16px; height: 24px; flex-shrink: 0; padding: 0; border: none; background: none; color: var(--text-muted); cursor: pointer; }
  .car :global(svg) { transition: transform 0.12s; }
  .car.open :global(svg) { transform: rotate(90deg); }
  .lbl { flex: 1; min-width: 0; display: flex; align-items: center; gap: 7px; padding: 4px 8px 4px 2px; border: none; background: none; color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .kids { display: flex; flex-direction: column; gap: 1px; margin-left: 15px; padding-left: 3px; border-left: 1px solid var(--border); }
  .tag { color: var(--text-muted); font-size: var(--fs-tag); font-weight: 400; }
  .tag.on { color: var(--ok); }
  .tag.repo { color: var(--info); }
  /* A feature's state: lit when on, hollow when off. */
  .st { width: 7px; height: 7px; flex-shrink: 0; border-radius: 50%; border: 1px solid var(--text-muted); opacity: 0.6; }
  .st.lit { background: var(--ok); border-color: var(--ok); opacity: 1; }
  .none { margin: 8px; }
  .sample { display: block; margin-top: 4px; padding: 6px 8px; border-radius: var(--r6); background: var(--surface-muted); font-size: 13px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .toc .ver { margin-top: auto; padding: 10px 8px 0; color: var(--text-muted); font-family: var(--mono); font-size: var(--fs-tag); user-select: text; }
  .toc button.on { background: color-mix(in srgb, var(--accent) 15%, transparent); color: var(--accent); font-weight: 600; }
  .sw { width: 8px; height: 8px; flex-shrink: 0; border-radius: var(--r2); }
  .warn { display: inline-flex; color: var(--ko); font-size: 11px; }
  .dot { width: 6px; height: 6px; flex-shrink: 0; border-radius: 50%; background: var(--warn); }

  .body { display: flex; flex-direction: column; min-width: 0; overflow: auto; padding: 16px 22px 0; container-type: inline-size; }
  .crumb { display: flex; align-items: center; gap: 5px; margin-bottom: 2px; color: var(--text-muted); font-size: var(--fs-meta); }
  .crumb button { padding: 0; border: none; background: none; color: inherit; font: inherit; cursor: pointer; }
  .crumb button:hover { color: var(--accent); }
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .feat { display: inline-flex; align-items: center; gap: 7px; padding: 2px 4px 2px 8px; border-radius: var(--r6); color: var(--text-muted); font-size: var(--fs-meta); }
  .feat.changed { background: color-mix(in srgb, var(--warn) 12%, transparent); box-shadow: inset 3px 0 0 var(--warn); }
  .sec { scroll-margin-top: 8px; }
  .sub { display: flex; align-items: center; gap: 8px; margin: 4px 0 8px; font-size: var(--fs-title); font-weight: 600; }
  .tabs { display: flex; gap: 0; margin: 4px 0 12px; border-bottom: 1px solid var(--border); }
  .tabs button { display: inline-flex; align-items: center; gap: 7px; padding: 6px 12px; border: none; border-bottom: 2px solid transparent; margin-bottom: -1px; background: none; color: var(--text-muted); font: inherit; font-size: var(--fs-body); cursor: pointer; }
  .tabs button.on { color: var(--text); font-weight: 600; border-bottom-color: var(--accent); }
  .subs { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px; margin-bottom: 14px; }
  .subp { display: flex; flex-direction: column; gap: 3px; padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--r10); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-body); text-align: left; cursor: pointer; }
  .subp:hover { border-color: var(--accent); }
  .ptop { display: flex; align-items: center; gap: 8px; }
  .subp.off b { color: var(--text-muted); }
  .subp small { color: var(--text-muted); font-size: var(--fs-tag); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .link { padding: 0; border: none; background: none; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; }
  .head h3 { margin: 0; font-size: 16px; font-weight: 650; }
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
  .btn.armed { color: var(--ko); border-color: var(--ko); }
  .filerow { display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .btn { display: inline-flex; align-items: center; gap: 5px; padding: 3px 10px; border: 1px solid var(--border); border-radius: var(--r6); background: var(--surface); color: var(--text); font: inherit; font-size: var(--fs-meta); cursor: pointer; white-space: nowrap; }
  .btn:hover { border-color: var(--accent); }
  .btn.ghost { background: none; }
  .btn[aria-pressed='true'] { color: var(--accent); border-color: var(--accent); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: var(--on-accent); background-image: var(--btn-sheen); box-shadow: 0 0 var(--btn-glow) var(--accent); text-transform: var(--btn-case); letter-spacing: var(--btn-spacing); }
  .btn:disabled { opacity: 0.6; cursor: progress; }
  .kbd { opacity: 0.7; }
  code { font-family: var(--mono); font-size: var(--fs-tag); }
  /* Changes bar: stuck to the bottom of the area, even if the section is short. */
  .savebar { position: sticky; bottom: 0; display: flex; align-items: center; gap: 10px; margin: auto -22px 0; padding: 10px 22px; background: color-mix(in srgb, var(--warn) 12%, var(--surface)); border-top: 1px solid color-mix(in srgb, var(--warn) 40%, var(--border)); color: var(--warn); animation: up 0.2s ease-out; }
  .savebar.done { background: color-mix(in srgb, var(--ok) 12%, var(--surface)); border-color: color-mix(in srgb, var(--ok) 40%, var(--border)); color: var(--ok); }
  .savebar b { color: var(--text); }
  @keyframes up { from { transform: translateY(100%); } }
  .file { display: flex; flex-direction: column; min-width: 0; overflow: hidden; background: #121211; border-left: 1px solid var(--border); }
  .th { display: flex; align-items: center; gap: 8px; padding: 9px 12px; border-bottom: 1px solid var(--border); color: var(--text-muted); font-size: var(--fs-meta); }
  .th b { color: var(--text); font-size: var(--fs-body); }
  .th .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tsel { padding: 1px 4px; border: 1px solid var(--border); border-radius: var(--r5); background: none; color: var(--text); font: inherit; font-size: var(--fs-body); font-weight: 600; cursor: pointer; }
  .note.pad { padding: 10px 12px; }
  /* Narrow: the file goes below the section rather than beside it. */
  @container (max-width: 980px) {
    .layout.withtoml { grid-template-columns: 180px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) 260px; }
    .layout.withtoml .file { grid-column: 2; border-left: none; border-top: 1px solid var(--border); }
    .layout.withtoml .toc { grid-row: 1 / span 2; }
  }
</style>
