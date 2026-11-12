// Builds a Prism module's web part: `vite build` with `moduleConfig(id, root)` as its config.
//
//   // vite.config.mts
//   import { moduleConfig } from '@prism/sdk/vite'
//   export default moduleConfig('hello', import.meta.dirname)
//
// Two entries, `src/defs.ts` and `src/components.ts`, become `dist/<id>/web/defs.js` and
// `components.js`; `module.toml` goes next to them, and the back end when built
// (`backend/target/wasm32-wasip2/release/prism_module_<id>.wasm` → `backend.wasm`).
//
// Svelte, its runtime and Prism (`@prism/sdk`, `@prism/sdk/words`) are left out: each becomes a
// read of `globalThis.__prismHost` at run time, Prism's own instance. An import Prism does not
// give fails here, at the build, not in Prism: another Svelte module, Prism's sources, a file out
// of the module's folder. The module is compiled with its own Svelte, whose minor version must be
// Prism's (`exports.json`).
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'
import { pathToFileURL } from 'node:url'

/** What Prism gives a module: the contract, its Svelte, the names of `@prism/sdk`. */
export const PRISM = JSON.parse(readFileSync(new URL('./exports.json', import.meta.url), 'utf8'))

const HOST = '\0prism-host:'
const minor = (v) => v.split('.').slice(0, 2).join('.')

/** The names a host module gives, for the build to know them (rolldown has no synthetic exports). */
async function names(spec, req) {
  if (spec in PRISM.specifiers) return PRISM.specifiers[spec]
  const pkg = req('svelte/package.json')
  const entry = pkg.exports[spec === 'svelte' ? '.' : `./${spec.slice('svelte/'.length)}`]
  if (!entry) throw new Error(`${spec}: not a module of Svelte`)
  const file = resolve(dirname(req.resolve('svelte/package.json')), entry.browser ?? entry.default)
  return Object.keys(await import(pathToFileURL(file).href))
}

/** The externals: what Prism gives, read from `__prismHost`; what it does not, refused. */
function host(root, req) {
  const inside = (file) => {
    const rel = relative(root, file)
    return !rel.startsWith('..') && !isAbsolute(rel)
  }
  return {
    name: 'prism-module-host',
    enforce: 'pre',
    resolveId(id) {
      if (id === 'svelte' || id.startsWith('svelte/')) {
        if (!PRISM.svelteModules.includes(id)) this.error(`${id}: Prism gives modules ${PRISM.svelteModules.join(', ')} only`)
        return `${HOST}${id}.js`
      }
      if (id in PRISM.specifiers) return `${HOST}${id}.js`
      if (id.startsWith('$prism/') || id.startsWith('@prism/')) this.error(`${id}: a module reaches Prism through @prism/sdk only`)
    },
    async load(id) {
      if (id.startsWith(HOST)) {
        const spec = id.slice(HOST.length, -'.js'.length)
        const all = await names(spec, req)
        return [
          `const m = globalThis.__prismHost.get(${JSON.stringify(spec)});`,
          // Aliased: some of Svelte's are reserved words (`if`, `each`).
          ...all.filter((n) => n !== 'default').map((n, i) => `const _${i} = m[${JSON.stringify(n)}];\nexport { _${i} as ${n} };`),
        ].join('\n')
      }
      // Its own files and its dependencies: not Prism's sources, nor any file elsewhere.
      const file = id.split('?')[0]
      if (isAbsolute(file) && !inside(file) && !file.split(sep).includes('node_modules') && !id.startsWith('\0')) {
        this.error(`${relative(root, file)}: out of the module's folder (a module reaches Prism through @prism/sdk only)`)
      }
    },
  }
}

/** A module's build: `id`, its folder (`src/defs.ts`, `src/components.ts`, `module.toml`). */
export async function moduleConfig(id, root) {
  for (const f of ['src/defs.ts', 'src/components.ts', 'module.toml']) if (!existsSync(resolve(root, f))) throw new Error(`${id}: no ${f}`)
  // Its own Svelte and Vite plugin (its devDependencies), not Prism's.
  const req = createRequire(resolve(root, 'package.json'))
  const svelteVersion = req('svelte/package.json').version
  if (minor(svelteVersion) !== minor(PRISM.svelte)) {
    throw new Error(`${id}: Svelte ${svelteVersion}, Prism runs ${PRISM.svelte}: a module is compiled with Prism's minor (svelte@~${minor(PRISM.svelte)}.0)`)
  }
  const { svelte } = await import(pathToFileURL(req.resolve('@sveltejs/vite-plugin-svelte')).href)
  const out = resolve(root, 'dist', id)
  return {
    root,
    configFile: false,
    plugins: [
      host(root, req),
      // Runes: a component in legacy mode would turn Prism's whole runtime to it.
      svelte({ configFile: false, compilerOptions: { css: 'injected', runes: true } }),
      {
        name: 'prism-module-files',
        writeBundle() {
          mkdirSync(out, { recursive: true })
          copyFileSync(resolve(root, 'module.toml'), resolve(out, 'module.toml'))
          const wasm = resolve(root, 'backend/target/wasm32-wasip2/release', `prism_module_${id.replaceAll('-', '_')}.wasm`)
          if (existsSync(wasm)) copyFileSync(wasm, resolve(out, 'backend.wasm'))
        },
      },
    ],
    define: { __SVELTE__: JSON.stringify(svelteVersion), __PRISM_CONTRACT__: JSON.stringify(PRISM.contract) },
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
