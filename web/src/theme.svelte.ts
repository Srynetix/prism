import { pref } from './layout.svelte'
import { live } from './live.svelte'
import { saveNow } from './settings.svelte'
import { t } from './i18n.svelte'

/**
 * App theme: a set of color tokens (app.css) set on `<html data-theme=…>`.
 * "System" follows macOS: Paper by day, Graphite by night. The choice is in config.toml
 * (`[appearance] theme`): the Theme menu saves it there right away, Settings too.
 */
export interface Theme {
  id: string
  label: string
  hint: string
  /** Light or dark: the picker's filter. */
  tone: 'light' | 'dark'
  /** Theme family, as a subheading in the picker. */
  group?: string
  /** Background, surface, accent and a source color: the picker's preview. */
  swatch: [string, string, string, string]
}

const LIST: Theme[] = [
  { id: 'paper', label: 'Paper', hint: 'light', tone: 'light', group: 'Default', swatch: ['#f6f5f1', '#ffffff', '#7c5cdb', '#e2590f'] },
  { id: 'graphite', label: 'Graphite', hint: 'dark', tone: 'dark', group: 'Default', swatch: ['#1a1a18', '#242422', '#a78bfa', '#fc6d26'] },
  { id: 'fjord', label: 'Fjord', hint: 'cool dark', tone: 'dark', group: 'Default', swatch: ['#1b2129', '#222a34', '#5fb3b3', '#e8875a'] },
  { id: 'midnight', label: 'Midnight', hint: 'deep blue', tone: 'dark', group: 'Default', swatch: ['#0e1020', '#161931', '#8b7bff', '#ff8a50'] },
  { id: 'sand', label: 'Sand', hint: 'warm light', tone: 'light', group: 'Default', swatch: ['#f3eee4', '#fbf8f2', '#c2622d', '#6f5fc9'] },
  { id: 'contrast', label: 'Contrast', hint: 'high contrast', tone: 'dark', group: 'Default', swatch: ['#000000', '#0b0b0b', '#ffd23f', '#6cc4ff'] },
  { id: 'ctp-latte', label: 'Latte', hint: 'light', tone: 'light', group: 'Catppuccin', swatch: ['#dce0e8', '#eff1f5', '#8839ef', '#fe640b'] },
  { id: 'ctp-frappe', label: 'Frappé', hint: 'dark', tone: 'dark', group: 'Catppuccin', swatch: ['#292c3c', '#303446', '#ca9ee6', '#ef9f76'] },
  { id: 'ctp-macchiato', label: 'Macchiato', hint: 'dark', tone: 'dark', group: 'Catppuccin', swatch: ['#1e2030', '#24273a', '#c6a0f6', '#f5a97f'] },
  { id: 'ctp-mocha', label: 'Mocha', hint: 'dark', tone: 'dark', group: 'Catppuccin', swatch: ['#181825', '#1e1e2e', '#cba6f7', '#fab387'] },
  { id: 'one-dark', label: 'One Dark', hint: 'dark', tone: 'dark', group: 'Popular', swatch: ['#21252b', '#282c34', '#61afef', '#d19a66'] },
  { id: 'dracula', label: 'Dracula', hint: 'dark', tone: 'dark', group: 'Popular', swatch: ['#21222c', '#282a36', '#bd93f9', '#ffb86c'] },
  { id: 'tokyo-night', label: 'Tokyo Night', hint: 'dark', tone: 'dark', group: 'Popular', swatch: ['#16161e', '#1a1b26', '#7aa2f7', '#ff9e64'] },
  { id: 'nord', label: 'Nord', hint: 'dark', tone: 'dark', group: 'Popular', swatch: ['#2e3440', '#3b4252', '#88c0d0', '#d08770'] },
  { id: 'github-light', label: 'Light', hint: 'light', tone: 'light', group: 'GitHub', swatch: ['#f6f8fa', '#ffffff', '#0969da', '#bc4c00'] },
  { id: 'github-dark', label: 'Dark', hint: 'dark', tone: 'dark', group: 'GitHub', swatch: ['#010409', '#0d1117', '#2f81f7', '#db6d28'] },
  { id: 'gruvbox-light', label: 'Light', hint: 'light', tone: 'light', group: 'Gruvbox', swatch: ['#f2e5bc', '#fbf1c7', '#af3a03', '#af3a03'] },
  { id: 'gruvbox-dark', label: 'Dark', hint: 'dark', tone: 'dark', group: 'Gruvbox', swatch: ['#1d2021', '#282828', '#fe8019', '#fe8019'] },
  { id: 'solarized-light', label: 'Light', hint: 'light', tone: 'light', group: 'Solarized', swatch: ['#eee8d5', '#fdf6e3', '#268bd2', '#cb4b16'] },
  { id: 'solarized-dark', label: 'Dark', hint: 'dark', tone: 'dark', group: 'Solarized', swatch: ['#00212b', '#002b36', '#268bd2', '#cb4b16'] },
  { id: 'rose-pine-dawn', label: 'Dawn', hint: 'light', tone: 'light', group: 'Rosé Pine', swatch: ['#f2e9e1', '#fffaf3', '#907aa9', '#d7827e'] },
  { id: 'rose-pine', label: 'Main', hint: 'dark', tone: 'dark', group: 'Rosé Pine', swatch: ['#191724', '#1f1d2e', '#c4a7e7', '#ebbcba'] },
  { id: 'hacker', label: 'Hacker', hint: 'green phosphor', tone: 'dark', group: 'Fun', swatch: ['#050805', '#0b120b', '#39ff14', '#ffb000'] },
  { id: 'synthwave', label: 'Synthwave', hint: 'neon dusk', tone: 'dark', group: 'Fun', swatch: ['#171229', '#221a3d', '#ff7edb', '#36f9f6'] },
  { id: 'blueprint', label: 'Blueprint', hint: 'architect’s plan', tone: 'dark', group: 'Fun', swatch: ['#123766', '#174780', '#ffffff', '#ffb3a1'] },
  { id: 'quest', label: 'Quest', hint: '8-bit RPG', tone: 'dark', group: 'Fun', swatch: ['#0b0e2c', '#122070', '#ffd84a', '#6cff4a'] },
]
/** Their words in the window's language (names stay as they are). */
export const THEMES: Theme[] = LIST.map((th) => ({ ...th, label: t(th.label), hint: t(th.hint), group: th.group && t(th.group) }))

