/**
 * Prism version and the commit it was built from, injected by Vite (vite.config.ts). Outside this
 * config (another dev server), `dev` and no commit.
 */
declare const __PRISM_VERSION__: string
declare const __PRISM_COMMIT__: string

export const VERSION = typeof __PRISM_VERSION__ === 'string' ? __PRISM_VERSION__ : 'dev'
/** Short hash; `+` at the end: built with uncommitted changes. Empty without git. */
export const COMMIT = typeof __PRISM_COMMIT__ === 'string' ? __PRISM_COMMIT__ : ''
/** `1.0.0 · 3e11fc2`. */
export const versionLabel = () => (COMMIT ? `${VERSION} · ${COMMIT}` : VERSION)
