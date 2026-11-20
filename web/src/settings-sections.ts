import { t, tc } from './i18n.svelte'
import { SCHEMA, type FieldSpec, type SectionSpec } from './settings-schema'
import { LANGUAGES } from './i18n.svelte'
import { THEMES } from './theme.svelte'
import { SAVER_MODES } from './fun.svelte'

/**
 * The sections of the Settings pages: those of settings-schema.ts, and those Settings.svelte draws
 * itself (Profiles, Packs, Appearance…). A section keeps its TOML table; settings-tree.ts says on
 * which page it shows.
 */
export interface Section {
  id: string
  label: string
  table: string
  spec?: SectionSpec
}

export const SECTIONS: Section[] = [
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
export const SECTION = new Map(SECTIONS.map((s) => [s.id, s]))

/**
 * The keys of the sections Settings.svelte draws itself, described as the schema's are, for the
 * search to find them (and change the simple ones from a result). Their labels are the page's.
 */
const toggle = (key: string, label: string, syn?: string): FieldSpec => ({ key, label, kind: 'toggle', syn })
export const OWN_FIELDS: Record<string, FieldSpec[]> = {
  appearance: [
    { key: 'theme', label: t('Theme'), kind: 'select', options: ['system', ...THEMES.map((th) => th.id)], syn: t('dark, light, colours, colors, night, synthwave') },
    toggle('theme_preview', t('Theme preview')),
    { key: 'density', label: t('Density'), kind: 'select', options: ['compact', 'comfortable'], syn: t('size, spacing, compact, comfortable, larger text') },
    { key: 'zoom', label: tc('settings', 'Zoom'), kind: 'number', step: 0.05, syn: t('scale, size, bigger, smaller, larger text, magnify, zoom in, zoom out') },
    { key: 'font', label: t('Font'), kind: 'text', syn: t('typeface, font family, text') },
    { key: 'font_mono', label: t('Mono font'), kind: 'text', syn: t('typeface, monospace, code') },
    { key: 'font_terminal', label: t('Terminal font'), kind: 'text', syn: t('typeface, monospace, nerd font, shell') },
    { key: 'language', label: t('Language'), kind: 'select', optional: true, options: LANGUAGES.map((l) => l.id), syn: t('french, english, translation, locale') },
    { key: 'locale', label: t('Dates and numbers'), kind: 'text', syn: t('date format, locale, region') },
    toggle('boot', t('Boot screen'), t('splash, launch')),
    toggle('animations', t('Animations'), t('motion, effects')),
    toggle('theme_motion', t('Theme motion'), t('motion, effects')),
    toggle('celebrations', t('Celebrations'), t('confetti, fireworks')),
    toggle('easter_eggs', t('Easter eggs')),
    toggle('friday_guard', t('Friday guard'), t('production, deploy, friday')),
    toggle('tooltips', t('Tooltips'), t('hover, help bubbles')),
    { key: 'tooltip_delay', label: t('Delay before a tooltip shows'), kind: 'number', unit: 'ms' },
    toggle('ticket_threads', t('Ticket threads')),
    toggle('page_swipe', t('Swipe between pages'), t('trackpad, gesture')),
    toggle('pin_drawers', t('Pin side panes'), t('drawer, panel')),
    { key: 'statusbar', label: t('Status bar order'), kind: 'list' },
    { key: 'statusbar_hidden', label: t('Hidden in the status bar'), kind: 'list' },
    toggle('screensaver', t('Screensaver'), t('idle, sleep')),
    { key: 'screensaver_seconds', label: t('Seconds before it starts'), kind: 'number', unit: 's' },
    { key: 'screensaver_mode', label: t('Screensaver shown'), kind: 'select', options: [...SAVER_MODES, 'random'] },
    toggle('ignore_reduce', t('Ignore Reduce motion and transparency'), t('accessibility')),
  ],
  console: [
    { key: 'key', label: t('Shortcut'), kind: 'text', syn: t('keyboard shortcut, hotkey, console key') },
    { key: 'history', label: t('History'), kind: 'number', unit: 'lines' },
    { key: 'aliases', label: t('Aliases'), kind: 'list', syn: t('commands, shortcuts') },
  ],
  notes: [{ key: 'key', label: t('Shortcut'), kind: 'text', syn: t('keyboard shortcut, hotkey, quick notes') }],
}
/** Their tables' paths: `[appearance]` is `appearance`. */
export const OWN_PATHS: Record<string, string[]> = { appearance: ['appearance'], console: ['console'], notes: ['notes'] }

/** A section's fields: the schema's, or those of a section Settings.svelte draws. */
export const fieldsOf = (s: Section): FieldSpec[] => s.spec?.fields ?? OWN_FIELDS[s.id] ?? []
/** A section's table, as a path: `[[repos]]` and the like are keys of the root. */
export const pathOf = (s: Section): string[] | undefined => s.spec?.path ?? OWN_PATHS[s.id]
