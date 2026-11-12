import { copyFileSync, existsSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'
import type { Plugin, UserConfig } from 'vite'

/**
 * Builds a module (a team's blocks Prism loads at run time) with Prism's own toolchain:
 *
 *   pnpm --dir web exec vite build --config ../examples/modules/<name>/vite.config.mts
 *
 * Two entries, `defs.js` and `components.js`, in `dist/<id>/web/`, `module.toml` next to it.
 * Svelte, its runtime and Prism (`@prism/sdk`, `@prism/sdk/words`) are left out: each becomes a
 * read of `globalThis.__prismHost` at run time, Prism's own instance (`web/src/modules/host.ts`).
 * The module is compiled with Prism's Svelte, whose minor version it must then match.
 */
const here = dirname(fileURLToPath(import.meta.url))
const web = resolve(here, '../../web')
const req = createRequire(resolve(web, 'package.json'))
const { svelte } = (await import(pathToFileURL(req.resolve('@sveltejs/vite-plugin-svelte')).href)) as typeof import('@sveltejs/vite-plugin-svelte')
export const SVELTE: string = (req('svelte/package.json') as { version: string }).version

const HOST = '\0prism-host:'
const SDK: Record<string, string> = { '@prism/sdk': 'src/modules/sdk.ts', '@prism/sdk/words': 'src/modules/sdk-words.ts' }
/** Svelte's modules Prism gives at run time (`SVELTE_MODULES` of host.ts): another one would fail there, so the build refuses it. */
const GIVEN = new Set([...readFileSync(resolve(web, 'src/modules/host.ts'), 'utf8').matchAll(/^\s+(?:svelte|'(svelte\/[^']+)'):/gm)].map((m) => m[1] ?? 'svelte'))

/** The names a host module gives, for the build to know them (rolldown has no synthetic exports). */
async function names(spec: string): Promise<string[]> {
  if (spec in SDK) {
    const src = readFileSync(resolve(web, SDK[spec]), 'utf8')
    return [...src.matchAll(/^export\s*\{([^}]*)\}/gm)].flatMap((m) => m[1].split(',').map((p) => p.trim().split(/\s+as\s+/).pop()!.trim()).filter(Boolean))
  }
  const pkg = req('svelte/package.json') as { exports: Record<string, Record<string, string>> }
  const entry = pkg.exports[spec === 'svelte' ? '.' : `./${spec.slice('svelte/'.length)}`]
  if (!entry) throw new Error(`${spec}: not a module of Svelte`)
  const file = resolve(dirname(req.resolve('svelte/package.json')), entry.browser ?? entry.default)
  return Object.keys(await import(pathToFileURL(file).href))
}

const host: Plugin = {
  name: 'prism-module-host',
  enforce: 'pre',
  resolveId(id) {
    if (id === 'svelte' || id.startsWith('svelte/')) {
      if (!GIVEN.has(id)) this.error(`${id}: Prism gives modules ${[...GIVEN].join(', ')} only`)
      return `${HOST}${id}.js`
    }
    if (id in SDK) return `${HOST}${id}.js`
    if (id.startsWith('$prism/') || id.startsWith('@prism/')) this.error(`${id}: a module reaches Prism through @prism/sdk only`)
  },
  async load(id) {
    if (!id.startsWith(HOST)) return
    const spec = id.slice(HOST.length, -'.js'.length)
    const all = await names(spec)
    return [
      `const m = globalThis.__prismHost.get(${JSON.stringify(spec)});`,
      // Aliased: some of Svelte's are reserved words (`if`, `each`).
      ...all.filter((n) => n !== 'default').map((n, i) => `const _${i} = m[${JSON.stringify(n)}];\nexport { _${i} as ${n} };`),
    ].join('\n')
  },
}

/** A module's build: `id`, its folder (`src/defs.ts`, `src/components.ts`, `module.toml`). */
export function moduleConfig(id: string, root: string): UserConfig {
  for (const f of ['src/defs.ts', 'src/components.ts', 'module.toml']) if (!existsSync(resolve(root, f))) throw new Error(`${id}: no ${f}`)
  const out = resolve(root, 'dist', id)
  return {
    root,
    configFile: false,
    plugins: [
      host,
      svelte({ configFile: false, compilerOptions: { css: 'injected', runes: true } }),
      {
        name: 'prism-module-manifest',
        writeBundle() {
          copyFileSync(resolve(root, 'module.toml'), resolve(out, 'module.toml'))
        },
      },
    ],
    define: { __SVELTE__: JSON.stringify(SVELTE) },
    build: {
      outDir: resolve(out, 'web'),
      emptyOutDir: true,
      target: 'safari16',
      minify: false,
      rollupOptions: {
        input: { defs: resolve(root, 'src/defs.ts'), components: resolve(root, 'src/components.ts') },
        preserveEntrySignatures: 'strict',
        output: { format: 'es', entryFileNames: '[name].js', chunkFileNames: 'chunks/[name]-[hash].js' },
      },
    },
  }
}
