import { execSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync, rmSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

/**
 * Prism version: the one in `src-tauri/Cargo.toml` (single source, which Tauri picks up too), and the
 * commit the app is built from, followed by `+` when tracked files have uncommitted changes.
 * Read when Vite starts (build, or `pnpm dev`).
 */
const cargo = readFileSync(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8')
const version = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1] ?? '0.0.0'
const git = (args: string) => {
  try {
    return execSync(`git ${args}`, { cwd: new URL('..', import.meta.url), stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim()
  } catch {
    return ''
  }
}
const commit = git('rev-parse --short HEAD')
const dirty = !!commit && git('status --porcelain --untracked-files=no') !== ''

const prism = fileURLToPath(new URL('..', import.meta.url))

/**
 * The browser preview's snapshot (`public/dev-snapshot.json`, real data when dumped from a running
 * app) is for `pnpm dev` only: Vite copies `public/` into `dist/`, which Tauri bundles into the
 * app, so a build drops it.
 */
const noDevSnapshot = {
  name: 'prism-no-dev-snapshot',
  apply: 'build' as const,
  closeBundle() {
    rmSync(fileURLToPath(new URL('dist/dev-snapshot.json', import.meta.url)), { force: true })
  },
}

/**
 * The two pages are built one after the other (`pnpm build`: `--mode window`, then `--mode
 * remote`), each cut on its own: built together, the code they share was cut into a chunk per
 * module the phone loads lazily, which the window then fetched one by one at its start (33 → 179
 * files). Without a mode (`vite build`), both at once.
 */
const pages = { window: { main: 'index.html' }, remote: { remote: 'remote.html' } } as const
const inputs = (mode: string): Record<string, string> =>
  Object.fromEntries(Object.entries(mode in pages ? pages[mode as keyof typeof pages] : { ...pages.window, ...pages.remote }).map(([name, file]) => [name, fileURLToPath(new URL(file, import.meta.url))]))

/**
 * The browser preview's modules (`pnpm dev` only): those of `PRISM_MODULES_DIR`, one folder per
 * module, served as a phone's server serves them (`/modules/index.json`, `/modules/<id>/<path>` of
 * its web folder; `prism_core::modules`, simplified: no manifest check).
 */
const modulesDir = process.env.PRISM_MODULES_DIR?.trim() ? resolve(prism, process.env.PRISM_MODULES_DIR.trim()) : null
const previewModules = {
  name: 'prism-preview-modules',
  apply: 'serve' as const,
  configureServer(server: { middlewares: { use: (fn: (req: { url?: string }, res: { statusCode: number; setHeader: (k: string, v: string) => void; end: (b?: string | Buffer) => void }, next: () => void) => void) => void } }) {
    server.middlewares.use((req, res, next) => {
      const url = req.url?.split('?')[0] ?? ''
      if (!modulesDir || !url.startsWith('/modules/')) return next()
      let rest: string
      try {
        rest = decodeURIComponent(url.slice('/modules/'.length))
      } catch {
        return ((res.statusCode = 404), res.end())
      }
      res.setHeader('cache-control', 'no-store')
      if (rest === 'index.json') {
        const ids = readdirSync(modulesDir, { withFileTypes: true }).filter((e) => e.isDirectory() && existsSync(resolve(modulesDir, e.name, 'module.toml'))).map((e) => e.name)
        res.setHeader('content-type', 'application/json')
        return res.end(JSON.stringify(ids.map((id) => ({ id, version: null, description: null, requires: null, web: existsSync(resolve(modulesDir, id, 'web/defs.js')), backend: false, refused: null, capabilities: [], trusted: !existsSync(resolve(modulesDir, id, '.untrusted')), changes: existsSync(resolve(modulesDir, id, '.untrusted')) ? ['+ module ' + id, '+ capability git'] : [], trusted_version: null, fingerprint: 'preview', pack: null }))))
      }
      const [id, ...path] = rest.split('/')
      // As the core serves them: a plain folder name, a path without `.`, `..` nor empty parts.
      if (!/^[A-Za-z0-9_-]+$/.test(id) || path.some((p) => p === '' || p === '.' || p === '..' || p.includes('\\'))) return ((res.statusCode = 404), res.end())
      const web = resolve(modulesDir, id, 'web')
      const file = resolve(web, ...path)
      if (!file.startsWith(web + '/') || !existsSync(file)) return ((res.statusCode = 404), res.end())
      res.setHeader('content-type', file.endsWith('.js') ? 'text/javascript' : 'application/octet-stream')
      res.end(readFileSync(file))
    })
  },
}

// Vite serves the frontend during `tauri dev`; in a build, Tauri bundles `dist/`.
export default defineConfig(({ mode }) => ({
  plugins: [svelte(), noDevSnapshot, previewModules],
  // Tests read the stylesheets as written (`?raw`): vitest otherwise gives it empty.
  test: { css: { include: [/\.css\b/] } },
  define: {
    __PRISM_VERSION__: JSON.stringify(version),
    __PRISM_COMMIT__: JSON.stringify(commit ? `${commit}${dirty ? '+' : ''}` : ''),
  },
  clearScreen: false,
  server: { host: '127.0.0.1', port: 5173, strictPort: true, fs: { allow: [prism] } },
  // The window, and the remote's page a phone opens (`remote.html`, served by `prism-remote`).
  // The second page builds next to the first one.
  // The window's chunk (1.9 MB) is read from the app's bundle, not a network: evaluating 430 KB
  // less was 4 ms at start (measured), not worth splitting it; the phone's large ones (its
  // terminal, the help) load on demand. The warning stays for a chunk beyond that.
  build: { target: 'safari16', emptyOutDir: mode !== 'remote', chunkSizeWarningLimit: 2048, rollupOptions: { input: inputs(mode) } },
}))