const dark = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null

export const theme = $state({ choice: 'system' })

/** The theme shown for a choice: `system` (or one no longer known) is Paper or Graphite, as macOS. */
const resolve = (choice: string) => (choice === 'system' || !THEMES.some((th) => th.id === choice) ? (dark?.matches ? 'graphite' : 'paper') : choice)

/** A theme shown for a moment (hovered in the Theme menu), over the chosen one; null: none. */
let previewed: string | null = null

function apply() {
  document.documentElement.dataset.theme = resolve(previewed ?? theme.choice)
}

/** Shows a theme on the whole window without choosing it; null puts the chosen one back. */
export function previewTheme(id: string | null) {
  if (id === previewed) return
  previewed = id
  apply()
}

/**
 * On opening, the read snapshot (main.ts): the theme from config.toml, then the changes from
 * macOS. A theme chosen before it was in config.toml (stored with the layout) moves there.
 */
export function initTheme() {
  const fromFile = live.snapshot?.appearance?.theme
  const legacy = pref<string>('theme', 'system')
  theme.choice = fromFile ?? legacy
  apply()
  dark?.addEventListener('change', apply)
  if (fromFile === 'system' && legacy !== 'system') setTheme(legacy)
}

/** Follows config.toml: a theme saved from Settings, or the file re-read. */
export function watchTheme() {
  return $effect.root(() => {
    $effect(() => {
      const chosen = live.snapshot?.appearance?.theme
      if (chosen && chosen !== theme.choice) {
        theme.choice = chosen
        apply()
      }
    })
  })
}

export function setTheme(id: string) {
  previewed = null
  theme.choice = id
  apply()
  void saveNow(['appearance', 'theme'], id)
}
