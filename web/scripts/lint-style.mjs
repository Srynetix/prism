#!/usr/bin/env node
/**
 * The style lint (`pnpm --dir web lint:style`, part of `pnpm check`): one grammar for the whole
 * interface, kept. It refuses, in the components' styles and the stylesheets:
 * - a `font-size` in px: a `--fs-*` token (app.css) instead;
 * - a `border-radius` in px: a `--r*` token (follows the theme's `--round`); 0, 50%, 999px stay;
 * - a `z-index` of 10 or more: a `--z-*` layer; a component's own stacking stays under 10;
 * - a media query on another width than the interface's scale (app.css, « Widths »):
 *   `(max-width: 719.98px)` or `(max-width: 1199.98px)` only (not the range syntax `(width < …)`,
 *   unknown to older webviews); a block or a panel follows its own width with
 *   `@container` instead;
 * - a shared primitive defined again in a component (`.btn { … }`, `.pill:hover { … }`): they live
 *   in app.css alone; placing one is fine (`.row .pill { margin-left: auto; }`).
 * Some files draw on their own (the boot, the pet, a theme's scene): `FREE` lets them be.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('../src/', import.meta.url))

/** Drawings with their own measures: sizes, radii and layers of their own. */
const FREE = [
  'components/Boot.svelte',
  'components/ThemeScene.svelte',
  'components/Screensaver.svelte',
  'components/Confetti.svelte',
  'components/Logo.svelte',
  'components/PrismLogo.svelte',
  'remote/Gate.svelte',
  'blocks/fun/',
]
/** app.css defines the primitives; its @font-face and themes are not components. */
const PRIMITIVES_HOME = ['app.css']
const PRIMITIVES = ['btn', 'pill', 'seg', 'empty', 'err']

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name)
    if (statSync(path).isDirectory()) {
      if (name !== 'node_modules' && name !== 'bindings') walk(path, out)
    } else if (/\.(svelte|css)$/.test(name)) out.push(path)
  }
  return out
}

/** The style text of a file, with its line offset: a .svelte's `<style>` blocks, a .css whole. */
function styles(path, text) {
  if (path.endsWith('.css')) return [{ css: text, line: 1 }]
  const out = []
  for (const m of text.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
    const before = text.slice(0, m.index + m[0].indexOf('>') + 1)
    out.push({ css: m[1], line: before.split('\n').length })
  }
  return out
}

/** Comments blanked, lines kept. */
const uncomment = (css) => css.replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, ' '))

const problems = []
const say = (file, line, msg) => problems.push(`${file}:${line}  ${msg}`)

for (const path of walk(ROOT)) {
  const file = relative(ROOT, path)
  if (file.endsWith('.test.ts')) continue
  const free = FREE.some((f) => file === f || (f.endsWith('/') && file.startsWith(f)))
  const text = readFileSync(path, 'utf8')
  for (const { css: raw, line: offset } of styles(path, text)) {
    const css = uncomment(raw)
    const lineAt = (i) => offset + css.slice(0, i).split('\n').length - 1
    if (!free) {
      for (const m of css.matchAll(/font-size\s*:\s*([^;}\n]*)/g)) {
        if (/(^|[\s(,])-?[\d.]+px/.test(m[1])) say(file, lineAt(m.index), `font-size ${m[1].trim()}: a --fs-* token`)
      }
      for (const m of css.matchAll(/border(?:-(?:top|bottom)-(?:left|right))?-radius\s*:\s*([^;}\n]*)/g)) {
        const raw = m[1].replace(/var\([^()]*(?:\([^()]*\)[^()]*)*\)/g, '').replace(/calc\([^()]*(?:\([^()]*\)[^()]*)*\)/g, '')
        const px = [...raw.matchAll(/(-?[\d.]+)px/g)].map((x) => Number(x[1])).filter((n) => n !== 0 && n < 999)
        if (px.length) say(file, lineAt(m.index), `border-radius ${m[1].trim()}: a --r* token`)
      }
      for (const m of css.matchAll(/@media\s+([^{]*)\{/g)) {
        for (const w of m[1].matchAll(/\(\s*(?:min-width|max-width|width)\b[^)]*\)/g)) {
          if (!/^\(\s*max-width\s*:\s*(719|1199)\.98px\s*\)$/.test(w[0])) say(file, lineAt(m.index), `@media ${w[0]}: the scale's (max-width: 719.98px) or (max-width: 1199.98px), or an @container`)
        }
      }
      for (const m of css.matchAll(/z-index\s*:\s*(-?\d+)/g)) {
        if (Number(m[1]) >= 10) say(file, lineAt(m.index), `z-index ${m[1]}: a --z-* layer (a component's own stacking stays under 10)`)
      }
    }
    if (!PRIMITIVES_HOME.includes(file)) {
      // Each rule's selectors: the text before a `{` back to the previous `}`, `{` or `;`.
      for (const m of css.matchAll(/([^{};]+)\{/g)) {
        const sel = m[1].trim()
        if (sel.startsWith('@')) continue
        for (const one of sel.split(',')) {
          const s = one.trim().replace(/^:global\((.*)\)$/, '$1')
          const hit = PRIMITIVES.find((p) => new RegExp(`^\\.${p}(?::[\\w-]+(?:\\([^)]*\\))?)*$`).test(s))
          if (hit) say(file, lineAt(m.index + m[0].indexOf(m[1].trimStart())), `.${hit} is defined in app.css only: place it, or use a variant there`)
        }
      }
    }
  }
}

if (problems.length) {
  console.error(problems.join('\n'))
  console.error(`\n${problems.length} style problem(s): see web/scripts/lint-style.mjs`)
  process.exit(1)
}
console.log('style lint: ok')
